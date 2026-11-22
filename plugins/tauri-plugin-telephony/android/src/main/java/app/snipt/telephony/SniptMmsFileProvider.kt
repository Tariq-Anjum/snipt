package app.snipt.telephony

import androidx.core.content.FileProvider

/**
 * Own subclass so the manifest entry never merges with another library's
 * androidx FileProvider declaration. Serves only cacheDir/snipt-mms/
 * (res/xml/snipt_mms_paths.xml) for MMS transfers.
 */
class SniptMmsFileProvider : FileProvider(R.xml.snipt_mms_paths)
