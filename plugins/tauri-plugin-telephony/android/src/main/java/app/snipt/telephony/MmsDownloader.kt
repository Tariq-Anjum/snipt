package app.snipt.telephony

import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.net.Uri
import android.os.Build
import android.telephony.SmsManager
import android.util.Log
import androidx.core.content.FileProvider
import org.json.JSONObject
import java.io.ByteArrayOutputStream
import java.io.File

/**
 * MMS download via the platform (S15). As the default SMS app Snipt calls
 * `SmsManager.downloadMultimediaMessage(context, locationUrl, contentUri,
 * configOverrides, downloadedIntent)` (API 21). The platform MmsService
 * fetches the PDU over the carrier MMS APN in its own process (so no
 * INTERNET permission here) and writes it through `contentUri`: the system
 * (MmsServiceBroker) grants the phone process read/write on that URI on our
 * behalf, so it must be a content URI of a provider with
 * `grantUriPermissions` - our non-exported FileProvider over
 * `cacheDir/snipt-mms/`. The result arrives at [MmsDownloadReceiver]
 * (explicit, non-exported, immutable PendingIntent whose data URI carries
 * the pending id). MmsService does not persist for the default app (auto-
 * persist pref off), so Snipt writes `content://mms` itself
 * ([MmsProviderWriter]) and then acknowledges with M-NotifyResp.ind
 * (status Retrieved) through `sendMultimediaMessage`, as AOSP Messaging does.
 */
object MmsDownloader {
    private const val TAG = "SniptMms"
    const val SCHEME = "snipt-mms"
    private const val ABANDONED_MS = 24L * 60 * 60 * 1000
    private const val DIR = "snipt-mms"
    private const val MAX_PDU = 8 * 1024 * 1024
    private const val THUMB_MAX_SIDE = 480
    private const val THUMB_MAX_BYTES = 256 * 1024
    private const val MAX_PIXELS = 40_000_000L

    private val inFlight = HashSet<Long>()

    private fun authority(ctx: Context) = ctx.packageName + ".snipt.mms"

    private fun dir(ctx: Context): File = File(ctx.cacheDir, DIR).apply { mkdirs() }

    fun downloadFile(ctx: Context, pendingId: Long) = File(dir(ctx), "dl-$pendingId.pdu")
    private fun ackFile(ctx: Context, pendingId: Long) = File(dir(ctx), "ack-$pendingId.pdu")

    private fun uriFor(ctx: Context, f: File): Uri = FileProvider.getUriForFile(ctx, authority(ctx), f)

    private fun manager(ctx: Context, subId: Int): SmsManager =
        if (Build.VERSION.SDK_INT >= 31) {
            val base = ctx.getSystemService(SmsManager::class.java)
            if (subId >= 0) base.createForSubscriptionId(subId) else base
        } else {
            @Suppress("DEPRECATION")
            if (subId >= 0) SmsManager.getSmsManagerForSubscriptionId(subId) else SmsManager.getDefault()
        }

    private fun resultIntent(ctx: Context, kind: String, pendingId: Long): PendingIntent {
        val i = Intent(ctx, MmsDownloadReceiver::class.java)
            .setData(Uri.parse("$SCHEME://$kind/$pendingId"))
        return PendingIntent.getBroadcast(
            ctx, 0, i, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT
        )
    }

    /** Start (or retry) the platform download of a pending MMS. */
    fun start(ctx: Context, pendingId: Long, location: String, subId: Int) {
        synchronized(inFlight) {
            if (!inFlight.add(pendingId)) return
        }
        if (NativeCore.mmsAttempt(pendingId) < 0) {
            done(pendingId)
            return
        }
        try {
            val f = downloadFile(ctx, pendingId)
            f.delete()
            f.createNewFile()
            manager(ctx, subId).downloadMultimediaMessage(
                ctx, location, uriFor(ctx, f), null, resultIntent(ctx, "download", pendingId)
            )
            Log.i(TAG, "MMS download requested")
        } catch (e: Exception) {
            Log.w(TAG, "MMS download not started: ${e.javaClass.simpleName}")
            failed(ctx, pendingId)
        }
    }

    fun done(pendingId: Long) {
        synchronized(inFlight) { inFlight.remove(pendingId) }
    }

    /** Retries due downloads; posts placeholders for expired/exhausted ones. */
    fun maintenance(ctx: Context) {
        expireAbandoned(ctx)
        if (!NativeCore.ensure(ctx)) return
        val m = NativeCore.mmsMaintenance()?.let { JSONObject(it) } ?: return
        val ph = m.optJSONArray("placeholders")
        for (i in 0 until (ph?.length() ?: 0)) {
            val o = ph!!.getJSONObject(i)
            Notifier.incoming(ctx, o, 0)
        }
        val retry = m.optJSONArray("retry")
        for (i in 0 until (retry?.length() ?: 0)) {
            val o = retry!!.getJSONObject(i)
            val sub = if (o.isNull("subId")) -1 else o.optInt("subId", -1)
            start(ctx, o.getLong("pendingId"), o.getString("location"), sub)
        }
    }

    /** Per-attempt files of one pending MMS (download PDU and NotifyResp ack). */
    fun discardFiles(ctx: Context, pendingId: Long) {
        downloadFile(ctx, pendingId).delete()
        ackFile(ctx, pendingId).delete()
    }

