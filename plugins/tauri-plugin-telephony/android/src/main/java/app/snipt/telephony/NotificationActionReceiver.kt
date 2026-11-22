package app.snipt.telephony

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.os.Handler
import android.os.Looper
import android.util.Log
import android.widget.Toast
import androidx.core.app.RemoteInput
import org.json.JSONObject

/**
 * Notification actions (not exported). Works with the UI closed.
 *
 * Every action first asks the core to validate its source (`actionTarget`):
 * the message must still exist and come from the address the notification
 * was posted for. Ids are never reused, so after the message is deleted the
 * action is inert (its notification is dropped). The provider id comes from
 * the core, not from the intent.
 *
 * Mark read / Delete change the Telephony provider first and the core only
 * when the provider write is verified; on failure the notification stays
 * (the action can be retried) and a toast says so.
 */
class NotificationActionReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val pending = goAsync()
        BackgroundWork.begin()
        val ctx = context.applicationContext
        // RemoteInput results must be read from the delivered intent.
        val reply = RemoteInput.getResultsFromIntent(intent)?.getCharSequence(Notifier.KEY_REPLY)?.toString()
        Thread {
            try {
                handle(ctx, intent, reply)
            } catch (t: Throwable) {
                Log.w(TAG, "action failed: ${t.javaClass.simpleName}")
            } finally {
                BackgroundWork.end()
                pending.finish()
            }
        }.start()
    }

    private fun toast(ctx: Context, res: Int) {
        Handler(Looper.getMainLooper()).post { Toast.makeText(ctx, res, Toast.LENGTH_SHORT).show() }
    }

    private fun handle(ctx: Context, intent: Intent, reply: String?) {
        if (!NativeCore.ensure(ctx)) return
        val msgId = intent.getLongExtra(Notifier.EXTRA_MSG_ID, -1)
        val address = intent.getStringExtra(Notifier.EXTRA_ADDRESS) ?: ""
        val threadId = intent.getLongExtra(Notifier.EXTRA_THREAD_ID, -1)
        val notifId = intent.getIntExtra(Notifier.EXTRA_NOTIF_ID, 0)
        val slot = intent.getIntExtra(Notifier.EXTRA_SIM_SLOT, 0)
        val target = NativeCore.actionTarget(msgId, address)?.let { JSONObject(it) }
        if (target == null || target.optLong("threadId", -2) != threadId) {
            Log.i(TAG, "stale notification action ignored")
            Notifier.stale(ctx, notifId)
            toast(ctx, R.string.snipt_action_stale)
            return
        }
        val providerId = if (target.isNull("providerId")) -1L else target.optLong("providerId", -1L)
        // S15: an MMS lives in content://mms (separate id namespace).
        val mmsId = if (target.isNull("mmsId")) -1L else target.optLong("mmsId", -1L)
        val row = if (mmsId >= 0) Row(mmsId, true) else Row(providerId, false)
        when (intent.action) {
            Notifier.ACTION_MARK_READ -> {
                if (markRead(ctx, msgId, row)) {
                    Notifier.cancel(ctx, notifId)
                } else {
                    toast(ctx, R.string.snipt_action_failed)
                }
            }
            Notifier.ACTION_DELETE -> {
                val ok = row.id < 0 || ProviderSync.delete(ctx, listOf(row.id), row.mms).all { it.second }
                if (ok && NativeCore.deleteMessage(msgId)) {
                    Notifier.cancel(ctx, notifId)
                } else {
                    toast(ctx, R.string.snipt_action_failed)
                }
            }
            Notifier.ACTION_COPY_OTP -> {
                val code = NativeCore.otpCode(msgId) ?: return
                SecureClipboard.copy(ctx, code, SecureClipboard.DEFAULT_CLEAR_MS) { ok ->
                    toast(ctx, if (ok) R.string.snipt_code_copied else R.string.snipt_copy_failed)
                }
            }
            Notifier.ACTION_REPLY -> {
                val text = reply?.trim().orEmpty()
                val title = intent.getStringExtra(Notifier.EXTRA_TITLE) ?: ""
                if (text.isEmpty()) return
                val q = NativeCore.queueOutgoing(
                    JSONObject()
                        .put("threadId", threadId)
                        .put("address", address)
                        .put("replyTo", msgId)
                        .put("body", text)
                        .put("simSlot", slot)
                        .toString()
                )
                if (q == null) {
                    Log.w(TAG, "reply rejected")
                    Notifier.replied(ctx, notifId, title, text, Notifier.ReplyState.FAILED)
                    return
                }
                val o = JSONObject(q)
                // "Sending..." now; the sent callback reports the outcome.
                Notifier.replied(ctx, notifId, title, text, Notifier.ReplyState.SENDING)
                val handedOff = SmsSender.send(
                    ctx, o.getLong("id"), o.getString("address"), text, slot,
                    SmsSender.Reply(notifId, title, text)
                )
                if (!handedOff) Notifier.replied(ctx, notifId, title, text, Notifier.ReplyState.FAILED)
                // Replying reads the conversation (best-effort; failure keeps it unread).
                markRead(ctx, msgId, row)
            }
        }
    }

    /** Provider first; the core only after the provider write is verified. */
    private fun markRead(ctx: Context, msgId: Long, row: Row): Boolean {
        val ok = row.id < 0 || ProviderSync.markRead(ctx, listOf(row.id), row.mms).all { it.second }
        if (!ok) return false
        // The local write must succeed too (false = the message is gone or
        // the core failed): the action stays retryable instead of "done".
        return NativeCore.markRead(msgId)
    }

    /** A provider row: id in content://sms (mms=false) or content://mms. */
    private data class Row(val id: Long, val mms: Boolean)

    companion object {
        private const val TAG = "SniptAction"
    }
}
