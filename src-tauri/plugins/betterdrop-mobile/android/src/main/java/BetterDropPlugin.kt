package com.plugin.betterdrop_mobile

import android.app.Activity
import android.content.Context
import android.content.Intent
import android.webkit.WebView
import android.graphics.Bitmap
import android.graphics.ImageDecoder
import android.media.MediaMetadataRetriever
import android.media.MediaScannerConnection
import android.util.Base64
import java.io.ByteArrayOutputStream
import java.io.File
import android.net.Uri
import android.net.wifi.WifiManager
import android.os.Build
import android.os.Environment
import android.provider.Settings
import android.provider.OpenableColumns
import app.tauri.PermissionState
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.Permission
import app.tauri.annotation.PermissionCallback
import app.tauri.annotation.TauriPlugin
import androidx.appcompat.app.AppCompatActivity
import app.tauri.plugin.Channel
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSArray
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin

@InvokeArg
class UriArgs {
    lateinit var uri: String
}

@InvokeArg
class ThumbArgs {
    lateinit var uri: String
    var video: Boolean = false
}

@InvokeArg
class PathArgs {
    lateinit var path: String
}

@InvokeArg
class BackgroundArgs {
    lateinit var title: String
    lateinit var text: String
    lateinit var stopLabel: String
    lateinit var channelName: String
    var onStop: Channel? = null
}

@InvokeArg
class ChannelArgs {
    var channel: Channel? = null
}

@InvokeArg
class OnArgs {
    var on: Boolean = false
}

private const val LOCAL_NETWORK = "localNetwork"
private const val THUMB_SIDE = 160
private const val NOTIFICATIONS = "notifications"

/**
 * Android 17 (API 37) chặn app kết nối tới máy trong mạng LAN và nhận gói mDNS
 * ("Local network protection") cho tới khi người dùng cấp quyền ACCESS_LOCAL_NETWORK
 * (nhóm "Thiết bị lân cận"). Thiếu quyền này thì kết nối chỉ treo tới hết giờ.
 */
@TauriPlugin(
    permissions = [
        Permission(strings = ["android.permission.ACCESS_LOCAL_NETWORK"], alias = LOCAL_NETWORK),
        Permission(strings = ["android.permission.POST_NOTIFICATIONS"], alias = NOTIFICATIONS),
    ]
)
class BetterDropPlugin(private val activity: Activity) : Plugin(activity) {
    private var multicastLock: WifiManager.MulticastLock? = null

    // ---- Nhận file/văn bản từ menu "Chia sẻ" của app khác ----
    private val sharedUris = mutableListOf<String>()
    private var sharedText: String? = null
    private var sharedLink: String? = null

    override fun load(webView: WebView) {
        super.load(webView)
        handleShare(activity.intent)
    }

    override fun onNewIntent(intent: Intent) {
        handleShare(intent)
    }

    // ---- Chạy nền ----

    companion object {
        /** Báo phần Rust app đang hiện trên màn hình hay không (xem watchLifecycle). */
        @Volatile private var lifecycle: Channel? = null
        /** Lần bật chạy nền gần nhất bị Android từ chối (app không ở màn hình): thử lại khi mở app. */
        @Volatile private var pendingStart: BackgroundArgs? = null
    }

    override fun onResume(activity: AppCompatActivity) {
        lifecycle?.send(JSObject().put("foreground", true))
        pendingStart?.let { startService(it) }
    }

    override fun onStop(activity: AppCompatActivity) {
        lifecycle?.send(JSObject().put("foreground", false))
    }

    @Command
    fun watchLifecycle(invoke: Invoke) {
        lifecycle = invoke.parseArgs(ChannelArgs::class.java).channel
        invoke.resolve()
    }

    private fun startService(args: BackgroundArgs) {
        val onStop = args.onStop
        BetterDropService.onUserStop = { onStop?.send(JSObject()) }
        pendingStart = try {
            BetterDropService.start(activity.applicationContext, args.title, args.text, args.stopLabel, args.channelName)
            null
        } catch (e: Exception) {
            args
        }
    }

    /** Bật dịch vụ chạy nền (gọi lại để đổi chữ trên thông báo). `onStop`: người dùng bấm "Tắt". */
    @Command
    fun startBackground(invoke: Invoke) {
        startService(invoke.parseArgs(BackgroundArgs::class.java))
        invoke.resolve()
    }

    @Command
    fun stopBackground(invoke: Invoke) {
        pendingStart = null
        BetterDropService.stop(activity.applicationContext)
        invoke.resolve()
    }

    /** Giữ CPU chạy trong lúc nhận file khi màn hình tắt. */
    @Command
    fun keepAwake(invoke: Invoke) {
        BetterDropService.keepAwake(activity.applicationContext, invoke.parseArgs(OnArgs::class.java).on)
        invoke.resolve()
    }

