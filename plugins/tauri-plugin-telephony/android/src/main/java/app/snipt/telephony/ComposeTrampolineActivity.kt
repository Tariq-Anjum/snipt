package app.snipt.telephony

import android.app.Activity
import android.content.Intent
import android.os.Bundle

/**
 * SENDTO / VIEW (sms:, smsto:, mms:, mmsto:) and SEND text/plain from other
 * apps: keep the draft (recipient + text) for the web UI and bring the main
 * activity forward. The draft is taken once via `plugin:telephony|take_compose`.
 */
class ComposeTrampolineActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val i = intent
        val address = i?.data?.let { Recipients.fromUri(it) }
        val body = i?.getStringExtra(Intent.EXTRA_TEXT)
            ?: i?.getStringExtra("sms_body")
            ?: i?.data?.let { Recipients.bodyFromUri(it) }
        TelephonyPlugin.pendingCompose = TelephonyPlugin.Compose(address ?: "", (body ?: "").take(20_000))
        packageManager.getLaunchIntentForPackage(packageName)?.let {
            it.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP)
            startActivity(it)
        }
        if (NativeCore.ensure(applicationContext)) NativeCore.emit("compose")
        finish()
    }
}
