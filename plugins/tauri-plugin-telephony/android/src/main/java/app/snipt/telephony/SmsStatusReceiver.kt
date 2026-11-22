package app.snipt.telephony

import android.app.Activity
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.provider.Telephony

/**
 * Sent/delivered callbacks for [SmsSender] (not exported). Multipart: any
 * failed part marks the message failed; the last part's success marks it
 * sent / delivered. With FLAG_IMMUTABLE the status-report PDU extra is not
 * attached, so "delivered" means a status report arrived (failure codes in
 * the report are not distinguished; documented in ANDROID-NOTES).
 */
class SmsStatusReceiver : BroadcastReceiver() {
    companion object {
        /** Messages with a failed part in this process (a later part's OK must not mark them sent). */
        private val failed: MutableSet<Long> = java.util.concurrent.ConcurrentHashMap.newKeySet()
    }

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
        val id = intent.getLongExtra(SmsSender.EXTRA_ID, -1)
        val pid = intent.getLongExtra(SmsSender.EXTRA_PROVIDER_ID, -1)
        val part = intent.getIntExtra(SmsSender.EXTRA_PART, 0)
        val parts = intent.getIntExtra(SmsSender.EXTRA_PARTS, 1)
        if (id < 0) return
        val last = part >= parts - 1
        when (intent.action) {
            SmsSender.ACTION_SENT -> {
                if (code != Activity.RESULT_OK) {
                    failed.add(id)
                    ProviderSync.setType(ctx, pid, Telephony.Sms.MESSAGE_TYPE_FAILED)
                    NativeCore.setSendStatus(id, "failed", pid)
                } else if (last && !failed.remove(id)) {
                    ProviderSync.setType(ctx, pid, Telephony.Sms.MESSAGE_TYPE_SENT)
                    NativeCore.setSendStatus(id, "sent", pid)
                }
            }
            SmsSender.ACTION_DELIVERED -> if (last && !failed.contains(id)) {
                ProviderSync.setDelivered(ctx, pid)
                NativeCore.setSendStatus(id, "delivered", pid)
            }
        }
    }
}
