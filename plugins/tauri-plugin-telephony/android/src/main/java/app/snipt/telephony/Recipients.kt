package app.snipt.telephony

import android.content.Intent
import android.net.Uri

/**
 * `sms:` / `smsto:` / `mms:` / `mmsto:` URI parsing (RFC 5724 style:
 * `sms:<recipients>?body=<text>`).
 *
 * The *encoded* scheme-specific part is split first (recipients vs query,
 * then `&`-separated parameters) and each piece is decoded exactly once, so
 * an encoded `&` (`%26`) stays inside the body and a literal `%25` is not
 * decoded twice. Snipt v1 composes to a single recipient: with several
 * (`,` or `;`) only the first is used.
 */
object Recipients {
    val SCHEMES = setOf("sms", "smsto", "mms", "mmsto")

    fun supported(uri: Uri?): Boolean = uri?.scheme?.lowercase() in SCHEMES

    private fun parts(uri: Uri): Pair<String, String>? {
        if (!supported(uri)) return null
        val raw = uri.encodedSchemeSpecificPart ?: return null
        val q = raw.indexOf('?')
        return if (q < 0) raw to "" else raw.substring(0, q) to raw.substring(q + 1)
    }

    fun fromUri(uri: Uri): String? {
        val (addr, _) = parts(uri) ?: return null
        val first = addr.removePrefix("//").split(',', ';').firstOrNull() ?: return null
        return Uri.decode(first).trim().takeIf { it.isNotEmpty() }
    }

    fun bodyFromUri(uri: Uri): String? {
        val (_, query) = parts(uri) ?: return null
        if (query.isEmpty()) return null
        return query.split('&')
            .firstOrNull { it.startsWith("body=", ignoreCase = true) }
            ?.substring(5)
            ?.let { Uri.decode(it.replace('+', ' ')) }
    }

    /**
     * Draft from an external intent, validated: SENDTO/VIEW need a supported
     * scheme; SEND needs `text/plain`. Anything else yields null.
     */
    fun draftFrom(intent: Intent?): Pair<String, String>? {
        val i = intent ?: return null
        return when (i.action) {
            Intent.ACTION_SENDTO, Intent.ACTION_VIEW -> {
                val uri = i.data ?: return null
                if (!supported(uri)) return null
                val body = i.getStringExtra("sms_body") ?: i.getStringExtra(Intent.EXTRA_TEXT) ?: bodyFromUri(uri)
                (fromUri(uri) ?: "") to (body ?: "")
            }
            Intent.ACTION_SEND -> {
                if (i.type?.lowercase()?.startsWith("text/plain") != true) return null
                val text = i.getStringExtra(Intent.EXTRA_TEXT) ?: return null
                val addr = i.data?.takeIf { supported(it) }?.let { fromUri(it) } ?: ""
                addr to text
            }
            else -> null
        }
    }
}
