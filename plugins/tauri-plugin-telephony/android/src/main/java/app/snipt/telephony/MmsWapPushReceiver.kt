package app.snipt.telephony

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.provider.Telephony
import android.telephony.SubscriptionManager
import android.util.Log

/**
 * WAP_PUSH_DELIVER (default SMS app only; manifest-guarded by
 * BROADCAST_WAP_PUSH, mime application/vnd.wap.mms-message). The `data`
 * extra is the M-Notification.ind (Telephony.Sms.Intents docs). The Rust
 * core parses it and records the transaction (dedup by X-Mms-Transaction-Id,
 * expiry, attempts); then the platform downloads it
 * (SmsManager.downloadMultimediaMessage, [MmsDownloader]). Nothing about the
 * message is logged.
 */
class MmsWapPushReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        if (intent.action != Telephony.Sms.Intents.WAP_PUSH_DELIVER_ACTION) return
        val data = intent.getByteArrayExtra("data") ?: return
        val subId = subIdOf(intent)
        val pending = goAsync()
        val ctx = context.applicationContext
        Thread {
            try {
                handlePush(ctx, data, subId, download = true)
                MmsDownloader.maintenance(ctx)
            } catch (t: Throwable) {
                Log.w(TAG, "wap push failed: ${t.javaClass.simpleName}")
            } finally {
                pending.finish()
            }
        }.start()
    }

    companion object {
        private const val TAG = "SniptMms"

        /** Optional subscription extra: documented as long, platform code also uses int. */
        fun subIdOf(intent: Intent): Int {
            val e = intent.extras ?: return -1
            for (k in listOf(SubscriptionManager.EXTRA_SUBSCRIPTION_INDEX, "subscription")) {
                if (!e.containsKey(k)) continue
                val i = e.getInt(k, Int.MIN_VALUE)
                if (i != Int.MIN_VALUE) return i
                val l = e.getLong(k, Long.MIN_VALUE)
                if (l != Long.MIN_VALUE && l in 0..Int.MAX_VALUE) return l.toInt()
            }
            return -1
        }

        /**
         * Records the notification in the core; starts the download when the
         * core says so (new or still-pending transaction). Returns the
         * pending id, or -1.
         */
        fun handlePush(ctx: Context, pdu: ByteArray, subId: Int, download: Boolean): Long {
            if (!NativeCore.ensure(ctx)) return -1
            val slot = Subs.slotForSubId(ctx, subId)
            val out = NativeCore.mmsNotification(pdu, subId, slot)?.let { org.json.JSONObject(it) } ?: run {
                Log.w(TAG, "MMS notification rejected")
                return -1
            }
            val pid = out.getLong("pendingId")
            if (out.optString("action") != "download") {
                Log.i(TAG, "duplicate MMS notification ignored")
                return pid
            }
            Log.i(TAG, "MMS notification recorded; download=${download}")
            if (download) MmsDownloader.start(ctx, pid, out.getString("location"), subId)
            return pid
        }
    }
}