    /**
     * Cache files left by an attempt whose callback never ran (process
     * death, revoked role): removed after [ABANDONED_MS]. A live download
     * finishes or fails long before that (the core times attempts out at
     * 10 minutes).
     */
    private fun expireAbandoned(ctx: Context) {
        val cutoff = System.currentTimeMillis() - ABANDONED_MS
        File(ctx.cacheDir, DIR).listFiles()?.forEach { f ->
            if (f.isFile && f.lastModified() < cutoff) f.delete()
        }
    }

    /** A download attempt failed: the core decides retry vs placeholder. */
    fun failed(ctx: Context, pendingId: Long) {
        done(pendingId)
        downloadFile(ctx, pendingId).delete()
        val r = NativeCore.mmsFailed(pendingId)?.let { JSONObject(it) } ?: return
        Log.i(TAG, "MMS download failed; outcome=${r.optString("outcome")}")
        r.optJSONObject("placeholder")?.let { Notifier.incoming(ctx, it, 0) }
    }

    /** The PDU is in the download file: provider write, ingest, thumbnails, ack. */
    fun downloaded(ctx: Context, pendingId: Long) {
        done(pendingId)
        val f = downloadFile(ctx, pendingId)
        try {
            if (!NativeCore.ensure(ctx)) return
            val len = f.length()
            if (len <= 0 || len > MAX_PDU) {
                Log.w(TAG, "downloaded MMS empty or too large")
                failed(ctx, pendingId)
                return
            }
            val pdu = f.readBytes()
            val parsed = NativeCore.mmsRetrieved(pendingId, pdu)?.let { JSONObject(it) }
            if (parsed == null) {
                Log.w(TAG, "downloaded MMS rejected")
                failed(ctx, pendingId)
                return
            }
            val mmsId = MmsProviderWriter.writeInbox(ctx, parsed, pdu)
            val out = NativeCore.mmsIngest(pendingId, pdu, mmsId)?.let { JSONObject(it) } ?: run {
                Log.w(TAG, "MMS ingest rejected")
                return
            }
            val thumbs = out.optJSONArray("thumbs")
            for (i in 0 until (thumbs?.length() ?: 0)) {
                val t = thumbs!!.getJSONObject(i)
                thumbnail(pdu, t.getInt("offset"), t.getInt("len"))?.let {
                    NativeCore.mmsThumb(t.getLong("partId"), it)
                }
            }
            val ingest = out.getJSONObject("ingest")
            Log.i(TAG, "MMS stored: provider=${mmsId >= 0} thumbs=${thumbs?.length() ?: 0}")
            val sub = if (parsed.isNull("subId")) -1 else parsed.optInt("subId", -1)
            Notifier.incoming(ctx, ingest, Subs.slotForSubId(ctx, sub))
            acknowledge(ctx, pendingId, sub)
        } finally {
            f.delete()
        }
    }

    /** M-NotifyResp.ind (Retrieved); failures are harmless (MMSC may re-notify; deduped). */
    private fun acknowledge(ctx: Context, pendingId: Long, subId: Int) {
        try {
            val bytes = NativeCore.mmsNotifyResp(pendingId) ?: return
            val f = ackFile(ctx, pendingId)
            f.writeBytes(bytes)
            manager(ctx, subId).sendMultimediaMessage(
                ctx, uriFor(ctx, f), null, null, resultIntent(ctx, "ack", pendingId)
            )
        } catch (e: Exception) {
            // No result callback will come for an ack that was never handed off.
            ackFile(ctx, pendingId).delete()
            Log.w(TAG, "MMS acknowledge not sent: ${e.javaClass.simpleName}")
        }
    }

    fun acked(ctx: Context, pendingId: Long, resultCode: Int) {
        ackFile(ctx, pendingId).delete()
        Log.i(TAG, "MMS acknowledge result=$resultCode")
    }

    /**
     * JPEG thumbnail (longest side <= 480 px, <= 256 KiB), decoded with a
     * power-of-two sample size from the bounds first; absurd dimensions
     * (decompression bombs) are skipped.
     */
    fun thumbnail(pdu: ByteArray, offset: Int, len: Int): ByteArray? = try {
        if (offset < 0 || len <= 0 || offset + len > pdu.size) null else {
            val b = BitmapFactory.Options().apply { inJustDecodeBounds = true }
            BitmapFactory.decodeByteArray(pdu, offset, len, b)
            val w = b.outWidth
            val h = b.outHeight
            if (w <= 0 || h <= 0 || w.toLong() * h > MAX_PIXELS) null else {
                var sample = 1
                while (maxOf(w, h) / (sample * 2) >= THUMB_MAX_SIDE) sample *= 2
                val bmp = BitmapFactory.decodeByteArray(
                    pdu, offset, len, BitmapFactory.Options().apply { inSampleSize = sample }
                )
                if (bmp == null) null else {
                    val scale = minOf(1f, THUMB_MAX_SIDE.toFloat() / maxOf(bmp.width, bmp.height))
                    val scaled = if (scale < 1f) Bitmap.createScaledBitmap(
                        bmp, maxOf(1, (bmp.width * scale).toInt()), maxOf(1, (bmp.height * scale).toInt()), true
                    ) else bmp
                    var out: ByteArray? = null
                    for (q in intArrayOf(80, 60, 40)) {
                        val bos = ByteArrayOutputStream()
                        scaled.compress(Bitmap.CompressFormat.JPEG, q, bos)
                        if (bos.size() <= THUMB_MAX_BYTES) {
                            out = bos.toByteArray()
                            break
                        }
                    }
                    if (scaled !== bmp) scaled.recycle()
                    bmp.recycle()
                    out
                }
            }
        }
    } catch (t: Throwable) {
        Log.w(TAG, "thumbnail failed: ${t.javaClass.simpleName}")
        null
    }
}
