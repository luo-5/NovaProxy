package com.novaproxy.androidvpn

import android.app.Activity
import android.content.Intent
import android.net.VpnService
import androidx.activity.result.ActivityResult
import androidx.core.content.ContextCompat
import app.tauri.annotation.ActivityCallback
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin

@InvokeArg
class StartVpnArgs {
    var mixedPort: Int = 7890
    var ipv6: Boolean = false
    var mtu: Int = 1500
    var dnsServer: String? = null
    var logFile: String? = null
}

@TauriPlugin
class AndroidVpnPlugin(private val activity: Activity) : Plugin(activity) {

    @Command
    fun prepareVpn(invoke: Invoke) {
        val prepareIntent = VpnService.prepare(activity)
        if (prepareIntent == null) {
            invoke.resolve(JSObject().put("granted", true))
        } else {
            startActivityForResult(invoke, prepareIntent, "onVpnPermissionResult")
        }
    }

    @ActivityCallback
    fun onVpnPermissionResult(invoke: Invoke, result: ActivityResult) {
        val granted = result.resultCode == Activity.RESULT_OK
        invoke.resolve(JSObject().put("granted", granted))
    }

    @Command
    fun startVpn(invoke: Invoke) {
        try {
            val args = invoke.parseArgs(StartVpnArgs::class.java)
            if (VpnService.prepare(activity) != null) {
                invoke.reject("VPN permission not yet granted; call prepareVpn first")
                return
            }

            val svcIntent = Intent(activity, NovaProxyVpnService::class.java).apply {
                action = NovaProxyVpnService.ACTION_START
                putExtra(NovaProxyVpnService.EXTRA_MIXED_PORT, args.mixedPort)
                putExtra(NovaProxyVpnService.EXTRA_IPV6, args.ipv6)
                putExtra(NovaProxyVpnService.EXTRA_MTU, args.mtu)
                args.dnsServer?.let { putExtra(NovaProxyVpnService.EXTRA_DNS_SERVER, it) }
                args.logFile?.let { putExtra(NovaProxyVpnService.EXTRA_LOG_FILE, it) }
            }

            ContextCompat.startForegroundService(activity, svcIntent)
            invoke.resolve(JSObject().put("started", true))
        } catch (ex: Exception) {
            invoke.reject("Failed to start VpnService: ${ex.message}")
        }
    }

    @Command
    fun stopVpn(invoke: Invoke) {
        try {
            val svcIntent = Intent(activity, NovaProxyVpnService::class.java).apply {
                action = NovaProxyVpnService.ACTION_STOP
            }
            activity.startService(svcIntent)
            invoke.resolve(JSObject().put("stopped", true))
        } catch (ex: Exception) {
            invoke.reject("Failed to stop VpnService: ${ex.message}")
        }
    }

    @Command
    fun getVpnStatus(invoke: Invoke) {
        val prepared = VpnService.prepare(activity) == null
        val (rx, tx) = NovaProxyVpnService.getTrafficStats()
        val obj = JSObject().apply {
            put("running", NovaProxyVpnService.isRunningFlag.get())
            put("prepared", prepared)
            put("nativeLibDir", activity.applicationInfo.nativeLibraryDir)
            put("rxBytes", rx)
            put("txBytes", tx)
        }
        invoke.resolve(obj)
    }

    @Command
    fun getNativeLibDir(invoke: Invoke) {
        val obj = JSObject().apply {
            put("path", activity.applicationInfo.nativeLibraryDir)
        }
        invoke.resolve(obj)
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        val url = intent.dataString ?: return
        val lower = url.lowercase()
        if (lower.startsWith("novaproxy://") || lower.startsWith("clash://")) {
            trigger("deep-link", JSObject().put("url", url))
        }
    }
}
