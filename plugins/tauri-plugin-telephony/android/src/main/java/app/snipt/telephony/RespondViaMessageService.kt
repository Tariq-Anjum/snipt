package app.snipt.telephony

import android.app.Service
import android.content.Intent
import android.os.IBinder
import android.telephony.SubscriptionManager
import android.util.Log
import org.json.JSONObject

/**
 * RESPOND_VIA_MESSAGE ("reply to an incoming call with a text"); guarded by
 * SEND_RESPOND_VIA_MESSAGE, so only the phone app can start it. Queues the
 * text in the core and sends it on the user's default SMS subscription (not
 * a fixed slot), recording that subscription's slot on the message.
 */
class RespondViaMessageService : Service() {
    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (intent?.action != "android.intent.action.RESPOND_VIA_MESSAGE") {
            stopSelf(startId)
            return START_NOT_STICKY
        }
        val address = intent.data?.let { Recipients.fromUri(it) }
        val text = intent.getStringExtra(Intent.EXTRA_TEXT)?.trim()
        if (address.isNullOrEmpty() || text.isNullOrEmpty()) {
            stopSelf(startId)
            return START_NOT_STICKY
        }
        val ctx = applicationContext
        Thread {
            try {
                if (NativeCore.ensure(ctx)) {
                    val subId = SubscriptionManager.getDefaultSmsSubscriptionId()
                    val slot = Subs.slotForSubId(ctx, subId)
                    val q = NativeCore.queueOutgoing(
                        JSONObject().put("address", address).put("body", text).put("simSlot", slot).toString()
                    )
                    if (q != null) {
                        val o = JSONObject(q)
                        SmsSender.sendOnSub(ctx, o.getLong("id"), o.getString("address"), text, subId)
                    }
                }
            } catch (t: Throwable) {
                Log.w("SniptRespond", "respond failed: ${t.javaClass.simpleName}")
            } finally {
                stopSelf(startId)
            }
        }.start()
        return START_NOT_STICKY
    }
}
