package app.snipt.telephony

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import android.telephony.SubscriptionManager

/** Dual-SIM helpers (SubscriptionManager; needs READ_PHONE_STATE for the list). */
object Subs {
    private fun canRead(ctx: Context) =
        ctx.checkSelfPermission(Manifest.permission.READ_PHONE_STATE) == PackageManager.PERMISSION_GRANTED

    /** 0/1 slot for a subscription id (unknown -> 0). getSlotIndex is API 29. */
    fun slotForSubId(ctx: Context, subId: Int): Int {
        if (subId < 0) return 0
        val slot = SubscriptionManager.getSlotIndex(subId)
        return if (slot == 1) 1 else 0
    }

    /** Subscription id in a slot, or -1 (use the default SMS subscription). */
    fun subIdForSlot(ctx: Context, slot: Int): Int {
        return active(ctx).firstOrNull { it.slot == slot }?.subId ?: -1
    }

    data class Info(val subId: Int, val slot: Int, val carrier: String)

    @Suppress("MissingPermission")
    fun active(ctx: Context): List<Info> {
        if (!canRead(ctx)) return emptyList()
        return try {
            val sm = ctx.getSystemService(SubscriptionManager::class.java) ?: return emptyList()
            (sm.activeSubscriptionInfoList ?: emptyList()).map {
                Info(it.subscriptionId, it.simSlotIndex, it.carrierName?.toString() ?: "")
            }
        } catch (e: SecurityException) {
            emptyList()
        }
    }
}
