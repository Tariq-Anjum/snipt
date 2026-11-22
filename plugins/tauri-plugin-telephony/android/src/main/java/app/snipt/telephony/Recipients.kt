package app.snipt.telephony

import android.net.Uri

/** `sms:` / `smsto:` / `mms:` / `mmsto:` URI parsing (recipient list + ?body=). */
object Recipients {
    fun fromUri(uri: Uri): String? {
        val ssp = uri.schemeSpecificPart ?: return null
        val addr = ssp.substringBefore('?').removePrefix("//").trim()
        // Multiple recipients (`,` or `;`): Snipt v1 composes to the first.
        return addr.split(',', ';').firstOrNull()?.trim()?.takeIf { it.isNotEmpty() }
    }

    fun bodyFromUri(uri: Uri): String? {
        val ssp = uri.schemeSpecificPart ?: return null
        val q = ssp.substringAfter('?', "")
        return q.split('&').firstOrNull { it.startsWith("body=") }
            ?.removePrefix("body=")
            ?.let { Uri.decode(it) }
    }
}
