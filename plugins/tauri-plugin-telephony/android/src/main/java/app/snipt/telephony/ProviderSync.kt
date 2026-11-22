package app.snipt.telephony

import android.content.ContentUris
import android.content.ContentValues
import android.content.Context
import android.provider.Telephony
import android.util.Log
import org.json.JSONArray
import org.json.JSONObject

/**
 * Telephony provider (`content://sms`) writes and sync. Only the default SMS
 * app may write; every write is wrapped so a missing role never crashes a
 * receiver. Logs carry counts/ids only, never addresses or bodies.
 */
object ProviderSync {
    private const val TAG = "SniptProvider"
    private const val PREFS = "snipt_sync"
    private const val KEY_IMPORTED = "inbox_imported_v1"

    fun writeInbox(ctx: Context, address: String, body: String, receivedMs: Long, sentMs: Long, subId: Int): Long {
        val v = ContentValues().apply {
            put(Telephony.Sms.ADDRESS, address)
            put(Telephony.Sms.BODY, body)
            put(Telephony.Sms.DATE, receivedMs)
            put(Telephony.Sms.DATE_SENT, sentMs)
            put(Telephony.Sms.READ, 0)
            put(Telephony.Sms.SEEN, 0)
            put(Telephony.Sms.TYPE, Telephony.Sms.MESSAGE_TYPE_INBOX)
            if (subId >= 0) put(Telephony.Sms.SUBSCRIPTION_ID, subId)
        }
        return insert(ctx, Telephony.Sms.Inbox.CONTENT_URI, v)
    }

    fun writeOutbox(ctx: Context, address: String, body: String, subId: Int): Long {
        val v = ContentValues().apply {
            put(Telephony.Sms.ADDRESS, address)
            put(Telephony.Sms.BODY, body)
            put(Telephony.Sms.DATE, System.currentTimeMillis())
            put(Telephony.Sms.READ, 1)
            put(Telephony.Sms.SEEN, 1)
            put(Telephony.Sms.TYPE, Telephony.Sms.MESSAGE_TYPE_OUTBOX)
            if (subId >= 0) put(Telephony.Sms.SUBSCRIPTION_ID, subId)
        }
        return insert(ctx, Telephony.Sms.Outbox.CONTENT_URI, v)
    }

    private fun insert(ctx: Context, uri: android.net.Uri, v: ContentValues): Long = try {
        ctx.contentResolver.insert(uri, v)?.let { ContentUris.parseId(it) } ?: -1L
    } catch (e: Exception) {
        Log.w(TAG, "provider insert failed: ${e.javaClass.simpleName}")
        -1L
    }

    /** Outgoing result: MESSAGE_TYPE_SENT / MESSAGE_TYPE_FAILED. */
    fun setType(ctx: Context, providerId: Long, type: Int) {
        if (providerId < 0) return
        update(ctx, providerId, ContentValues().apply { put(Telephony.Sms.TYPE, type) })
    }

    fun setDelivered(ctx: Context, providerId: Long) {
        if (providerId < 0) return
        update(ctx, providerId, ContentValues().apply {
            put(Telephony.Sms.STATUS, Telephony.Sms.STATUS_COMPLETE)
        })
    }

    private fun update(ctx: Context, id: Long, v: ContentValues) {
        try {
            ctx.contentResolver.update(ContentUris.withAppendedId(Telephony.Sms.CONTENT_URI, id), v, null, null)
        } catch (e: Exception) {
            Log.w(TAG, "provider update failed: ${e.javaClass.simpleName}")
        }
    }

    /** Only the default SMS app's provider writes take effect. */
    fun roleHeld(ctx: Context): Boolean {
        val rm = ctx.getSystemService(android.app.role.RoleManager::class.java) ?: return false
        return rm.isRoleAvailable(android.app.role.RoleManager.ROLE_SMS) &&
            rm.isRoleHeld(android.app.role.RoleManager.ROLE_SMS)
    }

    /**
     * Table of a row id: `content://sms` or `content://mms` (S15). The two
     * `_id` namespaces are separate; the core tells which one an id is in.
     */
    private fun base(mms: Boolean): android.net.Uri =
        if (mms) Telephony.Mms.CONTENT_URI else Telephony.Sms.CONTENT_URI

