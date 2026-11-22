package app.snipt.telephony

import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.os.Build
import android.telephony.SmsManager
import android.util.Log

/**
 * Radio send for a message already queued in the core (store id [id]).
 * Writes the provider OUTBOX row first (the default SMS app must write its
 * own sent messages), sends multipart on the slot's subscription, and lets
 * [SmsStatusReceiver] advance sent/failed/delivered. Status PendingIntents
 * are explicit and FLAG_IMMUTABLE.
 */
object SmsSender {
    private const val TAG = "SniptSend"
    const val ACTION_SENT = "app.snipt.telephony.SMS_SENT"
    const val ACTION_DELIVERED = "app.snipt.telephony.SMS_DELIVERED"
    const val EXTRA_ID = "id"
    const val EXTRA_PROVIDER_ID = "providerId"
    const val EXTRA_PART = "part"
    const val EXTRA_PARTS = "parts"

    fun send(ctx: Context, id: Long, address: String, body: String, simSlot: Int): Boolean {
        if (!NativeCore.ensure(ctx)) return false
        val subId = Subs.subIdForSlot(ctx, simSlot)
        val sms = try {
            manager(ctx, subId)
        } catch (e: Exception) {
            null
        }
        if (sms == null) {
            NativeCore.setSendStatus(id, "failed", -1)
            return false
        }
        val parts = sms.divideMessage(body)
        val providerId = ProviderSync.writeOutbox(ctx, address, body, subId)
        NativeCore.setSendStatus(id, "sending", providerId)
        val sent = ArrayList<PendingIntent>(parts.size)
        val delivered = ArrayList<PendingIntent>(parts.size)
        for (i in parts.indices) {
            sent.add(status(ctx, ACTION_SENT, id, providerId, i, parts.size))
            delivered.add(status(ctx, ACTION_DELIVERED, id, providerId, i, parts.size))
        }
        return try {
            sms.sendMultipartTextMessage(address, null, parts, sent, delivered)
            true
        } catch (e: Exception) {
            Log.w(TAG, "send rejected: ${e.javaClass.simpleName}")
            ProviderSync.setType(ctx, providerId, android.provider.Telephony.Sms.MESSAGE_TYPE_FAILED)
            NativeCore.setSendStatus(id, "failed", providerId)
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

    private fun status(ctx: Context, action: String, id: Long, providerId: Long, part: Int, parts: Int): PendingIntent {
        val i = Intent(ctx, SmsStatusReceiver::class.java)
            .setAction(action)
            .putExtra(EXTRA_ID, id)
            .putExtra(EXTRA_PROVIDER_ID, providerId)
            .putExtra(EXTRA_PART, part)
            .putExtra(EXTRA_PARTS, parts)
        // Unique request code per (message, part, kind) so extras are not merged.
        val kind = if (action == ACTION_SENT) 0 else 1
        val req = ((id and 0xFFFFF) * 64 + part.coerceAtMost(31) * 2 + kind).toInt()
        return PendingIntent.getBroadcast(ctx, req, i, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
    }
}
