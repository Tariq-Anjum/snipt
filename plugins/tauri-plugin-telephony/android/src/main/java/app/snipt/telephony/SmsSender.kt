package app.snipt.telephony

import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.telephony.SmsManager
import android.util.Log

/**
 * Radio send for a message already queued in the core (store id [id]).
 *
 * 1. The core opens a send attempt (`beginSend`, a never-reused id) and the
 *    message becomes `sending`.
 * 2. The provider OUTBOX row is written (the default SMS app must write its
 *    own sent messages) and bound to the attempt once (`bindSend`).
 * 3. Every part gets its own sent and delivery PendingIntent, made unique by
 *    a `snipt-send://<attempt>/<part>/<kind>` data URI (so >32 parts and
 *    concurrent sends never share or overwrite one), handled by
 *    [SmsStatusReceiver], which persists each part's result in the core.
 *
 * Sent intents are FLAG_IMMUTABLE (only the result code matters). Delivery
 * intents are explicit and FLAG_MUTABLE so the system can attach the
 * status-report PDU (`pdu`/`format` extras); without it the receiver would
 * have no way to tell a carrier failure report from a success.
 *
 * [reply] marks an inline notification reply: the receiver updates that
 * notification once the radio result is known (never before).
 */
object SmsSender {
    private const val TAG = "SniptSend"
    const val ACTION_SENT = "app.snipt.telephony.SMS_SENT"
    const val ACTION_DELIVERED = "app.snipt.telephony.SMS_DELIVERED"
    const val EXTRA_ATTEMPT = "attempt"
    const val EXTRA_PART = "part"
    const val EXTRA_PARTS = "parts"
    const val EXTRA_REPLY_NOTIF = "replyNotif"
    const val EXTRA_REPLY_TITLE = "replyTitle"
    const val EXTRA_REPLY_TEXT = "replyText"

    data class Reply(val notifId: Int, val title: String, val text: String)

    fun send(ctx: Context, id: Long, address: String, body: String, simSlot: Int, reply: Reply? = null): Boolean =
        sendOnSub(ctx, id, address, body, Subs.subIdForSlot(ctx, simSlot), reply)

    /** [subId] -1 = the default SMS subscription. */
    fun sendOnSub(ctx: Context, id: Long, address: String, body: String, subId: Int, reply: Reply? = null): Boolean {
        if (!NativeCore.ensure(ctx)) return false
        val sms = try {
            manager(ctx, subId)
        } catch (e: Exception) {
            null
        }
        if (sms == null) {
            NativeCore.failSend(id)
            return false
        }
        val parts = sms.divideMessage(body)
        if (parts.isEmpty()) {
            NativeCore.failSend(id)
            return false
        }
        val attempt = NativeCore.beginSend(id, parts.size)
        if (attempt < 0) {
            Log.w(TAG, "send attempt refused")
            NativeCore.failSend(id)
            return false
        }
        var providerId = ProviderSync.writeOutbox(ctx, address, body, subId)
        if (providerId >= 0 && !NativeCore.bindSend(attempt, providerId)) {
            // The core refused the binding: never update a row it does not own.
            Log.w(TAG, "provider binding refused")
            providerId = -1
        }
        val sent = ArrayList<PendingIntent>(parts.size)
        val delivered = ArrayList<PendingIntent>(parts.size)
        for (i in parts.indices) {
            sent.add(status(ctx, ACTION_SENT, attempt, i, parts.size, reply))
            delivered.add(status(ctx, ACTION_DELIVERED, attempt, i, parts.size, reply))
        }
        return try {
            sms.sendMultipartTextMessage(address, null, parts, sent, delivered)
            true
        } catch (e: Exception) {
            Log.w(TAG, "send rejected: ${e.javaClass.simpleName}")
            ProviderSync.setType(ctx, providerId, android.provider.Telephony.Sms.MESSAGE_TYPE_FAILED)
            NativeCore.failSend(id)
            false
        }
    }

    @Suppress("DEPRECATION")
    private fun manager(ctx: Context, subId: Int): SmsManager =
        if (Build.VERSION.SDK_INT >= 31) {
            val base = ctx.getSystemService(SmsManager::class.java)
            if (subId >= 0) base.createForSubscriptionId(subId) else base
        } else {
            if (subId >= 0) SmsManager.getSmsManagerForSubscriptionId(subId) else SmsManager.getDefault()
        }

    private fun status(ctx: Context, action: String, attempt: Long, part: Int, parts: Int, reply: Reply?): PendingIntent {
        val kind = if (action == ACTION_SENT) "sent" else "delivery"
        val i = Intent(ctx, SmsStatusReceiver::class.java)
            .setAction(action)
            // Identity of this PendingIntent (Intent.filterEquals compares data).
            .setData(Uri.parse("snipt-send://$attempt/$part/$kind"))
            .putExtra(EXTRA_ATTEMPT, attempt)
            .putExtra(EXTRA_PART, part)
            .putExtra(EXTRA_PARTS, parts)
        if (reply != null) {
            i.putExtra(EXTRA_REPLY_NOTIF, reply.notifId)
                .putExtra(EXTRA_REPLY_TITLE, reply.title)
                .putExtra(EXTRA_REPLY_TEXT, reply.text)
        }
        val mutability = if (action == ACTION_SENT) PendingIntent.FLAG_IMMUTABLE else PendingIntent.FLAG_MUTABLE
        return PendingIntent.getBroadcast(ctx, 0, i, mutability or PendingIntent.FLAG_UPDATE_CURRENT)
    }
}