    /** (exists, read) of one provider row, or null when it cannot be queried. */
    private fun rowState(ctx: Context, id: Long, mms: Boolean): Pair<Boolean, Boolean>? = try {
        // Telephony.Sms.READ and Telephony.BaseMmsColumns.READ are both "read".
        ctx.contentResolver.query(
            ContentUris.withAppendedId(base(mms), id),
            arrayOf(Telephony.Sms._ID, Telephony.Sms.READ), null, null, null
        )?.use { c -> if (c.moveToFirst()) true to (c.getInt(1) != 0) else false to false }
    } catch (e: Exception) {
        null
    }

    /**
     * Marks rows read and verifies each by reading it back: (id, ok). A row
     * that no longer exists counts as ok (nothing left to keep in sync).
     * Without the SMS role every row fails (the provider ignores the write).
     */
    fun markRead(ctx: Context, ids: List<Long>, mms: Boolean = false): List<Pair<Long, Boolean>> {
        val role = roleHeld(ctx)
        val v = ContentValues().apply {
            // Same column names in both tables (Telephony.BaseMmsColumns READ/SEEN).
            put(Telephony.Sms.READ, 1)
            put(Telephony.Sms.SEEN, 1)
        }
        return ids.filter { it >= 0 }.map { id ->
            if (!role) return@map id to false
            try {
                ctx.contentResolver.update(ContentUris.withAppendedId(base(mms), id), v, null, null)
            } catch (e: Exception) {
                // verified below
            }
            val st = rowState(ctx, id, mms)
            id to (st != null && (!st.first || st.second))
        }.also { r -> r.count { !it.second }.let { if (it > 0) Log.w(TAG, "mark read failed for $it row(s)") } }
    }

    /** Deletes rows and verifies each is gone: (id, ok). */
    fun delete(ctx: Context, ids: List<Long>, mms: Boolean = false): List<Pair<Long, Boolean>> {
        val role = roleHeld(ctx)
        return ids.filter { it >= 0 }.map { id ->
            if (!role) return@map id to false
            try {
                ctx.contentResolver.delete(ContentUris.withAppendedId(base(mms), id), null, null)
            } catch (e: Exception) {
                // verified below
            }
            val st = rowState(ctx, id, mms)
            id to (st != null && !st.first)
        }.also { r -> r.count { !it.second }.let { if (it > 0) Log.w(TAG, "delete failed for $it row(s)") } }
    }

    fun isImported(ctx: Context): Boolean =
        ctx.getSharedPreferences(PREFS, Context.MODE_PRIVATE).getBoolean(KEY_IMPORTED, false)

    /**
     * Import of provider inbox rows the core is missing: the first-run
     * import, and the explicit "Import missing messages" action (R4 #5
     * recovery for pre-v2 receipts the automatic replay cannot tell from v1
     * deletions). Only rows the core neither holds nor deleted locally
     * (tombstones) are ingested, with their provider id and READ flag, so it
     * is idempotent and never resurrects a message deleted in Snipt. Then a
     * reconcile pass. Returns (inserted, skipped) or null when the provider
     * is not readable.
     */
    @Synchronized
    fun importInbox(ctx: Context): Pair<Long, Long>? {
        if (!NativeCore.ensure(ctx)) return null
        val ids = ArrayList<Long>()
        try {
            ctx.contentResolver.query(
                Telephony.Sms.Inbox.CONTENT_URI,
                arrayOf(Telephony.Sms._ID),
                null, null, Telephony.Sms._ID + " ASC"
            )
        } catch (e: Exception) {
            null
        }?.use { c -> while (c.moveToNext()) ids.add(c.getLong(0)) } ?: return null
        val missing = ArrayList<Long>()
        for (chunk in ids.chunked(5000)) {
            val r = NativeCore.providerMissing(JSONArray(chunk).toString()) ?: return null
            val a = JSONArray(r)
            for (k in 0 until a.length()) missing.add(a.getLong(k))
        }
        val inserted = replay(ctx, missing)
        reconcile(ctx)
        ctx.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit().putBoolean(KEY_IMPORTED, true).apply()
        Log.i(TAG, "inbox import: inserted=$inserted skipped=${ids.size - inserted}")
        return inserted to (ids.size - inserted)
    }

    /** Inbox rows this young may still be mid-receipt (SMS_DELIVER running). */
    private const val REPLAY_GRACE_MS = 30_000L

