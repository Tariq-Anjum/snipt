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

/** Notification actions (not exported). Works with the UI closed. */
class NotificationActionReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val pending = goAsync()
        val ctx = context.applicationContext
        // RemoteInput results must be read from the delivered intent.
        val reply = RemoteInput.getResultsFromIntent(intent)?.getCharSequence(Notifier.KEY_REPLY)?.toString()
        Thread {
            try {
                handle(ctx, intent, reply)
            } catch (t: Throwable) {
                Log.w(TAG, "action failed: ${t.javaClass.simpleName}")
            } finally {
                pending.finish()
            }
        }.start()
    }

    private fun handle(ctx: Context, intent: Intent, reply: String?) {
        if (!NativeCore.ensure(ctx)) return
        val msgId = intent.getLongExtra(Notifier.EXTRA_MSG_ID, -1)
        val providerId = intent.getLongExtra(Notifier.EXTRA_PROVIDER_ID, -1)
        val threadId = intent.getLongExtra(Notifier.EXTRA_THREAD_ID, -1)
        val notifId = intent.getIntExtra(Notifier.EXTRA_NOTIF_ID, 0)
        val slot = intent.getIntExtra(Notifier.EXTRA_SIM_SLOT, 0)
        when (intent.action) {
            Notifier.ACTION_MARK_READ -> {
                ProviderSync.markRead(ctx, listOf(providerId))
                NativeCore.markRead(msgId)
                Notifier.cancel(ctx, notifId)
            }
            Notifier.ACTION_DELETE -> {
                ProviderSync.delete(ctx, listOf(providerId))
                NativeCore.deleteMessage(msgId)
                Notifier.cancel(ctx, notifId)
            }
            Notifier.ACTION_COPY_OTP -> {
                val code = NativeCore.otpCode(msgId) ?: return
                SecureClipboard.copy(ctx, code, SecureClipboard.DEFAULT_CLEAR_MS)
                Handler(Looper.getMainLooper()).post {
                    Toast.makeText(ctx, R.string.snipt_code_copied, Toast.LENGTH_SHORT).show()
                }
            }
            Notifier.ACTION_REPLY -> {
                val text = reply?.trim().orEmpty()
                if (text.isNotEmpty() && threadId >= 0) {
                    val q = NativeCore.queueOutgoing(
                        JSONObject().put("threadId", threadId).put("body", text).put("simSlot", slot).toString()
                    )
                    if (q != null) {
                        val o = JSONObject(q)
                        SmsSender.send(ctx, o.getLong("id"), o.getString("address"), text, slot)
                        // Replying reads the conversation.
                        ProviderSync.markRead(ctx, listOf(providerId))
                        NativeCore.markRead(msgId)
                    } else {
                        Log.w(TAG, "reply rejected")
                    }
                }
                Notifier.cancel(ctx, notifId)
            }
        }
    }

    companion object {
        private const val TAG = "SniptAction"
    }
}
