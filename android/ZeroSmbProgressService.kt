package com.zerosmb.app

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import androidx.core.app.NotificationCompat

class ZeroSmbProgressService : Service() {
    companion object {
        const val CHANNEL_ID = "zerosmb_transfers"
        const val NOTIFICATION_ID = 7001

        @Volatile
        var instance: ZeroSmbProgressService? = null
    }

    private lateinit var manager: NotificationManager

    override fun onCreate() {
        super.onCreate()
        instance = this
        manager = getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
        createChannel()
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (intent?.action == "stop") {
            stopForegroundCompat()
            stopSelf()
            return START_NOT_STICKY
        }
        val title = intent?.getStringExtra("title") ?: "ZeroSMB"
        val text = intent?.getStringExtra("text") ?: ""
        val current = intent?.getIntExtra("current", 0) ?: 0
        val total = intent?.getIntExtra("total", 0) ?: 0
        val done = intent?.getBooleanExtra("done", false) ?: false
        val notification = buildNotification(title, text, current, total, done)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            startForeground(
                NOTIFICATION_ID,
                notification,
                ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC,
            )
        } else {
            startForeground(NOTIFICATION_ID, notification)
        }
        if (done) {
            manager.notify(NOTIFICATION_ID, notification)
            stopForegroundCompat()
            stopSelf()
        }
        return START_NOT_STICKY
    }

    fun update(title: String, text: String, current: Int, total: Int, done: Boolean) {
        manager.notify(NOTIFICATION_ID, buildNotification(title, text, current, total, done))
        if (done) {
            stopForegroundCompat()
            stopSelf()
        }
    }

    fun finish(title: String, text: String, current: Int, total: Int) {
        update(title, text, current, total, true)
    }

    private fun createChannel() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
        val channel = NotificationChannel(
            CHANNEL_ID,
            "Передачи ZeroSMB",
            NotificationManager.IMPORTANCE_LOW,
        ).apply {
            description = "Скачивание и загрузка файлов"
            setShowBadge(false)
        }
        manager.createNotificationChannel(channel)
    }

    private fun buildNotification(
        title: String,
        text: String,
        current: Int,
        total: Int,
        done: Boolean,
    ): Notification {
        val launch = packageManager.getLaunchIntentForPackage(packageName)
        val pending = if (launch != null) {
            PendingIntent.getActivity(
                this,
                0,
                launch,
                PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
            )
        } else {
            null
        }
        val builder = NotificationCompat.Builder(this, CHANNEL_ID)
            .setSmallIcon(R.drawable.ic_stat_zerosmb)
            .setContentTitle(title)
            .setContentText(if (done) text else if (total > 0) "$current из $total" else text)
            .setStyle(NotificationCompat.BigTextStyle().bigText(text))
            .setOnlyAlertOnce(true)
            .setOngoing(!done)
            .setAutoCancel(done)
            .setPriority(NotificationCompat.PRIORITY_LOW)
        if (pending != null) builder.setContentIntent(pending)
        if (!done) {
            if (total > 0) {
                builder.setProgress(total, current.coerceIn(0, total), false)
            } else {
                builder.setProgress(0, 0, true)
            }
        }
        return builder.build()
    }

    private fun stopForegroundCompat() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.N) {
            stopForeground(STOP_FOREGROUND_DETACH)
        } else {
            @Suppress("DEPRECATION")
            stopForeground(false)
        }
    }

    override fun onDestroy() {
        instance = null
        super.onDestroy()
    }

    override fun onBind(intent: Intent?): IBinder? = null
}
