package com.novaproxy.androidvpn

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Intent
import android.content.pm.ServiceInfo
import android.net.ProxyInfo
import android.net.VpnService
import android.os.Build
import android.os.ParcelFileDescriptor
import android.util.Log
import hev.htproxy.TProxyService
import java.io.File
import java.util.concurrent.atomic.AtomicBoolean

class NovaProxyVpnService : VpnService() {

    companion object {
        const val ACTION_START = "com.novaproxy.androidvpn.START"
        const val ACTION_STOP = "com.novaproxy.androidvpn.STOP"
        const val EXTRA_MIXED_PORT = "mixed_port"
        const val EXTRA_IPV6 = "ipv6"
        const val EXTRA_MTU = "mtu"
        const val EXTRA_DNS_SERVER = "dns_server"
        const val EXTRA_LOG_FILE = "log_file"

        private const val TAG = "NovaProxyVpnService"
        private const val CHANNEL_ID = "novaproxy_vpn_channel"
        private const val NOTIFICATION_ID = 1001

        val isRunningFlag = AtomicBoolean(false)

        fun getTrafficStats(): Pair<Long, Long> {
            return try {
                if (!isRunningFlag.get() || !TProxyService.TProxyIsRunning()) {
                    0L to 0L
                } else {
                    val stats = TProxyService.TProxyGetStats()
                    // hev-socks5-tunnel stats array: [tx_packets, tx_bytes, rx_packets, rx_bytes]
                    val tx = if (stats != null && stats.size >= 2) stats[1] else 0L
                    val rx = if (stats != null && stats.size >= 4) stats[3] else 0L
                    rx to tx
                }
            } catch (_: Throwable) {
                0L to 0L
            }
        }
    }

    private var tunInterface: ParcelFileDescriptor? = null
    private var tunnelThread: Thread? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (intent?.action == ACTION_STOP) {
            stopTunnel()
            stopSelf()
            return START_NOT_STICKY
        }

        val mixedPort = intent?.getIntExtra(EXTRA_MIXED_PORT, 7890) ?: 7890
        val ipv6 = intent?.getBooleanExtra(EXTRA_IPV6, false) ?: false
        val mtu = intent?.getIntExtra(EXTRA_MTU, 1500) ?: 1500
        val dnsServer = intent?.getStringExtra(EXTRA_DNS_SERVER) ?: "1.1.1.1"
        val logFile = intent?.getStringExtra(EXTRA_LOG_FILE)
            ?: File(filesDir, "logs/hev-tunnel.log").absolutePath