    /** Android 13+: xin quyền hiện thông báo. Trả về {granted}. */
    @Command
    fun requestNotifications(invoke: Invoke) {
        if (Build.VERSION.SDK_INT < 33 || getPermissionState(NOTIFICATIONS) == PermissionState.GRANTED) {
            invoke.resolve(JSObject().put("granted", true))
            return
        }
        requestPermissionForAlias(NOTIFICATIONS, invoke, "notificationsCallback")
    }

    @PermissionCallback
    fun notificationsCallback(invoke: Invoke) {
        invoke.resolve(JSObject().put("granted", getPermissionState(NOTIFICATIONS) == PermissionState.GRANTED))
    }

    private fun handleShare(intent: Intent?) {
        if (intent == null) return
        when (intent.action) {
            Intent.ACTION_SEND -> {
                val uri = if (Build.VERSION.SDK_INT >= 33) intent.getParcelableExtra(Intent.EXTRA_STREAM, Uri::class.java)
                          else @Suppress("DEPRECATION") intent.getParcelableExtra<Uri>(Intent.EXTRA_STREAM)
                if (uri != null) sharedUris.add(uri.toString())
                else intent.getStringExtra(Intent.EXTRA_TEXT)?.let { sharedText = it }
            }
            Intent.ACTION_SEND_MULTIPLE -> {
                val uris = if (Build.VERSION.SDK_INT >= 33) intent.getParcelableArrayListExtra(Intent.EXTRA_STREAM, Uri::class.java)
                           else @Suppress("DEPRECATION") intent.getParcelableArrayListExtra<Uri>(Intent.EXTRA_STREAM)
                uris?.forEach { u -> sharedUris.add(u.toString()) }
            }
            // Quét mã QR ghép nối bằng camera thường: Android mở app với betterdrop://pair?…
            Intent.ACTION_VIEW -> {
                val data = intent.data ?: return
                if (data.scheme != "betterdrop" || data.host != "pair") return
                sharedLink = data.toString()
            }
            else -> return
        }
        // Không xử lý lại cùng intent khi activity được tạo lại (xoay màn hình…).
        intent.action = null
    }

    /** Lấy (và xoá) các mục được chia sẻ vào. */
    @Command
    fun takeShared(invoke: Invoke) {
        val ret = JSObject()
        ret.put("uris", JSArray.from(sharedUris.toTypedArray()))
        sharedText?.let { ret.put("text", it) }
        sharedLink?.let { ret.put("link", it) }
        sharedUris.clear()
        sharedText = null
        sharedLink = null
        invoke.resolve(ret)
    }

    /** Xin quyền truy cập mạng cục bộ (chỉ cần từ Android 17). Trả về {granted}. */
    @Command
    fun requestLocalNetwork(invoke: Invoke) {
        if (Build.VERSION.SDK_INT < 37 || getPermissionState(LOCAL_NETWORK) == PermissionState.GRANTED) {
            val ret = JSObject()
            ret.put("granted", true)
            invoke.resolve(ret)
            return
        }
        requestPermissionForAlias(LOCAL_NETWORK, invoke, "localNetworkCallback")
    }

    @PermissionCallback
    fun localNetworkCallback(invoke: Invoke) {
        val ret = JSObject()
        ret.put("granted", getPermissionState(LOCAL_NETWORK) == PermissionState.GRANTED)
        invoke.resolve(ret)
    }

    /**
     * Android bỏ qua gói tin multicast (mDNS) nếu app không giữ khoá này.
     * Gọi lại sau khi mạng thay đổi: khoá cũ được nhả ra và xin khoá mới.
     */
    @Command
    fun acquireMulticastLock(invoke: Invoke) {
        multicastLock?.let { if (it.isHeld) it.release() }
        val wifi = activity.applicationContext.getSystemService(Context.WIFI_SERVICE) as WifiManager
        multicastLock = wifi.createMulticastLock("betterdrop").apply {
            setReferenceCounted(false)
            acquire()
        }
        invoke.resolve()
    }

    /** Tên hiển thị và kích thước của một file `content://` do bộ chọn file trả về. */
    @Command
    fun fileInfo(invoke: Invoke) {
        val args = invoke.parseArgs(UriArgs::class.java)
        val uri = Uri.parse(args.uri)
        var name: String? = null
        var size = -1L
        try {
            activity.contentResolver.query(
                uri, arrayOf(OpenableColumns.DISPLAY_NAME, OpenableColumns.SIZE), null, null, null
            )?.use { c ->
                if (c.moveToFirst()) {
                    val ni = c.getColumnIndex(OpenableColumns.DISPLAY_NAME)
                    val si = c.getColumnIndex(OpenableColumns.SIZE)
                    if (ni >= 0 && !c.isNull(ni)) name = c.getString(ni)
                    if (si >= 0 && !c.isNull(si)) size = c.getLong(si)
                }
            }
        } catch (e: Exception) {
            invoke.reject("Không đọc được thông tin file: ${e.message}")
            return
        }
        val ret = JSObject()
        ret.put("name", name ?: uri.lastPathSegment ?: "file")
        ret.put("size", size)
        invoke.resolve(ret)
    }

