package app.snipt.telephony

import android.app.PendingIntent
import android.appwidget.AppWidgetManager
import android.appwidget.AppWidgetProvider
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.content.res.ColorStateList
import android.os.Build
import android.text.format.DateUtils
import android.view.View
import android.widget.RemoteViews
import org.json.JSONArray
import org.json.JSONObject
import java.text.NumberFormat
import java.util.Locale
import kotlin.math.roundToLong

/**
 * Home-screen widgets ("Liquid Glass" look: translucent rounded card, 1 dp
 * highlight stroke; RemoteViews cannot blur, so the glass is a gradient).
 *
 * Data: a small JSON snapshot in app-private SharedPreferences, written by
 * the web UI (`updateWidgets`, built from masked, OTP-free data) and bumped
 * natively when an SMS arrives while the UI is closed. One-time codes are
 * never written: an OTP message shows as "Verification code".
 */
object SniptWidgets {
    const val EXTRA_TAB = "app.snipt.widget.TAB"
    private const val PREFS = "snipt_widgets"
    private const val KEY = "snapshot"
    private const val MAX_LATEST = 3
    private const val MAX_TEXT = 120

    fun store(ctx: Context, snapshot: String): Boolean {
        val json = try {
            JSONObject(snapshot)
        } catch (_: Exception) {
            return false
        }
        if (json.optInt("v") != 1) return false
        ctx.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit().putString(KEY, json.toString()).apply()
        refreshAll(ctx)
        return true
    }

    /** Called by the SMS receiver after a successful ingest (no UI running). */
    fun noteIncoming(ctx: Context, title: String, body: String, hasOtp: Boolean, ts: Long) {
        val prefs = ctx.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
        val snap = load(ctx) ?: JSONObject().put("v", 1).put("previews", true).put("unread", 0)
        snap.put("unread", snap.optInt("unread") + 1)
        val shown = if (hasOtp) ctx.getString(R.string.snipt_widget_code) else body
        val old = snap.optJSONArray("latest") ?: JSONArray()
        val next = JSONArray().put(JSONObject().put("title", clip(title)).put("body", clip(shown)).put("ts", ts))
        for (i in 0 until minOf(old.length(), MAX_LATEST - 1)) next.put(old.get(i))
        snap.put("latest", next).put("updatedAt", ts)
        prefs.edit().putString(KEY, snap.toString()).apply()
        refreshAll(ctx)
    }

    private fun clip(s: String) = if (s.length > MAX_TEXT) s.substring(0, MAX_TEXT) else s

    private fun load(ctx: Context): JSONObject? = try {
        ctx.getSharedPreferences(PREFS, Context.MODE_PRIVATE).getString(KEY, null)?.let { JSONObject(it) }
    } catch (_: Exception) {
        null
    }

    fun refreshAll(ctx: Context) {
        val mgr = AppWidgetManager.getInstance(ctx)
        val sms = mgr.getAppWidgetIds(ComponentName(ctx, SmsWidgetProvider::class.java))
        if (sms.isNotEmpty()) renderSms(ctx, mgr, sms)
        val budget = mgr.getAppWidgetIds(ComponentName(ctx, BudgetWidgetProvider::class.java))
        if (budget.isNotEmpty()) renderBudget(ctx, mgr, budget)
    }

