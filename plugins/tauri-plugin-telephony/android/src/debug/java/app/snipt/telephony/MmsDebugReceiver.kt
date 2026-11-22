package app.snipt.telephony

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.util.Log
import java.io.File

/**
 * DEBUG ONLY. `adb shell am broadcast -a app.snipt.sms.DEBUG_MMS
 *   -n app.snipt.sms/app.snipt.telephony.MmsDebugReceiver --es mode <m>`
 * with `filesDir/debug-mms/notification.pdu` (M-Notification.ind) and, for
 * mode `fixture`, `filesDir/debug-mms/retrieve.pdu` (M-Retrieve.conf):
 * - `fixture`: notification -> core (pending), retrieve.pdu is placed where
 *   the platform download would have written it, then the same
 *   [MmsDownloader.downloaded] path runs (provider write, ingest,
 *   thumbnails, notification, NotifyResp send).
 * - `download`: notification -> real `downloadMultimediaMessage` (on an
 *   emulator this fails: exercises the failure / retry / placeholder path).
 * - `maintenance`: run the retry/expiry pass.
 */
class MmsDebugReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        if (intent.action != "app.snipt.sms.DEBUG_MMS") return
        val mode = intent.getStringExtra("mode") ?: "fixture"
        val pending = goAsync()
        val ctx = context.applicationContext
        Thread {
            try {
                run(ctx, mode)
            } catch (t: Throwable) {
                Log.w(TAG, "debug MMS failed: ${t.javaClass.simpleName}")
            } finally {
                pending.finish()
            }
        }.start()
    }

    private fun run(ctx: Context, mode: String) {
        val dir = File(ctx.filesDir, "debug-mms")
        if (mode == "maintenance") {
            MmsDownloader.maintenance(ctx)
            Log.i(TAG, "debug MMS maintenance done")
            return
        }
        val notif = File(dir, "notification.pdu").takeIf { it.isFile } ?: run {
            Log.w(TAG, "debug MMS: notification.pdu missing")
            return
        }
        val pid = MmsWapPushReceiver.handlePush(ctx, notif.readBytes(), -1, download = mode == "download")
        Log.i(TAG, "debug MMS mode=$mode pending=$pid")
        if (mode != "fixture" || pid < 0) return
        val retrieve = File(dir, "retrieve.pdu").takeIf { it.isFile } ?: return
        if (NativeCore.mmsAttempt(pid) < 0) {
            Log.i(TAG, "debug MMS: not pending (already handled)")
            return
        }
        retrieve.copyTo(MmsDownloader.downloadFile(ctx, pid), overwrite = true)
        MmsDownloader.downloaded(ctx, pid)
    }

    companion object {
        private const val TAG = "SniptMmsDebug"
    }
}
