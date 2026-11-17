package dev.xreallab.xr_lab

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import io.flutter.embedding.android.FlutterActivity
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.EventChannel
import io.flutter.plugin.common.MethodChannel

class MainActivity : FlutterActivity() {
    private var network: GlassesNetwork? = null
    private var sink: EventChannel.EventSink? = null
    private var lastHandle: Long? = null

    override fun configureFlutterEngine(flutterEngine: FlutterEngine) {
        super.configureFlutterEngine(flutterEngine)
        val messenger = flutterEngine.dartExecutor.binaryMessenger

        EventChannel(messenger, "xr_lab/rede").setStreamHandler(object : EventChannel.StreamHandler {
            override fun onListen(arguments: Any?, events: EventChannel.EventSink) {
                sink = events
                events.success(lastHandle)
            }

            override fun onCancel(arguments: Any?) {
                sink = null
            }
        })

        MethodChannel(messenger, "xr_lab/servico").setMethodCallHandler { call, result ->
            when (call.method) {
                "iniciar" -> {
                    askNotificationPermission()
                    val i = Intent(this, GlassesService::class.java)
                    if (Build.VERSION.SDK_INT >= 26) startForegroundService(i) else startService(i)
                    result.success(true)
                }
                "parar" -> {
                    stopService(Intent(this, GlassesService::class.java))
                    result.success(true)
                }
                else -> result.notImplemented()
            }
        }

        network = GlassesNetwork(this) { handle ->
            runOnUiThread {
                lastHandle = handle
                sink?.success(handle)
            }
        }.also { it.start() }
    }

    private fun askNotificationPermission() {
        if (Build.VERSION.SDK_INT >= 33 &&
            checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED
        ) {
            requestPermissions(arrayOf(Manifest.permission.POST_NOTIFICATIONS), 1)
        }
    }

    override fun onDestroy() {
        network?.stop()
        stopService(Intent(this, GlassesService::class.java))
        super.onDestroy()
    }
}
