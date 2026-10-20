package com.plugin.betterdrop_mobile

import android.app.Activity
import android.content.Context
import android.media.MediaScannerConnection
import android.net.Uri
import android.net.wifi.WifiManager
import android.os.Build
import android.os.Environment
import android.provider.Settings
import android.provider.OpenableColumns
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
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

@TauriPlugin
class BetterDropPlugin(private val activity: Activity) : Plugin(activity) {
    private var multicastLock: WifiManager.MulticastLock? = null

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
