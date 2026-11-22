package app.snipt.telephony

import android.content.Context
import android.util.Log
import java.io.File

/**
 * JNI bridge into the Rust core (src-tauri/src/jni_bridge.rs, library
 * `snipt_lib`). Works with no Activity/WebView: receivers call [ensure] and
 * then the functions below on a background thread. JSON in/out, null/false on
 * failure. Never log arguments or results (they carry message bodies/OTPs).
 */
object NativeCore {
    private const val TAG = "SniptNative"
    /** Same file as Tauri's `app_data_dir()/snipt.db` (dataDir on Android). */
    private const val DB_FILE = "snipt.db"

    @Volatile private var ready = false
    @Volatile private var keyInstalled = false
    private val lock = Any()

    /**
     * Opens (or joins) the shared core and installs the Keystore-unwrapped
     * vault key. SMS receipt works as soon as the core is open; the vault
     * stays not-ready in Rust (vault calls fail, no payloads are issued under
     * a throwaway key) until the key is installed, and installation is
     * retried on later calls if the Keystore failed.
     */
    fun ensure(context: Context): Boolean {
        if (ready && keyInstalled) return true
        synchronized(lock) {
            if (ready && keyInstalled) return true
            if (!ready) {
                try {
                    System.loadLibrary("snipt_lib")
                } catch (e: UnsatisfiedLinkError) {
                    Log.e(TAG, "native library unavailable")
                    return false
                }
            }
            val dek: ByteArray? = try {
                KeystoreWrap.loadOrCreateDek(context)
            } catch (e: Exception) {
                Log.w(TAG, "vault key unavailable: ${e.javaClass.simpleName}")
                null
            }
            try {
                val ok = init(File(context.dataDir, DB_FILE).absolutePath, dek)
                if (ok) {
                    ready = true
                    if (dek != null) keyInstalled = true
                }
            } finally {
                dek?.fill(0)
            }
            if (!ready) Log.e(TAG, "native core init failed")
            return ready
        }
    }

    @JvmStatic external fun init(dbPath: String, dek: ByteArray?): Boolean
    @JvmStatic external fun ingest(json: String): String?
    @JvmStatic external fun ingestBatch(json: String): String?
    @JvmStatic external fun queueOutgoing(json: String): String?
    /** Opens a send attempt for a queued message; -1 on failure. */
    @JvmStatic external fun beginSend(id: Long, parts: Int): Long
    /** Binds the attempt's provider row (once). */
    @JvmStatic external fun bindSend(attemptId: Long, providerId: Long): Boolean
    /** kind: sent|delivery, outcome: ok|failed|unknown -> SendUpdate JSON. */
    @JvmStatic external fun sendPartResult(attemptId: Long, part: Int, kind: String, outcome: String): String?
    /** Monotonic failure for a message whose hand-off failed -> SendUpdate JSON. */
    @JvmStatic external fun failSend(id: Long): String?
    /** Notification action source check -> {msgId, threadId, providerId?, isRead} or null if stale. */
    @JvmStatic external fun actionTarget(id: Long, address: String): String?
    @JvmStatic external fun markRead(id: Long): Boolean
    @JvmStatic external fun deleteMessage(id: Long): Boolean
    @JvmStatic external fun otpCode(id: Long): String?
    /** Local (id, providerId) candidates, captured before the provider query. */
    @JvmStatic external fun reconcileBegin(): String?
    @JvmStatic external fun reconcile(json: String): String?
    @JvmStatic external fun emit(event: String)

    // ---- MMS receive (S15); PDUs are passed as byte arrays, JSON out ----
    /** WAP push data -> {action: download|duplicate, pendingId, location, subId}. */
    @JvmStatic external fun mmsNotification(pdu: ByteArray, subId: Int, slot: Int): String?
    /** Counts a download attempt; -1 when the MMS is no longer pending. */
    @JvmStatic external fun mmsAttempt(pendingId: Long): Long
    /** {retry: [notice], placeholders: [ingest]} */
    @JvmStatic external fun mmsMaintenance(): String?
    /** M-Retrieve.conf -> headers + parts (PDU ranges) for the provider write. */
    @JvmStatic external fun mmsRetrieved(pendingId: Long, pdu: ByteArray): String?
    /** -> {ingest, thumbs: [{partId, offset, len}]}; mmsId -1 if the provider write failed. */
    @JvmStatic external fun mmsIngest(pendingId: Long, pdu: ByteArray, mmsId: Long): String?
    /** -> {outcome: retry|gaveUp|ignored, placeholder?} */
    @JvmStatic external fun mmsFailed(pendingId: Long): String?
    @JvmStatic external fun mmsThumb(partId: Long, jpeg: ByteArray): Boolean
    /** M-NotifyResp.ind bytes (status Retrieved). */
    @JvmStatic external fun mmsNotifyResp(pendingId: Long): ByteArray?
}
