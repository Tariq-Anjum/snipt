package app.snipt.telephony

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.util.Log

/**
 * WAP_PUSH_DELIVER (MMS notification) - required for the default-SMS role.
 * MMS download/parse is slice S15; until then the push is acknowledged and
 * dropped. Logs only the fact of arrival, never the PDU or sender.
 */
class MmsWapPushReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        if (intent.action != "android.provider.Telephony.WAP_PUSH_DELIVER") return
        Log.i("SniptMms", "MMS notification received; MMS is not supported yet (S15)")
    }
}
