package com.plugin.betterdrop_mobile

import android.app.Activity
import android.content.Context
import android.content.Intent
import android.webkit.WebView
import android.media.MediaScannerConnection
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
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSArray
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin

@InvokeArg
class UriArgs {
    lateinit var uri: String
}

@InvokeArg
class PathArgs {
    lateinit var path: String
}

private const val LOCAL_NETWORK = "localNetwork"

/**
 * Android 17 (API 37) chặn app kết nối tới máy trong mạng LAN và nhận gói mDNS
 * ("Local network protection") cho tới khi người dùng cấp quyền ACCESS_LOCAL_NETWORK
 * (nhóm "Thiết bị lân cận"). Thiếu quyền này thì kết nối chỉ treo tới hết giờ.
 */
@TauriPlugin(
    permissions = [Permission(strings = ["android.permission.ACCESS_LOCAL_NETWORK"], alias = LOCAL_NETWORK)]
)
class BetterDropPlugin(private val activity: Activity) : Plugin(activity) {
    private var multicastLock: WifiManager.MulticastLock? = null

    // ---- Nhận file/văn bản từ menu "Chia sẻ" của app khác ----
    private val sharedUris = mutableListOf<String>()
    private var sharedText: String? = null

    override fun load(webView: WebView) {
        super.load(webView)
        handleShare(activity.intent)
    }

    override fun onNewIntent(intent: Intent) {
        handleShare(intent)
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
        sharedUris.clear()
        sharedText = null
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
