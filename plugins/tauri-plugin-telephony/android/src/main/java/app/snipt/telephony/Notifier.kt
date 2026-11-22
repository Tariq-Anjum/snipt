package app.snipt.telephony

import android.Manifest
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.app.RemoteInput
import org.json.JSONObject

/**
 * Incoming-message notifications with Mark read / Reply (RemoteInput) /
 * Delete / Copy code actions, handled by [NotificationActionReceiver] with the
 * UI closed. The body shown is the core's redacted body (OTP as bullets); the
 * lock screen gets a generic public version. PendingIntents are explicit and
 * FLAG_IMMUTABLE, except the Reply one, which must be FLAG_MUTABLE so the
 * system can attach the RemoteInput text.
 */
object Notifier {
    const val CHANNEL_MESSAGES = "snipt_messages"
    const val CHANNEL_CODES = "snipt_codes"
    const val KEY_REPLY = "snipt_reply"

    const val ACTION_MARK_READ = "app.snipt.telephony.MARK_READ"
    const val ACTION_DELETE = "app.snipt.telephony.DELETE"
    const val ACTION_COPY_OTP = "app.snipt.telephony.COPY_OTP"
    const val ACTION_REPLY = "app.snipt.telephony.REPLY"

    const val EXTRA_MSG_ID = "msgId"
    const val EXTRA_PROVIDER_ID = "providerId"
    const val EXTRA_THREAD_ID = "threadId"
    const val EXTRA_NOTIF_ID = "notifId"
    const val EXTRA_SIM_SLOT = "simSlot"
    const val EXTRA_TITLE = "title"

    fun canPost(ctx: Context): Boolean =
        Build.VERSION.SDK_INT < 33 ||
            ctx.checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) == PackageManager.PERMISSION_GRANTED

    private fun channels(ctx: Context) {
        val nm = ctx.getSystemService(NotificationManager::class.java) ?: return
        nm.createNotificationChannel(
            NotificationChannel(CHANNEL_MESSAGES, ctx.getString(R.string.snipt_channel_messages), NotificationManager.IMPORTANCE_HIGH)
                .apply { lockscreenVisibility = android.app.Notification.VISIBILITY_PRIVATE }
        )
        nm.createNotificationChannel(
            NotificationChannel(CHANNEL_CODES, ctx.getString(R.string.snipt_channel_codes), NotificationManager.IMPORTANCE_HIGH)
                .apply { lockscreenVisibility = android.app.Notification.VISIBILITY_PRIVATE }
        )
    }

    /** [out] is the JNI ingest result (redacted body, ids, notify flag). */
    fun incoming(ctx: Context, out: JSONObject, providerId: Long, simSlot: Int) {
        if (!out.optBoolean("notify") || !canPost(ctx)) return
        channels(ctx)
        val msgId = out.getLong("msgId")
        val threadId = out.getLong("threadId")
        val hasOtp = out.optBoolean("hasOtp")
        val notifId = (threadId and 0x7FFFFFFF).toInt()
        val title = out.optString("title", "")
        val body = out.optString("body", "")

        fun action(action: String, req: Int, mutable: Boolean = false): PendingIntent {
            val i = Intent(ctx, NotificationActionReceiver::class.java)
                .setAction(action)
                .putExtra(EXTRA_MSG_ID, msgId)
                .putExtra(EXTRA_PROVIDER_ID, providerId)
                .putExtra(EXTRA_THREAD_ID, threadId)
                .putExtra(EXTRA_NOTIF_ID, notifId)
                .putExtra(EXTRA_SIM_SLOT, simSlot)
                .putExtra(EXTRA_TITLE, title)
            val flags = PendingIntent.FLAG_UPDATE_CURRENT or
                if (mutable) PendingIntent.FLAG_MUTABLE else PendingIntent.FLAG_IMMUTABLE
            return PendingIntent.getBroadcast(ctx, notifId * 8 + req, i, flags)
        }

        val channel = if (hasOtp) CHANNEL_CODES else CHANNEL_MESSAGES
        val publicVersion = NotificationCompat.Builder(ctx, channel)
            .setSmallIcon(R.drawable.ic_snipt_stat)
            .setContentTitle(ctx.getString(if (hasOtp) R.string.snipt_public_code else R.string.snipt_public_message))
            .build()

        val b = NotificationCompat.Builder(ctx, channel)
            .setSmallIcon(R.drawable.ic_snipt_stat)
            .setContentTitle(title)
            .setContentText(body)
            .setStyle(NotificationCompat.BigTextStyle().bigText(body))
            .setCategory(NotificationCompat.CATEGORY_MESSAGE)
            .setPriority(NotificationCompat.PRIORITY_HIGH)
            .setVisibility(NotificationCompat.VISIBILITY_PRIVATE)
            .setPublicVersion(publicVersion)
            .setAutoCancel(true)
            .setOnlyAlertOnce(false)

        launchIntent(ctx)?.let { b.setContentIntent(it) }

        if (hasOtp) {
            b.addAction(0, ctx.getString(R.string.snipt_action_copy_otp), action(ACTION_COPY_OTP, 0))
        }
        val remote = RemoteInput.Builder(KEY_REPLY).setLabel(ctx.getString(R.string.snipt_action_reply)).build()
        b.addAction(
            NotificationCompat.Action.Builder(0, ctx.getString(R.string.snipt_action_reply), action(ACTION_REPLY, 1, mutable = true))
                .addRemoteInput(remote)
                .setAllowGeneratedReplies(false)
                .build()
        )
        b.addAction(0, ctx.getString(R.string.snipt_action_mark_read), action(ACTION_MARK_READ, 2))
        b.addAction(0, ctx.getString(R.string.snipt_action_delete), action(ACTION_DELETE, 3))
        try {
            NotificationManagerCompat.from(ctx).notify(notifId, b.build())
        } catch (e: SecurityException) {
            // permission revoked between the check and the post
        }
    }

    /**
     * After an inline reply. Android 15+ keeps a replied notification alive
     * through a plain cancel (FLAG_LIFETIME_EXTENDED_BY_DIRECT_REPLY), so the
     * app posts a silent update showing the reply, which times out.
     */
    fun replied(ctx: Context, notifId: Int, title: String, reply: String, sent: Boolean) {
        if (!canPost(ctx)) return
        channels(ctx)
        val b = NotificationCompat.Builder(ctx, CHANNEL_MESSAGES)
            .setSmallIcon(R.drawable.ic_snipt_stat)
            .setContentTitle(title)
            .setContentText(ctx.getString(if (sent) R.string.snipt_reply_sent else R.string.snipt_reply_failed))
            .setRemoteInputHistory(arrayOf<CharSequence>(reply))
            .setVisibility(NotificationCompat.VISIBILITY_PRIVATE)
            .setSilent(true)
            .setAutoCancel(true)
            .setTimeoutAfter(4_000)
        try {
            NotificationManagerCompat.from(ctx).notify(notifId, b.build())
        } catch (e: SecurityException) {
            // permission revoked
        }
    }

    fun cancel(ctx: Context, notifId: Int) {
        NotificationManagerCompat.from(ctx).cancel(notifId)
    }

    private fun launchIntent(ctx: Context): PendingIntent? {
        val i = ctx.packageManager.getLaunchIntentForPackage(ctx.packageName) ?: return null
        i.flags = Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP
        return PendingIntent.getActivity(ctx, 0, i, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
    }
}
