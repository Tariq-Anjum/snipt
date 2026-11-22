package app.snipt.telephony

import android.app.Activity
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.provider.Telephony
import android.telephony.SmsMessage
import org.json.JSONObject

/**
 * Sent/delivered callbacks for [SmsSender] (not exported). Each callback is
 * persisted per (attempt, part, kind) in the core, which derives the message
 * state from all stored results and only moves it forward, so callbacks
 * racing on separate threads cannot regress it and nothing lives in process
 * memory. The provider row and an inline-reply notification are updated
 * only when the core reports a state change. The core commit and the
 * provider write run under [SmsSender.STATUS_LOCK], so two callbacks can
 * never write the provider in the opposite order of their core commits.
 */
class SmsStatusReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val code = resultCode
        val pending = goAsync()
        val ctx = context.applicationContext
        Thread {
            try {
                handle(ctx, intent, code)
            } catch (t: Throwable) {
                android.util.Log.w("SniptSend", "status failed: ${t.javaClass.simpleName}")
            } finally {
                pending.finish()
            }
        }.start()
    }

    private fun handle(ctx: Context, intent: Intent, code: Int) {
        if (!NativeCore.ensure(ctx)) return
        val attempt = intent.getLongExtra(SmsSender.EXTRA_ATTEMPT, -1)
        val part = intent.getIntExtra(SmsSender.EXTRA_PART, -1)
        if (attempt < 0 || part < 0) return
        val (kind, outcome) = when (intent.action) {
            SmsSender.ACTION_SENT -> "sent" to if (code == Activity.RESULT_OK) "ok" else "failed"
            SmsSender.ACTION_DELIVERED -> "delivery" to (DeliveryReport.outcome(intent) ?: return)
            else -> return
        }
        val u = synchronized(SmsSender.STATUS_LOCK) {
            val json = NativeCore.sendPartResult(attempt, part, kind, outcome) ?: return
            val u = JSONObject(json)
            if (!u.optBoolean("changed")) return
            val pid = if (u.isNull("providerId")) -1L else u.optLong("providerId", -1L)
            when (u.optString("status")) {
                "sent", "unconfirmed" -> ProviderSync.setType(ctx, pid, Telephony.Sms.MESSAGE_TYPE_SENT)
                "delivered" -> {
                    ProviderSync.setType(ctx, pid, Telephony.Sms.MESSAGE_TYPE_SENT)
                    ProviderSync.setDelivered(ctx, pid)
                }
                "failed" -> ProviderSync.setType(ctx, pid, Telephony.Sms.MESSAGE_TYPE_FAILED)
            }
            u
        }
        val status = u.optString("status")
        // Inline reply: report the radio result once, when it leaves "sending".
        val notif = intent.getIntExtra(SmsSender.EXTRA_REPLY_NOTIF, -1)
        if (notif >= 0 && u.optString("previous") == "sending" && status != "sending") {
            Notifier.replied(
                ctx, notif,
                intent.getStringExtra(SmsSender.EXTRA_REPLY_TITLE) ?: "",
                intent.getStringExtra(SmsSender.EXTRA_REPLY_TEXT) ?: "",
                if (status == "failed") Notifier.ReplyState.FAILED else Notifier.ReplyState.SENT
            )
        }
    }
}

/**
 * Interprets the SMS-STATUS-REPORT attached to a delivery callback
 * (3GPP TS 23.040 TP-Status). Returns `ok`, `failed`, `unknown` (no PDU or
 * one that cannot be parsed: shown as "delivery unconfirmed"), or null for a
 * temporary status where the service centre is still trying (a final report
 * follows).
 */
object DeliveryReport {
    fun outcome(intent: Intent): String? {
        val pdu = intent.getByteArrayExtra("pdu") ?: return "unknown"
        val format = intent.getStringExtra("format") ?: return "unknown"
        if (format != SmsMessage.FORMAT_3GPP) return "unknown"
        val msg = try {
            SmsMessage.createFromPdu(pdu, format)
        } catch (e: Exception) {
            null
        } ?: return "unknown"
        if (!msg.isStatusReportMessage) return "unknown"
        return tpStatus(msg.status)
    }

    /** TP-Status: 0x00-0x1F completed, 0x20-0x3F still trying, 0x40+ failed. */
    fun tpStatus(st: Int): String? = when {
        st < 0 -> "unknown"
        st < 0x20 -> "ok"
        st < 0x40 -> null
        st <= 0xFF -> "failed"
        else -> "unknown"
    }
}