    /**
     * Ảnh thu nhỏ JPEG (cạnh dài 160px, base64) của một ảnh (kể cả HEIC) hoặc khung hình
     * gần đầu của một video, để máy nhận xem trước. Chạy ở luồng riêng.
     */
    @Command
    fun mediaThumb(invoke: Invoke) {
        val args = invoke.parseArgs(ThumbArgs::class.java)
        Thread {
            try {
                val uri = if (args.uri.contains("://")) Uri.parse(args.uri) else Uri.fromFile(File(args.uri))
                val bitmap = if (args.video) videoFrame(uri) else imageThumb(uri)
                if (bitmap == null) {
                    invoke.reject("Không tạo được ảnh thu nhỏ")
                    return@Thread
                }
                val small = scaleDown(bitmap, THUMB_SIDE)
                val out = ByteArrayOutputStream()
                small.compress(Bitmap.CompressFormat.JPEG, 70, out)
                val ret = JSObject()
                ret.put("data", Base64.encodeToString(out.toByteArray(), Base64.NO_WRAP))
                invoke.resolve(ret)
            } catch (e: Throwable) {
                invoke.reject("Không tạo được ảnh thu nhỏ: ${e.message}")
            }
        }.start()
    }

    private fun videoFrame(uri: Uri): Bitmap? {
        val r = MediaMetadataRetriever()
        try {
            r.setDataSource(activity.applicationContext, uri)
            // Giây thứ nhất thường đẹp hơn khung hình đầu (hay bị đen); video ngắn hơn thì lấy khung đầu.
            for (at in longArrayOf(1_000_000L, 0L)) {
                val frame = if (Build.VERSION.SDK_INT >= 27) {
                    r.getScaledFrameAtTime(at, MediaMetadataRetriever.OPTION_CLOSEST_SYNC, THUMB_SIDE * 2, THUMB_SIDE * 2)
                } else {
                    r.getFrameAtTime(at, MediaMetadataRetriever.OPTION_CLOSEST_SYNC)
                }
                if (frame != null) return frame
            }
            return null
        } finally {
            r.release()
        }
    }

    /** Android 9+: ImageDecoder đọc được HEIC và tự xoay theo EXIF. */
    private fun imageThumb(uri: Uri): Bitmap? {
        if (Build.VERSION.SDK_INT < 28) return null
        val source = ImageDecoder.createSource(activity.contentResolver, uri)
        return ImageDecoder.decodeBitmap(source) { decoder, info, _ ->
            val side = maxOf(info.size.width, info.size.height)
            decoder.setTargetSampleSize(maxOf(1, side / (THUMB_SIDE * 2)))
            decoder.allocator = ImageDecoder.ALLOCATOR_SOFTWARE
        }
    }

    private fun scaleDown(b: Bitmap, side: Int): Bitmap {
        val longest = maxOf(b.width, b.height)
        if (longest <= side) return b
        val w = maxOf(1, b.width * side / longest)
        val h = maxOf(1, b.height * side / longest)
        return Bitmap.createScaledBitmap(b, w, h, true)
    }

    /** Thư mục Download công khai (hiện trong app Files). */
    @Command
    fun downloadDir(invoke: Invoke) {
        val dir = Environment.getExternalStoragePublicDirectory(Environment.DIRECTORY_DOWNLOADS)
        val ret = JSObject()
        ret.put("path", dir.absolutePath)
        invoke.resolve(ret)
    }

    /** Tên máy do người dùng đặt (Cài đặt > Giới thiệu điện thoại), nếu không có thì tên model. */
    @Command
    fun deviceName(invoke: Invoke) {
        val name = try {
            Settings.Global.getString(activity.contentResolver, Settings.Global.DEVICE_NAME)
        } catch (e: Exception) {
            null
        }
        val ret = JSObject()
        ret.put("path", if (name.isNullOrBlank()) "${Build.MANUFACTURER} ${Build.MODEL}" else name)
        invoke.resolve(ret)
    }

    /** Báo hệ thống có file mới để nó hiện ngay trong Files/Ảnh. */
    @Command
    fun scanFile(invoke: Invoke) {
        val args = invoke.parseArgs(PathArgs::class.java)
        MediaScannerConnection.scanFile(activity.applicationContext, arrayOf(args.path), null, null)
        invoke.resolve()
    }
}
