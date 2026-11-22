package app.snipt.telephony

import android.content.ClipData
import android.content.ClipDescription
import android.content.ClipboardManager
import android.content.Context
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.os.PersistableBundle
import android.util.Log
import java.util.UUID

/**
 * Sensitive copy (OTP). The clip is flagged EXTRA_IS_SENSITIVE (API 33+;
 * the same extra key is set on older releases, where it is ignored) and
 * carries a random per-copy ownership token in its description extras.
 *
 * Timed wipe, ownership-checked: when the timer fires, the clip is cleared
 * only if the current primary clip still carries *this* copy's token. A clip
 * the user copied afterwards (any time, including the first second) has no
 * token or another one, so it is never erased. If Android does not let Snipt
 * read the clip description (Android 10+ hides the clipboard from apps that
 * are not in focus), ownership cannot be verified and nothing is cleared:
 * the wipe is best-effort, and the OS clipboard auto-clear is the backstop.
 * The wipe runs in-process only (documented in ANDROID-NOTES).
 */
object SecureClipboard {
    const val DEFAULT_CLEAR_MS = 30_000L
    private const val LABEL = "Snipt code"
    private const val TAG = "SniptClip"
    private const val EXTRA_IS_SENSITIVE_KEY = "android.content.extra.IS_SENSITIVE"
    private const val EXTRA_TOKEN = "app.snipt.telephony.CLIP_TOKEN"

    private val main = Handler(Looper.getMainLooper())
    private var wipe: Runnable? = null

    /**
     * Copies [text]; [done] gets true only when the clip was set (the wipe is
     * armed then). Runs on the main thread.
     */
    fun copy(ctx: Context, text: String, clearAfterMs: Long, done: (Boolean) -> Unit = {}) {
        val app = ctx.applicationContext
        val delay = clearAfterMs.coerceIn(5_000L, 120_000L)
        main.post {
            val cm = app.getSystemService(ClipboardManager::class.java)
            if (cm == null) {
                done(false)
                return@post
            }
            wipe?.let { main.removeCallbacks(it) }
            wipe = null
            val token = UUID.randomUUID().toString()
            val clip = ClipData.newPlainText(LABEL, text)
            clip.description.extras = PersistableBundle().apply {
                if (Build.VERSION.SDK_INT >= 33) {
                    putBoolean(ClipDescription.EXTRA_IS_SENSITIVE, true)
                } else {
                    putBoolean(EXTRA_IS_SENSITIVE_KEY, true)
                }
                putString(EXTRA_TOKEN, token)
            }
            try {
                cm.setPrimaryClip(clip)
            } catch (e: Exception) {
                Log.w(TAG, "clip set failed: ${e.javaClass.simpleName}")
                done(false)
                return@post
            }
            Log.i(TAG, "sensitive clip set; ownership-checked wipe in ${delay / 1000} s")
            val r = Runnable { wipeIfOwned(cm, token) }
            wipe = r
            main.postDelayed(r, delay)
            done(true)
        }
    }

    private fun wipeIfOwned(cm: ClipboardManager, token: String) {
        wipe = null
        val desc = try {
            cm.primaryClipDescription
        } catch (e: Exception) {
            null
        }
        if (desc == null) {
            Log.i(TAG, "clip ownership not verifiable; left to the OS clipboard expiry")
            return
        }
        if (desc.extras?.getString(EXTRA_TOKEN) != token) {
            Log.i(TAG, "clipboard holds another clip; not cleared")
            return
        }
        try {
            cm.clearPrimaryClip()
            Log.i(TAG, "sensitive clip wiped")
        } catch (e: Exception) {
            Log.w(TAG, "clip wipe failed: ${e.javaClass.simpleName}")
        }
    }
}