        promoteForeground(mixedPort)
        startTunnel(mixedPort, ipv6, mtu, dnsServer, logFile)
        return START_STICKY
    }

    private fun promoteForeground(mixedPort: Int) {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            val nm = getSystemService(NotificationManager::class.java)
            val channel = NotificationChannel(
                CHANNEL_ID,
                "NovaProxy VPN",
                NotificationManager.IMPORTANCE_LOW
            )
            nm?.createNotificationChannel(channel)
        }

        val launchIntent = packageManager.getLaunchIntentForPackage(packageName)
        val pendingFlags = PendingIntent.FLAG_UPDATE_CURRENT or
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.M) PendingIntent.FLAG_IMMUTABLE else 0
        val contentIntent = launchIntent?.let {
            PendingIntent.getActivity(this, 0, it, pendingFlags)
        }

        val builder = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            Notification.Builder(this, CHANNEL_ID)
        } else {
            @Suppress("DEPRECATION")
            Notification.Builder(this)
        }

        val iconRes = if (applicationInfo.icon != 0) {
            applicationInfo.icon
        } else {
            android.R.drawable.sym_def_app_icon
        }
        builder.setContentTitle("NovaProxy")
            .setContentText("VPN active via 127.0.0.1:$mixedPort")
            .setSmallIcon(iconRes)
            .setOngoing(true)
        if (contentIntent != null) {
            builder.setContentIntent(contentIntent)
        }

        val notification = builder.build()
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
            startForeground(
                NOTIFICATION_ID,
                notification,
                ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE
            )
        } else {
            startForeground(NOTIFICATION_ID, notification)
        }
    }

    private fun startTunnel(
        mixedPort: Int,
        ipv6: Boolean,
        mtu: Int,
        dnsServer: String,
        logFilePath: String
    ) {
        stopTunnel()

        try {
            val builder = Builder()
                .setSession("NovaProxy")
                .setMtu(mtu)
                .addAddress("172.19.0.1", 30)
                .addRoute("0.0.0.0", 0)
                .addDnsServer(dnsServer)

            if (ipv6) {
                builder.addAddress("fdfe:dcba:9876::1", 126)
                builder.addRoute("::", 0)
            }

            try {
                // Exempt NovaProxy and child proxy binaries to eliminate loopback routing loops.
                builder.addDisallowedApplication(packageName)
            } catch (ex: Exception) {
                Log.w(TAG, "addDisallowedApplication failed: ${ex.message}")
            }

            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
                builder.setMetered(false)
                builder.setHttpProxy(ProxyInfo.buildDirectProxy("127.0.0.1", mixedPort))
            }

            val pfd = builder.establish()
            if (pfd == null) {
                Log.e(TAG, "VpnService.Builder.establish() returned null (missing consent)")
                stopSelf()
                return
            }
            tunInterface = pfd

            val logParent = File(logFilePath).parentFile
            if (logParent != null && !logParent.exists()) {
                logParent.mkdirs()
            }

            val tunnelCfg = buildString {
                appendLine("tunnel:")
                appendLine("  mtu: $mtu")
                appendLine("  ipv4: 172.19.0.1")
                if (ipv6) {
                    appendLine("  ipv6: 'fdfe:dcba:9876::1'")
                }
                appendLine("socks5:")
                appendLine("  port: $mixedPort")
                appendLine("  address: '127.0.0.1'")
                appendLine("  udp: 'udp'")
                appendLine("misc:")
                appendLine("  task-stack-size: 81920")
                appendLine("  connect-timeout: 5000")
                appendLine("  read-write-timeout: 60000")
                appendLine("  log-file: '$logFilePath'")
                appendLine("  log-level: 'warn'")
            }

            val cfgFile = File(filesDir, "hev-socks5-tunnel.yml")
            cfgFile.writeText(tunnelCfg, Charsets.UTF_8)

            val rawFd = pfd.fd
            isRunningFlag.set(true)

            val worker = Thread({
                try {
                    Log.i(TAG, "Starting TProxyService on fd=$rawFd port=$mixedPort")
                    TProxyService.TProxyStartService(cfgFile.absolutePath, rawFd)
                } catch (t: Throwable) {
                    Log.e(TAG, "TProxyService failed: ${t.message}", t)
                } finally {
                    isRunningFlag.set(false)
                }
            }, "novaproxy-hev-tunnel")
            worker.isDaemon = true
            worker.start()
            tunnelThread = worker
        } catch (ex: Exception) {
            Log.e(TAG, "Failed to start Android VPN tunnel: ${ex.message}", ex)
            stopTunnel()
            stopSelf()
        }
    }

    private fun stopTunnel() {
        isRunningFlag.set(false)
        try {
            if (TProxyService.TProxyIsRunning()) {
                TProxyService.TProxyStopService()
            }
        } catch (_: Throwable) {
        }
        try {
            tunInterface?.close()
        } catch (_: Throwable) {
        }
        tunInterface = null
        tunnelThread = null
    }

    override fun onRevoke() {
        Log.w(TAG, "VPN permission revoked by OS or another VPN app")
        stopTunnel()
        stopSelf()
        super.onRevoke()
    }

    override fun onDestroy() {
        stopTunnel()
        super.onDestroy()
    }
}
