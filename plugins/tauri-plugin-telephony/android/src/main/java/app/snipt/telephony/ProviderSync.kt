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
    private const val BATCH = 300

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

    /** (exists, read) of one provider row, or null when it cannot be queried. */
    private fun rowState(ctx: Context, id: Long): Pair<Boolean, Boolean>? = try {
        ctx.contentResolver.query(
            ContentUris.withAppendedId(Telephony.Sms.CONTENT_URI, id),
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
    fun markRead(ctx: Context, ids: List<Long>): List<Pair<Long, Boolean>> {
        val role = roleHeld(ctx)
        val v = ContentValues().apply {
            put(Telephony.Sms.READ, 1)
            put(Telephony.Sms.SEEN, 1)
        }
        return ids.filter { it >= 0 }.map { id ->
            if (!role) return@map id to false
            try {
                ctx.contentResolver.update(ContentUris.withAppendedId(Telephony.Sms.CONTENT_URI, id), v, null, null)
            } catch (e: Exception) {
                // verified below
            }
            val st = rowState(ctx, id)
            id to (st != null && (!st.first || st.second))
        }.also { r -> r.count { !it.second }.let { if (it > 0) Log.w(TAG, "mark read failed for $it row(s)") } }
    }

    /** Deletes rows and verifies each is gone: (id, ok). */
    fun delete(ctx: Context, ids: List<Long>): List<Pair<Long, Boolean>> {
        val role = roleHeld(ctx)
        return ids.filter { it >= 0 }.map { id ->
            if (!role) return@map id to false
            try {
                ctx.contentResolver.delete(ContentUris.withAppendedId(Telephony.Sms.CONTENT_URI, id), null, null)
            } catch (e: Exception) {
                // verified below
            }
            val st = rowState(ctx, id)
            id to (st != null && !st.first)
        }.also { r -> r.count { !it.second }.let { if (it > 0) Log.w(TAG, "delete failed for $it row(s)") } }
    }

    fun isImported(ctx: Context): Boolean =
        ctx.getSharedPreferences(PREFS, Context.MODE_PRIVATE).getBoolean(KEY_IMPORTED, false)

    /**
     * First-run import of the existing provider inbox into the core (with
     * provider ids, so it is idempotent), then a reconcile pass so read flags
     * match. Returns (inserted, skipped) or null when the provider is not
     * readable.
     */
    @Synchronized
    fun importInbox(ctx: Context): Pair<Long, Long>? {
        if (!NativeCore.ensure(ctx)) return null
        val cursor = try {
            ctx.contentResolver.query(
                Telephony.Sms.Inbox.CONTENT_URI,
                arrayOf(Telephony.Sms._ID, Telephony.Sms.ADDRESS, Telephony.Sms.BODY, Telephony.Sms.DATE, Telephony.Sms.SUBSCRIPTION_ID),
                null, null, Telephony.Sms._ID + " ASC"
            )
        } catch (e: Exception) {
            null
        } ?: return null
        var inserted = 0L
        var skipped = 0L
        cursor.use { c ->
            var batch = JSONArray()
            fun flush() {
                if (batch.length() == 0) return
                NativeCore.ingestBatch(batch.toString())?.let {
                    val o = JSONObject(it)
                    inserted += o.optLong("inserted")
                    skipped += o.optLong("skipped") + o.optLong("failed")
                }
                batch = JSONArray()
            }
            while (c.moveToNext()) {
                val address = c.getString(1) ?: continue
                val body = c.getString(2) ?: ""
                if (address.isBlank()) continue
                val sub = if (c.isNull(4)) -1 else c.getInt(4)
                batch.put(JSONObject().apply {
                    put("providerId", c.getLong(0))
                    put("message", JSONObject().apply {
                        put("sender", address)
                        put("body", body)
                        put("isKnownContact", false)
                        put("simSlot", Subs.slotForSubId(ctx, sub))
                        put("timestampMs", c.getLong(3))
                    })
                })
                if (batch.length() >= BATCH) flush()
            }
            flush()
        }
        reconcile(ctx)
        ctx.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit().putBoolean(KEY_IMPORTED, true).apply()
        Log.i(TAG, "inbox import: inserted=$inserted skipped=$skipped")
        return inserted to skipped
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
        val candidates = NativeCore.reconcileBegin()?.let { JSONArray(it) } ?: return
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
            JSONObject().put("candidates", candidates).put("present", present)
                .put("read", read).put("inbox", inbox).toString()
        ) ?: return
        val rep = JSONObject(r)
        val missing = rep.optJSONArray("missing") ?: JSONArray()
        Log.i(TAG, "reconcile: deleted=${rep.optInt("deleted")} markedRead=${rep.optInt("markedRead")} missing=${missing.length()}")
        if (missing.length() > 0) replay(ctx, (0 until missing.length()).map { missing.getLong(it) })
    }

    /** Re-ingest provider inbox rows by id (chunked IN queries). */
    private fun replay(ctx: Context, ids: List<Long>) {
        var inserted = 0L
        for (chunk in ids.chunked(200)) {
            val sel = Telephony.Sms._ID + " IN (" + chunk.joinToString(",") + ")"
            val batch = JSONArray()
            try {
                ctx.contentResolver.query(
                    Telephony.Sms.CONTENT_URI,
                    arrayOf(Telephony.Sms._ID, Telephony.Sms.ADDRESS, Telephony.Sms.BODY, Telephony.Sms.DATE, Telephony.Sms.SUBSCRIPTION_ID, Telephony.Sms.TYPE),
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
    }
}
