package app.snipt.telephony

import android.content.ContentUris
import android.content.ContentValues
import android.content.Context
import android.net.Uri
import android.provider.Telephony
import android.util.Log
import org.json.JSONObject

/**
 * Writes a retrieved MMS to `content://mms` (the default SMS app must write
 * incoming messages itself; MmsService does not persist for it). Columns
 * from Telephony.BaseMmsColumns / Mms.Addr / Mms.Part. `date` is in
 * seconds (as AOSP Messaging writes it). Addr/part sub-tables are addressed
 * as `content://mms/<id>/addr|part` (the getAddrUriForMessage /
 * getPartUriForMessage helpers are API 30; minSdk is 29). Binary part data
 * is streamed into the part row's URI. All-or-nothing: on any failure the
 * message row is deleted and -1 returned (the core still keeps the message).
 * Logs carry no content.
 */
object MmsProviderWriter {
    private const val TAG = "SniptMms"
    private const val PDU_FROM = 0x89 // PduHeaders.FROM (137)
    private const val PDU_TO = 0x97 // PduHeaders.TO (151)
    private const val CHARSET_UTF8 = 106
    private const val M_RETRIEVE_CONF = 132

    fun writeInbox(ctx: Context, r: JSONObject, pdu: ByteArray): Long {
        if (!ProviderSync.roleHeld(ctx)) return -1
        val cr = ctx.contentResolver
        var id = -1L
        return try {
            val from = r.getString("from")
            val threadId = Telephony.Threads.getOrCreateThreadId(ctx, from)
            val parts = r.getJSONArray("parts")
            var textOnly = true
            for (i in 0 until parts.length()) {
                val ct = parts.getJSONObject(i).getString("contentType")
                if (ct != "text/plain" && ct != "application/smil") textOnly = false
            }
            val v = ContentValues().apply {
                put(Telephony.Mms.MESSAGE_BOX, Telephony.Mms.MESSAGE_BOX_INBOX)
                put(Telephony.Mms.THREAD_ID, threadId)
                put(Telephony.Mms.DATE, r.getLong("receivedMs") / 1000L)
                if (!r.isNull("dateSent")) put(Telephony.Mms.DATE_SENT, r.getLong("dateSent"))
                put(Telephony.Mms.READ, 0)
                put(Telephony.Mms.SEEN, 0)
                put(Telephony.Mms.MESSAGE_TYPE, M_RETRIEVE_CONF)
                put(Telephony.Mms.MMS_VERSION, r.optInt("mmsVersion", 0x12))
                put(Telephony.Mms.CONTENT_TYPE, r.optString("contentType"))
                put(Telephony.Mms.MESSAGE_SIZE, pdu.size)
                put(Telephony.Mms.TEXT_ONLY, if (textOnly) 1 else 0)
                if (!r.isNull("subject")) {
                    put(Telephony.Mms.SUBJECT, r.getString("subject"))
                    put(Telephony.Mms.SUBJECT_CHARSET, CHARSET_UTF8)
                }
                if (!r.isNull("messageId")) put(Telephony.Mms.MESSAGE_ID, r.getString("messageId"))
                if (!r.isNull("transactionId")) put(Telephony.Mms.TRANSACTION_ID, r.getString("transactionId"))
                if (!r.isNull("subId")) put(Telephony.Mms.SUBSCRIPTION_ID, r.getLong("subId"))
            }
            id = cr.insert(Telephony.Mms.Inbox.CONTENT_URI, v)?.let { ContentUris.parseId(it) } ?: return -1
            val addrUri = Uri.parse("content://mms/$id/addr")
            fun addr(a: String, type: Int) {
                cr.insert(addrUri, ContentValues().apply {
                    put(Telephony.Mms.Addr.ADDRESS, a)
                    put(Telephony.Mms.Addr.TYPE, type)
                    put(Telephony.Mms.Addr.CHARSET, CHARSET_UTF8)
                }) ?: error("addr")
            }
            addr(from, PDU_FROM)
            val to = r.optJSONArray("to")
            for (i in 0 until (to?.length() ?: 0)) addr(to!!.getString(i), PDU_TO)
            val partUri = Uri.parse("content://mms/$id/part")
            for (i in 0 until parts.length()) {
                val p = parts.getJSONObject(i)
                val ct = p.getString("contentType")
                val pv = ContentValues().apply {
                    put(Telephony.Mms.Part.SEQ, p.getInt("seq"))
                    put(Telephony.Mms.Part.CONTENT_TYPE, ct)
                    if (!p.isNull("name")) {
                        put(Telephony.Mms.Part.NAME, p.getString("name"))
                        put(Telephony.Mms.Part.FILENAME, p.getString("name"))
                    }
                    if (!p.isNull("charset")) put(Telephony.Mms.Part.CHARSET, p.getLong("charset"))
                    if (!p.isNull("contentId")) put(Telephony.Mms.Part.CONTENT_ID, "<" + p.getString("contentId") + ">")
                    if (!p.isNull("contentLocation")) put(Telephony.Mms.Part.CONTENT_LOCATION, p.getString("contentLocation"))
                    if (ct == "text/plain") {
                        put(Telephony.Mms.Part.TEXT, p.optString("text", ""))
                        if (p.isNull("charset")) put(Telephony.Mms.Part.CHARSET, CHARSET_UTF8)
                    }
                }
                val u = cr.insert(partUri, pv) ?: error("part")
                if (ct != "text/plain") {
                    val off = p.getInt("offset")
                    val len = p.getInt("len")
                    require(off >= 0 && len >= 0 && off + len <= pdu.size)
                    cr.openOutputStream(u)?.use { it.write(pdu, off, len) } ?: error("part data")
                }
            }
            Log.i(TAG, "provider MMS row written")
            id
        } catch (e: Exception) {
            Log.w(TAG, "provider MMS write failed: ${e.javaClass.simpleName}")
            if (id >= 0) try {
                cr.delete(ContentUris.withAppendedId(Telephony.Mms.CONTENT_URI, id), null, null)
            } catch (_: Exception) {
            }
            -1
        }
    }
}
