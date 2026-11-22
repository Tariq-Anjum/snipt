package app.snipt.telephony

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import java.security.KeyStore
import java.security.SecureRandom
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/**
 * OTP-vault data key (32 random bytes) wrapped by a non-exportable
 * AndroidKeyStore AES-256-GCM key (ARCHITECTURE §5). The wrapped blob
 * (iv || ciphertext+tag) lives in app-private prefs (allowBackup=false). The
 * raw DEK only exists transiently and is handed to Rust over JNI; callers wipe
 * the returned array. Rust never sees the Keystore key.
 */
object KeystoreWrap {
    private const val ALIAS = "snipt_vault_wrap_v1"
    private const val PREFS = "snipt_secure"
    private const val KEY_WRAPPED = "vault_dek_wrapped_v1"
    private const val TRANSFORM = "AES/GCM/NoPadding"

    @Synchronized
    fun loadOrCreateDek(context: Context): ByteArray {
        val prefs = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
        val key = keystoreKey()
        prefs.getString(KEY_WRAPPED, null)?.let { stored ->
            try {
                val raw = Base64.decode(stored, Base64.NO_WRAP)
                val c = Cipher.getInstance(TRANSFORM)
                c.init(Cipher.DECRYPT_MODE, key, GCMParameterSpec(128, raw.copyOfRange(0, 12)))
                val dek = c.doFinal(raw, 12, raw.size - 12)
                if (dek.size == 32) return dek
                dek.fill(0)
            } catch (e: Exception) {
                // Keystore key lost/rotated (e.g. after a restore): issue a new
                // DEK. Only in-memory vault payloads depended on the old one.
            }
        }
        val dek = ByteArray(32).also { SecureRandom().nextBytes(it) }
        val c = Cipher.getInstance(TRANSFORM)
        c.init(Cipher.ENCRYPT_MODE, key)
        val blob = c.iv + c.doFinal(dek)
        prefs.edit().putString(KEY_WRAPPED, Base64.encodeToString(blob, Base64.NO_WRAP)).commit()
        return dek
    }

    private fun keystoreKey(): SecretKey {
        val ks = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        (ks.getKey(ALIAS, null) as? SecretKey)?.let { return it }
        val gen = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore")
        gen.init(
            KeyGenParameterSpec.Builder(
                ALIAS,
                KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT
            )
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setKeySize(256)
                .build()
        )
        return gen.generateKey()
    }
}
