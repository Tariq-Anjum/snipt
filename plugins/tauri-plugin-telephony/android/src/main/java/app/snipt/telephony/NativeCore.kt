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
    private val lock = Any()

    fun ensure(context: Context): Boolean {
        if (ready) return true
        synchronized(lock) {
            if (ready) return true
            try {
                System.loadLibrary("snipt_lib")
            } catch (e: UnsatisfiedLinkError) {
                Log.e(TAG, "native library unavailable")
                return false
            }
            val dek: ByteArray? = try {
                KeystoreWrap.loadOrCreateDek(context)
            } catch (e: Exception) {
                Log.w(TAG, "vault key unavailable: ${e.javaClass.simpleName}")
                null
            }
            try {
                ready = init(File(context.dataDir, DB_FILE).absolutePath, dek)
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
    @JvmStatic external fun setSendStatus(id: Long, status: String, providerId: Long): Boolean
    @JvmStatic external fun markRead(id: Long): Boolean
    @JvmStatic external fun deleteMessage(id: Long): Boolean
    @JvmStatic external fun otpCode(id: Long): String?
    @JvmStatic external fun reconcile(json: String): String?
    @JvmStatic external fun emit(event: String)
}
