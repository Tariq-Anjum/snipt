package app.snipt.telephony

import android.app.Activity
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.util.Log

/**
 * Result of `downloadMultimediaMessage` / `sendMultimediaMessage` (the
 * NotifyResp ack). Not exported: only our own PendingIntents reach it. Data
 * URI `snipt-mms://download/<pendingId>` or `snipt-mms://ack/<pendingId>`.
 * Result code: Activity.RESULT_OK or SmsManager.MMS_ERROR_* (SmsManager docs).
 */
class MmsDownloadReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val data = intent.data ?: return
        if (data.scheme != MmsDownloader.SCHEME) return
        val pid = data.lastPathSegment?.toLongOrNull() ?: return
        val kind = data.host
        val code = resultCode
        val pending = goAsync()
        val ctx = context.applicationContext
        Thread {
            try {
                if (!NativeCore.ensure(ctx)) return@Thread
                when (kind) {
                    "download" ->
                        if (code == Activity.RESULT_OK) MmsDownloader.downloaded(ctx, pid)
                        else {
                            Log.i(TAG, "MMS download result=$code")
                            MmsDownloader.failed(ctx, pid)
                        }
                    "ack" -> MmsDownloader.acked(ctx, pid, code)
                }
            } catch (t: Throwable) {
                Log.w(TAG, "MMS result failed: ${t.javaClass.simpleName}")
            } finally {
                pending.finish()
            }
        }.start()
    }

    companion object {
        private const val TAG = "SniptMms"
    }
}
