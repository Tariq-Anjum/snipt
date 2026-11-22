package app.snipt.telephony

import android.content.ClipData
import android.content.ClipDescription
import android.content.ClipboardManager
import android.content.Context
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.os.PersistableBundle

/**
 * Sensitive copy (OTP): the clip is flagged EXTRA_IS_SENSITIVE (API 33+;
 * the same extra key is set on older releases, where it is ignored), and a
 * wipe runs after [DEFAULT_CLEAR_MS]. Cancel-safe: a newer sensitive copy
 * replaces the pending wipe, and any clipboard change after ours (the user
 * copied something else) cancels it, so we never clear someone else's clip.
 * The wipe runs in-process only; if the process dies first, the OS clipboard
 * auto-clear is the backstop (documented in ANDROID-NOTES).
 */
object SecureClipboard {
    const val DEFAULT_CLEAR_MS = 30_000L
    private const val LABEL = "Snipt code"
    private const val EXTRA_IS_SENSITIVE_KEY = "android.content.extra.IS_SENSITIVE"

    private val main = Handler(Looper.getMainLooper())
    private var wipe: Runnable? = null
    private var listener: ClipboardManager.OnPrimaryClipChangedListener? = null
    private var generation = 0

    fun copy(ctx: Context, text: String, clearAfterMs: Long) {
        val app = ctx.applicationContext
        val delay = clearAfterMs.coerceIn(5_000L, 120_000L)
        main.post {
            val cm = app.getSystemService(ClipboardManager::class.java) ?: return@post
            disarm(cm)
            val clip = ClipData.newPlainText(LABEL, text)
            clip.description.extras = PersistableBundle().apply {
                if (Build.VERSION.SDK_INT >= 33) {
                    putBoolean(ClipDescription.EXTRA_IS_SENSITIVE, true)
                } else {
                    putBoolean(EXTRA_IS_SENSITIVE_KEY, true)
                }
            }
            cm.setPrimaryClip(clip)
            val gen = ++generation
            // Arm the change listener after our own change has been dispatched.
            main.postDelayed({
                if (gen != generation) return@postDelayed
                val l = ClipboardManager.OnPrimaryClipChangedListener {
                    if (gen == generation) disarm(cm) // someone else's clip now: keep it
                }
                listener = l
                cm.addPrimaryClipChangedListener(l)
            }, 750)
            val r = Runnable {
                if (gen != generation) return@Runnable
                disarm(cm)
                try {
                    cm.clearPrimaryClip()
                } catch (e: Exception) {
                    // ignore: nothing else we can do
                }
            }
            wipe = r
            main.postDelayed(r, delay)
        }
    }

    private fun disarm(cm: ClipboardManager) {
        generation++
        wipe?.let { main.removeCallbacks(it) }
        wipe = null
        listener?.let { cm.removePrimaryClipChangedListener(it) }
        listener = null
    }
}
