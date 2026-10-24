package com.plugin.betterdrop_mobile

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.net.wifi.WifiManager
import android.os.Build
import android.os.IBinder
import android.os.PowerManager
import android.os.Process
import androidx.core.app.NotificationCompat
import androidx.core.app.ServiceCompat
import androidx.core.content.ContextCompat

/**
 * Giữ tiến trình BetterDrop sống khi app ở nền hoặc màn hình tắt, để máy chủ nhận file
 * (trong phần Rust) và mDNS vẫn chạy: hiện thông báo thường trực, giữ khoá multicast
 * (nhận gói mDNS) và WifiLock (Wi‑Fi không ngủ).
 *
 * Loại "connectedDevice" (không phải "dataSync"): Android 15+ chỉ cho "dataSync" chạy
 * 6 tiếng mỗi ngày, còn đây là chờ máy khác trong mạng kết nối tới, không giới hạn giờ.
 * Điều kiện của loại này (Android 14+) là khai báo CHANGE_WIFI_MULTICAST_STATE.
 */
class BetterDropService : Service() {
    private var multicastLock: WifiManager.MulticastLock? = null
    private var wifiLock: WifiManager.WifiLock? = null

    companion object {
        private const val CHANNEL_ID = "betterdrop_background"
        private const val NOTIFICATION_ID = 53318
        private const val ACTION_STOP = "com.plugin.betterdrop_mobile.STOP"
        private const val EXTRA_TITLE = "title"
        private const val EXTRA_TEXT = "text"
        private const val EXTRA_STOP = "stopLabel"
        private const val EXTRA_CHANNEL = "channelName"

        /** Gọi khi người dùng bấm "Tắt" trên thông báo. */
        @Volatile var onUserStop: (() -> Unit)? = null
        @Volatile var running = false
            private set

        private var wakeLock: PowerManager.WakeLock? = null

        fun start(context: Context, title: String, text: String, stopLabel: String, channelName: String) {
            val intent = Intent(context, BetterDropService::class.java)
                .putExtra(EXTRA_TITLE, title)
                .putExtra(EXTRA_TEXT, text)
                .putExtra(EXTRA_STOP, stopLabel)
                .putExtra(EXTRA_CHANNEL, channelName)
            ContextCompat.startForegroundService(context, intent)
        }

        fun stop(context: Context) {
            context.stopService(Intent(context, BetterDropService::class.java))
        }

        /** Giữ CPU chạy khi đang nhận file lúc màn hình tắt (tự nhả sau 30 phút). */
        @Synchronized
        fun keepAwake(context: Context, on: Boolean) {
            if (on) {
                if (wakeLock?.isHeld == true) return
                val pm = context.getSystemService(Context.POWER_SERVICE) as PowerManager
                wakeLock = pm.newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "BetterDrop:receive").apply {
                    setReferenceCounted(false)
                    acquire(30 * 60 * 1000L)
                }
            } else {
                wakeLock?.let { if (it.isHeld) it.release() }
                wakeLock = null
            }
        }
    }

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onCreate() {
        super.onCreate()
        val wifi = applicationContext.getSystemService(Context.WIFI_SERVICE) as WifiManager
        multicastLock = wifi.createMulticastLock("betterdrop-background").apply {
            setReferenceCounted(false)
            acquire()
        }
        @Suppress("DEPRECATION")
        wifiLock = wifi.createWifiLock(WifiManager.WIFI_MODE_FULL_HIGH_PERF, "betterdrop-background").apply {
            setReferenceCounted(false)
            acquire()
        }
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (intent?.action == ACTION_STOP) {
            onUserStop?.invoke()
            stopSelf()
            return START_NOT_STICKY
        }
        val title = intent?.getStringExtra(EXTRA_TITLE) ?: "BetterDrop"
        val text = intent?.getStringExtra(EXTRA_TEXT) ?: ""
        val stopLabel = intent?.getStringExtra(EXTRA_STOP) ?: "Tắt"
        val channelName = intent?.getStringExtra(EXTRA_CHANNEL) ?: "BetterDrop"
        val notification = buildNotification(title, text, stopLabel, channelName)
        try {
            ServiceCompat.startForeground(
                this,
                NOTIFICATION_ID,
                notification,
                if (Build.VERSION.SDK_INT >= 29) ServiceInfo.FOREGROUND_SERVICE_TYPE_CONNECTED_DEVICE else 0,
            )
            running = true
        } catch (e: Exception) {
            // Android 12+ không cho bắt đầu khi app không ở màn hình: plugin thử lại khi app được mở.
            stopSelf()
        }
        // Tiến trình bị giết thì phần Rust cũng mất, khởi động lại riêng service là vô ích.
        return START_NOT_STICKY
    }

    /** Vuốt app khỏi danh sách gần đây: thoát hẳn (như "Thoát" trên máy tính). */
    override fun onTaskRemoved(rootIntent: Intent?) {
        super.onTaskRemoved(rootIntent)
        stopSelf()
        Process.killProcess(Process.myPid())
    }

    override fun onDestroy() {
        running = false
        multicastLock?.let { if (it.isHeld) it.release() }
        wifiLock?.let { if (it.isHeld) it.release() }
        keepAwake(this, false)
        ServiceCompat.stopForeground(this, ServiceCompat.STOP_FOREGROUND_REMOVE)
        super.onDestroy()
    }

    private fun buildNotification(title: String, text: String, stopLabel: String, channelName: String): Notification {
        val nm = getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
        if (Build.VERSION.SDK_INT >= 26) {
            // Mức thấp: không kêu, không rung, chỉ nằm trong ngăn thông báo.
            val channel = NotificationChannel(CHANNEL_ID, channelName, NotificationManager.IMPORTANCE_LOW)
            channel.setShowBadge(false)
            nm.createNotificationChannel(channel)
        }
        val flags = PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
        val open = packageManager.getLaunchIntentForPackage(packageName)?.let {
            it.flags = Intent.FLAG_ACTIVITY_SINGLE_TOP
            PendingIntent.getActivity(this, 0, it, flags)
        }
        val stop = PendingIntent.getService(
            this, 1, Intent(this, BetterDropService::class.java).setAction(ACTION_STOP), flags
        )
        return NotificationCompat.Builder(this, CHANNEL_ID)
            .setSmallIcon(R.drawable.ic_stat_betterdrop)
            .setContentTitle(title)
            .setContentText(text)
            .setContentIntent(open)
            .addAction(0, stopLabel, stop)
            .setOngoing(true)
            .setSilent(true)
            .setShowWhen(false)
            .setCategory(NotificationCompat.CATEGORY_SERVICE)
            .setForegroundServiceBehavior(NotificationCompat.FOREGROUND_SERVICE_IMMEDIATE)
            .build()
    }
}
