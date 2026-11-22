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
        val draft = Recipients.draftFrom(intent)
        if (draft == null) {
            finish()
            return
        }
        TelephonyPlugin.pendingCompose = TelephonyPlugin.Compose(draft.first.take(128), draft.second.take(20_000))
        packageManager.getLaunchIntentForPackage(packageName)?.let {
            it.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP)
            startActivity(it)
        }
        if (NativeCore.ensure(applicationContext)) NativeCore.emit("compose")
        finish()
    }
}
