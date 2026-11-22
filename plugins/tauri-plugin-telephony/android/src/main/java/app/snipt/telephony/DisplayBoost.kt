package app.snipt.telephony

import android.app.Activity
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.os.PowerManager
import android.view.View

/**
 * Adaptive refresh: high rate while the user interacts, back to the system's
 * choice [IDLE_MS] after the last input. API 35+: View.setRequestedFrameRate
 * (REQUESTED_FRAME_RATE_CATEGORY_HIGH / _DEFAULT) on the WebView; API 30-34:
 * window preferredDisplayModeId = highest-rate mode at the current
 * resolution, 0 (no preference) when idle. Disabled in power-save.
 */
class DisplayBoost(private val activity: Activity, private val view: View) {
    private val main = Handler(Looper.getMainLooper())
    private var boosted = false
    private val idle = Runnable { apply(false) }

    fun poke() {
        main.removeCallbacks(idle)
        if (!boosted) apply(true)
        main.postDelayed(idle, IDLE_MS)
    }

    fun set(on: Boolean) {
        main.removeCallbacks(idle)
        apply(on)
    }

    private fun powerSave(): Boolean =
        activity.getSystemService(PowerManager::class.java)?.isPowerSaveMode == true

    private fun apply(on: Boolean) {
        val want = on && !powerSave()
        if (want == boosted) return
        boosted = want
        if (Build.VERSION.SDK_INT >= 35) {
            view.setRequestedFrameRate(
                if (want) View.REQUESTED_FRAME_RATE_CATEGORY_HIGH else View.REQUESTED_FRAME_RATE_CATEGORY_DEFAULT
            )
        } else if (Build.VERSION.SDK_INT >= 30) {
            val w = activity.window ?: return
            val attrs = w.attributes
            attrs.preferredDisplayModeId = if (want) bestModeId() else 0
            w.attributes = attrs
        }
    }

    private fun bestModeId(): Int {
        val d = (if (Build.VERSION.SDK_INT >= 30) activity.display else null) ?: return 0
        val cur = d.mode
        return d.supportedModes
            .filter { it.physicalWidth == cur.physicalWidth && it.physicalHeight == cur.physicalHeight }
            .maxByOrNull { it.refreshRate }?.modeId ?: 0
    }

    companion object {
        const val IDLE_MS = 1_500L
    }
}
