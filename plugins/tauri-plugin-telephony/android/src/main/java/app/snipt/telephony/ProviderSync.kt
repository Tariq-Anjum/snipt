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

    fun markRead(ctx: Context, ids: List<Long>): Int {
        var n = 0
        val v = ContentValues().apply {
            put(Telephony.Sms.READ, 1)
            put(Telephony.Sms.SEEN, 1)
        }
        for (id in ids) {
            if (id < 0) continue
            n += try {
                ctx.contentResolver.update(ContentUris.withAppendedId(Telephony.Sms.CONTENT_URI, id), v, null, null)
            } catch (e: Exception) {
                0
            }
        }
        return n
    }

    fun delete(ctx: Context, ids: List<Long>): Int {
        var n = 0
        for (id in ids) {
            if (id < 0) continue
            n += try {
                ctx.contentResolver.delete(ContentUris.withAppendedId(Telephony.Sms.CONTENT_URI, id), null, null)
            } catch (e: Exception) {
                0
            }
        }
        return n
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

    /** Provider -> store: deletions and read flags made outside Snipt. */
    @Synchronized
    fun reconcile(ctx: Context) {
        if (!NativeCore.ensure(ctx)) return
        val present = JSONArray()
        val read = JSONArray()
        val cursor = try {
            ctx.contentResolver.query(
                Telephony.Sms.CONTENT_URI,
                arrayOf(Telephony.Sms._ID, Telephony.Sms.READ),
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
            }
        }
        val r = NativeCore.reconcile(JSONObject().put("present", present).put("read", read).toString())
        if (r != null) Log.i(TAG, "reconcile: $r")
    }
}
