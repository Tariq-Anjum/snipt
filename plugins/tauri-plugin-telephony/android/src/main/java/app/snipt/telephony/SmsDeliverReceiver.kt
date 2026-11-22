package app.snipt.telephony

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.provider.Telephony
import android.telephony.SubscriptionManager
import android.util.Log
import org.json.JSONObject

/**
 * SMS_DELIVER (default SMS app only; guarded by BROADCAST_SMS in the
 * manifest). Runs with no Activity/WebView: write the provider row (the
 * default app must), ingest into the Rust core over JNI with the provider id,
 * post the notification. Nothing about the message is logged.
 */
class SmsDeliverReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        if (intent.action != Telephony.Sms.Intents.SMS_DELIVER_ACTION) return
        val pending = goAsync()
        val ctx = context.applicationContext
        Thread {
            try {
                handle(ctx, intent)
            } catch (t: Throwable) {
                Log.w(TAG, "deliver failed: ${t.javaClass.simpleName}")
            } finally {
                pending.finish()
            }
        }.start()
    }

    private fun handle(ctx: Context, intent: Intent) {
        val parts = Telephony.Sms.Intents.getMessagesFromIntent(intent)
        if (parts.isNullOrEmpty()) return
        val first = parts[0] ?: return
        val address = first.displayOriginatingAddress ?: first.originatingAddress ?: return
        val body = parts.joinToString("") { it?.displayMessageBody ?: "" }
        val received = System.currentTimeMillis()
        val subId = intent.getIntExtra(
            SubscriptionManager.EXTRA_SUBSCRIPTION_INDEX,
            intent.getIntExtra("subscription", -1)
        )
        val slot = Subs.slotForSubId(ctx, subId)
        val providerId = ProviderSync.writeInbox(ctx, address, body, received, first.timestampMillis, subId)
        if (!NativeCore.ensure(ctx)) return
        val json = JSONObject().apply {
            if (providerId >= 0) put("providerId", providerId)
            put("message", JSONObject().apply {
                put("sender", address)
                put("body", body)
                put("isKnownContact", false)
                put("simSlot", slot)
                put("timestampMs", received)
            })
        }
        val out = NativeCore.ingest(json.toString()) ?: run {
            Log.w(TAG, "ingest rejected")
            return
        }
        Notifier.incoming(ctx, JSONObject(out), slot)
    }

    companion object {
        private const val TAG = "SniptDeliver"
    }
}