    private fun openApp(ctx: Context, tab: String, req: Int): PendingIntent? {
        val i = ctx.packageManager.getLaunchIntentForPackage(ctx.packageName) ?: return null
        i.flags = Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP
        i.putExtra(EXTRA_TAB, tab)
        return PendingIntent.getActivity(ctx, req, i, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
    }

    private fun inr(v: Double): String {
        val f = NumberFormat.getNumberInstance(Locale("en", "IN"))
        f.maximumFractionDigits = 0
        return "₹" + f.format(v.roundToLong())
    }

    // ---- SMS widget -------------------------------------------------------

    private val ROW_IDS = intArrayOf(R.id.snipt_w_row0, R.id.snipt_w_row1, R.id.snipt_w_row2)
    private val TITLE_IDS = intArrayOf(R.id.snipt_w_title0, R.id.snipt_w_title1, R.id.snipt_w_title2)
    private val BODY_IDS = intArrayOf(R.id.snipt_w_body0, R.id.snipt_w_body1, R.id.snipt_w_body2)
    private val TIME_IDS = intArrayOf(R.id.snipt_w_time0, R.id.snipt_w_time1, R.id.snipt_w_time2)

    fun renderSms(ctx: Context, mgr: AppWidgetManager, ids: IntArray) {
        val snap = load(ctx)
        val rv = RemoteViews(ctx.packageName, R.layout.snipt_widget_sms)
        val unread = snap?.optInt("unread") ?: 0
        rv.setTextViewText(R.id.snipt_w_unread, if (unread > 99) "99+" else unread.toString())
        rv.setViewVisibility(R.id.snipt_w_unread, if (unread > 0) View.VISIBLE else View.GONE)
        val previews = snap?.optBoolean("previews", true) ?: true
        val latest = snap?.optJSONArray("latest") ?: JSONArray()
        val now = System.currentTimeMillis()
        for (i in ROW_IDS.indices) {
            val o = latest.optJSONObject(i)
            if (o == null) {
                rv.setViewVisibility(ROW_IDS[i], View.GONE)
                continue
            }
            rv.setViewVisibility(ROW_IDS[i], View.VISIBLE)
            rv.setTextViewText(TITLE_IDS[i], o.optString("title"))
            rv.setTextViewText(BODY_IDS[i], if (previews) o.optString("body") else ctx.getString(R.string.snipt_widget_hidden))
            val ts = o.optLong("ts")
            rv.setTextViewText(
                TIME_IDS[i],
                if (ts > 0) DateUtils.getRelativeTimeSpanString(ts, now, DateUtils.MINUTE_IN_MILLIS, DateUtils.FORMAT_ABBREV_RELATIVE) else ""
            )
        }
        rv.setViewVisibility(R.id.snipt_w_empty, if (latest.length() == 0) View.VISIBLE else View.GONE)
        openApp(ctx, "inbox", 41)?.let { rv.setOnClickPendingIntent(R.id.snipt_w_root, it) }
        mgr.updateAppWidget(ids, rv)
    }

    // ---- Budget widget ----------------------------------------------------

    private val CAT_ROWS = intArrayOf(R.id.snipt_b_cat0, R.id.snipt_b_cat1, R.id.snipt_b_cat2)
    private val CAT_NAMES = intArrayOf(R.id.snipt_b_cat0_name, R.id.snipt_b_cat1_name, R.id.snipt_b_cat2_name)
    private val CAT_AMTS = intArrayOf(R.id.snipt_b_cat0_amt, R.id.snipt_b_cat1_amt, R.id.snipt_b_cat2_amt)
    private val CAT_BARS = intArrayOf(R.id.snipt_b_cat0_bar, R.id.snipt_b_cat1_bar, R.id.snipt_b_cat2_bar)

    fun renderBudget(ctx: Context, mgr: AppWidgetManager, ids: IntArray) {
        val rv = RemoteViews(ctx.packageName, R.layout.snipt_widget_budget)
        val b = load(ctx)?.optJSONObject("budget")
        if (b == null) {
            rv.setTextViewText(R.id.snipt_b_label, ctx.getString(R.string.snipt_widget_budget))
            rv.setTextViewText(R.id.snipt_b_spent, "—")
            rv.setTextViewText(R.id.snipt_b_of, ctx.getString(R.string.snipt_widget_open_app))
            rv.setTextViewText(R.id.snipt_b_left, "")
            rv.setProgressBar(R.id.snipt_b_bar, 100, 0, false)
            for (r in CAT_ROWS) rv.setViewVisibility(r, View.GONE)
        } else {
            val spent = b.optDouble("spent", 0.0)
            val limit = b.optDouble("limit", 0.0)
            val pct = if (limit > 0) (spent / limit * 100).coerceIn(0.0, 999.0) else 0.0
            val over = limit > 0 && spent > limit
            rv.setTextViewText(R.id.snipt_b_label, b.optString("label", ctx.getString(R.string.snipt_widget_budget)))
            rv.setTextViewText(R.id.snipt_b_spent, inr(spent))
            rv.setTextViewText(R.id.snipt_b_of, ctx.getString(R.string.snipt_widget_of, inr(limit)))
            rv.setTextViewText(
                R.id.snipt_b_left,
                if (over) ctx.getString(R.string.snipt_widget_over, inr(spent - limit))
                else ctx.getString(R.string.snipt_widget_left, inr(limit - spent))
            )
            rv.setTextViewText(R.id.snipt_b_pct, "${pct.roundToLong()}%")
            rv.setProgressBar(R.id.snipt_b_bar, 1000, (pct * 10).toInt().coerceAtMost(1000), false)
            tint(ctx, rv, R.id.snipt_b_bar, over || pct >= 90)
            val cats = b.optJSONArray("categories") ?: JSONArray()
            for (i in CAT_ROWS.indices) {
                val c = cats.optJSONObject(i)
                if (c == null) {
                    rv.setViewVisibility(CAT_ROWS[i], View.GONE)
                    continue
                }
                val cs = c.optDouble("spent", 0.0)
                val cl = c.optDouble("limit", 0.0)
                val cp = if (cl > 0) (cs / cl * 1000).coerceIn(0.0, 1000.0).toInt() else 0
                rv.setViewVisibility(CAT_ROWS[i], View.VISIBLE)
                rv.setTextViewText(CAT_NAMES[i], c.optString("name"))
                rv.setTextViewText(CAT_AMTS[i], "${inr(cs)} / ${inr(cl)}")
                rv.setProgressBar(CAT_BARS[i], 1000, cp, false)
                tint(ctx, rv, CAT_BARS[i], cl > 0 && cs >= cl * 0.9)
            }
        }
        openApp(ctx, "finance", 42)?.let { rv.setOnClickPendingIntent(R.id.snipt_b_root, it) }
        mgr.updateAppWidget(ids, rv)
    }

    private fun tint(ctx: Context, rv: RemoteViews, id: Int, warn: Boolean) {
        if (Build.VERSION.SDK_INT < 31) return
        val color = ctx.getColor(if (warn) R.color.snipt_w_warn else R.color.snipt_w_accent)
        rv.setColorStateList(id, "setProgressTintList", ColorStateList.valueOf(color))
    }
}

class SmsWidgetProvider : AppWidgetProvider() {
    override fun onUpdate(context: Context, mgr: AppWidgetManager, ids: IntArray) {
        SniptWidgets.renderSms(context, mgr, ids)
    }
}

class BudgetWidgetProvider : AppWidgetProvider() {
    override fun onUpdate(context: Context, mgr: AppWidgetManager, ids: IntArray) {
        SniptWidgets.renderBudget(context, mgr, ids)
    }
}