    /**
     * Provider -> store: deletions and read flags made outside Snipt, plus a
     * replay of inbox rows whose ingest was lost.
     *
     * Race-free order: (1) the core's local candidate set is captured first,
     * (2) the provider is queried, (3) the core deletes only candidates whose
     * provider id is absent *and* still bound to the same row. A message
     * received between (1) and (3) is not a candidate, so it is never deleted
     * even though the provider snapshot lacks it. (4) Inbox rows the core
     * never ingested (and did not delete locally) are re-ingested with their
     * provider id (idempotent).
     */
    @Synchronized
    fun reconcile(ctx: Context) {
        if (!NativeCore.ensure(ctx)) return
        // Candidates and tombstones, both captured before the provider query.
        val snapshot = NativeCore.reconcileBegin()?.let { JSONObject(it) } ?: return
        val present = JSONArray()
        val read = JSONArray()
        val inbox = JSONArray()
        val cutoff = System.currentTimeMillis() - REPLAY_GRACE_MS
        val cursor = try {
            ctx.contentResolver.query(
                Telephony.Sms.CONTENT_URI,
                arrayOf(Telephony.Sms._ID, Telephony.Sms.READ, Telephony.Sms.TYPE, Telephony.Sms.DATE),
                null, null, null
            )
        } catch (e: Exception) {
            null
        } ?: return
        cursor.use { c ->
            while (c.moveToNext()) {
                val id = c.getLong(0)
                present.put(id)
                if (c.getInt(1) != 0) read.put(id)
                if (c.getInt(2) == Telephony.Sms.MESSAGE_TYPE_INBOX && c.getLong(3) < cutoff) inbox.put(id)
            }
        }
        val r = NativeCore.reconcile(
            JSONObject().put("candidates", snapshot.getJSONArray("candidates"))
                .put("tombstones", snapshot.getJSONArray("tombstones"))
                .put("present", present).put("read", read).put("inbox", inbox).toString()
        ) ?: return
        val rep = JSONObject(r)
        val missing = rep.optJSONArray("missing") ?: JSONArray()
        Log.i(TAG, "reconcile: deleted=${rep.optInt("deleted")} markedRead=${rep.optInt("markedRead")} missing=${missing.length()}")
        if (missing.length() > 0) replay(ctx, (0 until missing.length()).map { missing.getLong(it) })
    }

    /**
     * Re-ingest provider inbox rows by id (chunked IN queries), carrying the
     * provider READ flag. Returns the number stored.
     */
    private fun replay(ctx: Context, ids: List<Long>): Long {
        var inserted = 0L
        for (chunk in ids.chunked(200)) {
            val sel = Telephony.Sms._ID + " IN (" + chunk.joinToString(",") + ")"
            val batch = JSONArray()
            try {
                ctx.contentResolver.query(
                    Telephony.Sms.CONTENT_URI,
                    arrayOf(Telephony.Sms._ID, Telephony.Sms.ADDRESS, Telephony.Sms.BODY, Telephony.Sms.DATE, Telephony.Sms.SUBSCRIPTION_ID, Telephony.Sms.TYPE, Telephony.Sms.READ),
                    sel, null, null
                )
            } catch (e: Exception) {
                null
            }?.use { c ->
                while (c.moveToNext()) {
                    if (c.getInt(5) != Telephony.Sms.MESSAGE_TYPE_INBOX) continue
                    val address = c.getString(1) ?: continue
                    if (address.isBlank()) continue
                    val sub = if (c.isNull(4)) -1 else c.getInt(4)
                    batch.put(JSONObject().apply {
                        put("providerId", c.getLong(0))
                        put("read", c.getInt(6) != 0)
                        put("message", JSONObject().apply {
                            put("sender", address)
                            put("body", c.getString(2) ?: "")
                            put("isKnownContact", false)
                            put("simSlot", Subs.slotForSubId(ctx, sub))
                            put("timestampMs", c.getLong(3))
                        })
                    })
                }
            }
            if (batch.length() > 0) {
                NativeCore.ingestBatch(batch.toString())?.let { inserted += JSONObject(it).optLong("inserted") }
            }
        }
        Log.i(TAG, "replayed missed receipts: inserted=$inserted")
        return inserted
    }
}
