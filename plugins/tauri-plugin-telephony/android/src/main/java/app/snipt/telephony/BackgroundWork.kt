package app.snipt.telephony

import java.util.concurrent.atomic.AtomicInteger

/**
 * Receivers doing store/provider work right now (goAsync threads). The
 * activity ends the process on exit only when this is idle, so leaving the
 * app never interrupts an SMS being saved.
 */
object BackgroundWork {
    private val running = AtomicInteger(0)

    fun begin() {
        running.incrementAndGet()
    }

    fun end() {
        running.decrementAndGet()
    }

    @JvmStatic
    fun busy(): Boolean = running.get() > 0
}
