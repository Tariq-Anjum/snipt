package app.snipt.telephony

import android.Manifest
import android.annotation.SuppressLint
import android.app.Activity
import android.app.role.RoleManager
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.provider.DocumentsContract
import android.provider.OpenableColumns
import android.os.Build
import android.os.PowerManager
import android.view.MotionEvent
import android.view.WindowManager
import android.webkit.WebView
import androidx.activity.ComponentActivity
import androidx.activity.OnBackPressedCallback
import androidx.activity.result.ActivityResult
import androidx.appcompat.app.AppCompatActivity
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import app.tauri.annotation.ActivityCallback
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSArray
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin

@InvokeArg
class SendArgs {
    var id: Long = -1
    var address: String = ""
    var body: String = ""
    var simSlot: Int = 0
}

@InvokeArg
class CopyArgs {
    var text: String = ""
    var clearAfterMs: Long? = null
}

@InvokeArg
class SecureArgs {
    var scope: String = "vault"
    var on: Boolean = false
}

@InvokeArg
class ToggleArgs {
    var on: Boolean = false
}

@InvokeArg
class BackArgs {
    var enabled: Boolean = false
}

@InvokeArg
class ProviderIdsArgs {
    var providerIds: LongArray = LongArray(0)
}

@InvokeArg
class SafSaveArgs {
    var name: String = "snipt-backup.mysms.enc"
    var content: String = ""
}

/**
 * Activity-bound part of the telephony shell (commands from Rust). The
 * background paths (receivers, notification actions) do not depend on this
 * class or the WebView.
 */
@TauriPlugin
class TelephonyPlugin(private val activity: Activity) : Plugin(activity) {
    data class Compose(val address: String, val body: String)

    companion object {
        @Volatile var pendingCompose: Compose? = null
        /** Largest backup the shell hands to Rust (a phone-sized limit; the core accepts up to 256 MiB). */
        const val MAX_BACKUP_BYTES = 64 * 1024 * 1024
    }

    private var secureVault = false
    private var secureGlobal = false
    private var backCallback: OnBackPressedCallback? = null
    private var boost: DisplayBoost? = null
    private var webView: WebView? = null
    private var insetsJs: String? = null

    @SuppressLint("ClickableViewAccessibility")
    override fun load(webView: WebView) {
        this.webView = webView
        val ctx = activity.applicationContext
        // Keystore DEK into the shared core before the UI lists vault items;
        // then provider sync. The first onResume of a cold start runs before
        // the plugin is loaded, and the role may have been granted outside
        // the app (Settings / adb), so import or reconcile here too.
        Thread {
            if (NativeCore.ensure(ctx) && roleHeld()) {
                if (!ProviderSync.isImported(ctx)) ProviderSync.importInbox(ctx) else ProviderSync.reconcile(ctx)
            }
        }.start()

        // Predictive back: disabled unless the web layer has something to close.
        (activity as? ComponentActivity)?.let { a ->
            val cb = object : OnBackPressedCallback(false) {
                override fun handleOnBackPressed() {
                    if (NativeCore.ensure(ctx)) NativeCore.emit("back")
                }
            }
            a.onBackPressedDispatcher.addCallback(a, cb)
            backCallback = cb
        }

        // Edge-to-edge insets -> CSS vars (--inset-*, CSS px).
        ViewCompat.setOnApplyWindowInsetsListener(webView) { v, insets ->
            val i = insets.getInsets(WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout())
            val d = v.resources.displayMetrics.density
            insetsJs = "(function(s){s.setProperty('--inset-top','${i.top / d}px');" +
                "s.setProperty('--inset-bottom','${i.bottom / d}px');" +
                "s.setProperty('--inset-left','${i.left / d}px');" +
                "s.setProperty('--inset-right','${i.right / d}px');})(document.documentElement.style)"
            pushInsets()
            insets
        }
        ViewCompat.requestApplyInsets(webView)
        // The page may load after the first insets pass: re-push a few times.
        for (ms in longArrayOf(800, 2_000, 5_000)) webView.postDelayed({ pushInsets() }, ms)

        val b = DisplayBoost(activity, webView)
        boost = b
        webView.setOnTouchListener { _, ev ->
            if (ev.actionMasked == MotionEvent.ACTION_DOWN || ev.actionMasked == MotionEvent.ACTION_MOVE) b.poke()
            false
        }
    }

    private fun pushInsets() {
        val js = insetsJs ?: return
        webView?.post { webView?.evaluateJavascript(js, null) }
    }

    override fun onResume(activity: AppCompatActivity) {
        pushInsets()
        val ctx = activity.applicationContext
        if (roleHeld()) Thread { ProviderSync.reconcile(ctx) }.start()
    }

    private fun roleHeld(): Boolean {
        val rm = activity.getSystemService(RoleManager::class.java) ?: return false
        return rm.isRoleAvailable(RoleManager.ROLE_SMS) && rm.isRoleHeld(RoleManager.ROLE_SMS)
    }

    private fun granted(p: String) =
        activity.checkSelfPermission(p) == PackageManager.PERMISSION_GRANTED

    private fun hasTelephony() =
        activity.packageManager.hasSystemFeature(PackageManager.FEATURE_TELEPHONY_MESSAGING) ||
            activity.packageManager.hasSystemFeature(PackageManager.FEATURE_TELEPHONY)

    @Command
    fun status(invoke: Invoke) {
        val subs = Subs.active(activity)
        val arr = JSArray()
        for (s in subs) {
            arr.put(JSObject().apply {
                put("subId", s.subId)
                put("slot", s.slot)
                put("carrier", s.carrier)
            })
        }
        invoke.resolve(JSObject().apply {
            put("defaultSmsRole", roleHeld())
            put("canSend", hasTelephony() && granted(Manifest.permission.SEND_SMS))
            put("simSlots", subs.size.coerceIn(0, 2))
            put("subscriptions", arr)
            put("mms", false)
            put("notifications", Notifier.canPost(activity))
        })
    }

    @Command
    fun requestDefaultSmsRole(invoke: Invoke) {
        val rm = activity.getSystemService(RoleManager::class.java)
        if (rm == null || !rm.isRoleAvailable(RoleManager.ROLE_SMS)) {
            invoke.reject("SMS role is not available on this device")
            return
        }
        if (rm.isRoleHeld(RoleManager.ROLE_SMS)) {
            afterRole()
            invoke.resolve(JSObject().put("held", true))
            return
        }
        startActivityForResult(invoke, rm.createRequestRoleIntent(RoleManager.ROLE_SMS), "roleResult")
    }

    @ActivityCallback
    private fun roleResult(invoke: Invoke, result: ActivityResult) {
        val held = roleHeld()
        if (held) afterRole()
        invoke.resolve(JSObject().put("held", held))
    }

    /** Runtime prompts the role does not cover, then the first-run import. */
    private fun afterRole() {
        val want = mutableListOf<String>()
        if (Build.VERSION.SDK_INT >= 33 && !granted(Manifest.permission.POST_NOTIFICATIONS)) {
            want.add(Manifest.permission.POST_NOTIFICATIONS)
        }
        if (!granted(Manifest.permission.READ_PHONE_STATE)) want.add(Manifest.permission.READ_PHONE_STATE)
        if (want.isNotEmpty()) activity.runOnUiThread { activity.requestPermissions(want.toTypedArray(), 7301) }
        val ctx = activity.applicationContext
        if (!ProviderSync.isImported(ctx)) Thread { ProviderSync.importInbox(ctx) }.start()
    }

    @Command
    fun importInbox(invoke: Invoke) {
        if (!roleHeld() && !granted(Manifest.permission.READ_SMS)) {
            invoke.reject("SMS access not granted")
            return
        }
        val ctx = activity.applicationContext
        Thread {
            val r = ProviderSync.importInbox(ctx)
            if (r == null) invoke.reject("SMS provider not readable")
            else invoke.resolve(JSObject().put("imported", r.first).put("skipped", r.second))
        }.start()
    }

    @Command
    fun send(invoke: Invoke) {
        val a = invoke.parseArgs(SendArgs::class.java)
        if (a.id < 0 || a.address.isBlank()) {
            invoke.reject("invalid send request")
            return
        }
        if (!granted(Manifest.permission.SEND_SMS)) {
            invoke.reject("SEND_SMS not granted")
            return
        }
        val ctx = activity.applicationContext
        Thread {
            if (SmsSender.send(ctx, a.id, a.address, a.body, a.simSlot)) invoke.resolve(JSObject().put("ok", true))
            else invoke.reject("send failed")
        }.start()
    }

    private fun results(r: List<Pair<Long, Boolean>>): JSObject {
        val arr = JSArray()
        for ((id, ok) in r) arr.put(JSObject().put("providerId", id).put("ok", ok))
        return JSObject().put("results", arr)
    }

    /** Per-row results (`ok` = verified read / gone); failures are kept by Rust. */
    @Command
    fun providerMarkRead(invoke: Invoke) {
        val a = invoke.parseArgs(ProviderIdsArgs::class.java)
        val ctx = activity.applicationContext
        Thread { invoke.resolve(results(ProviderSync.markRead(ctx, a.providerIds.toList()))) }.start()
    }

    @Command
    fun providerDelete(invoke: Invoke) {
        val a = invoke.parseArgs(ProviderIdsArgs::class.java)
        val ctx = activity.applicationContext
        Thread { invoke.resolve(results(ProviderSync.delete(ctx, a.providerIds.toList()))) }.start()
    }

    /** Resolves only after the clip is set (the ownership-checked wipe is armed). */
    @Command
    fun copySensitive(invoke: Invoke) {
        val a = invoke.parseArgs(CopyArgs::class.java)
        SecureClipboard.copy(activity, a.text, a.clearAfterMs ?: SecureClipboard.DEFAULT_CLEAR_MS) { ok ->
            if (ok) invoke.resolve(JSObject().put("ok", true)) else invoke.reject("copy failed")
        }
    }

    /**
     * FLAG_SECURE for `vault` (sensitive screen showing) or `global` (user
     * preference). Resolves after the flag change ran on the UI thread, with
     * `ok` = the window's actual FLAG_SECURE state read back, so the web UI
     * enables reveal controls only on a confirmed secure window.
     */
    @Command
    fun setSecure(invoke: Invoke) {
        val a = invoke.parseArgs(SecureArgs::class.java)
        activity.runOnUiThread {
            if (a.scope == "global") secureGlobal = a.on else secureVault = a.on
            val on = secureGlobal || secureVault
            try {
                if (on) activity.window.addFlags(WindowManager.LayoutParams.FLAG_SECURE)
                else activity.window.clearFlags(WindowManager.LayoutParams.FLAG_SECURE)
                val actual = (activity.window.attributes.flags and WindowManager.LayoutParams.FLAG_SECURE) != 0
                invoke.resolve(JSObject().put("ok", actual))
            } catch (e: Exception) {
                invoke.reject("could not change the secure flag")
            }
        }
    }

    @Command
    fun displayBoost(invoke: Invoke) {
        val a = invoke.parseArgs(ToggleArgs::class.java)
        activity.runOnUiThread { boost?.set(a.on) }
        invoke.resolve(JSObject().put("ok", true))
    }

    @Command
    fun setBackHandler(invoke: Invoke) {
        val a = invoke.parseArgs(BackArgs::class.java)
        activity.runOnUiThread { backCallback?.isEnabled = a.enabled }
        invoke.resolve(JSObject().put("ok", true))
    }

    @Command
    fun powerState(invoke: Invoke) {
        val pm = activity.getSystemService(PowerManager::class.java)
        invoke.resolve(JSObject().apply {
            put("powerSave", pm?.isPowerSaveMode == true)
            put("thermal", pm?.currentThermalStatus ?: 0)
        })
    }

    @Command
    fun takeCompose(invoke: Invoke) {
        val c = pendingCompose
        pendingCompose = null
        if (c == null) invoke.resolve(JSObject().put("present", false))
        else invoke.resolve(JSObject().put("present", true).put("address", c.address).put("body", c.body))
    }

    // ---- Storage Access Framework (encrypted backup) -------------------
    // The payload is an already-encrypted envelope. It is streamed through
    // ContentResolver and never logged; errors carry no URI, name or content.

    @Command
    fun safSave(invoke: Invoke) {
        val a = invoke.parseArgs(SafSaveArgs::class.java)
        val intent = Intent(Intent.ACTION_CREATE_DOCUMENT).apply {
            addCategory(Intent.CATEGORY_OPENABLE)
            type = "application/octet-stream"
            putExtra(Intent.EXTRA_TITLE, a.name)
        }
        startActivityForResult(invoke, intent, "safSaveResult")
    }

    @ActivityCallback
    private fun safSaveResult(invoke: Invoke, result: ActivityResult) {
        val uri = result.data?.data
        if (result.resultCode != Activity.RESULT_OK || uri == null) {
            invoke.resolve(JSObject().put("cancelled", true))
            return
        }
        val a = invoke.parseArgs(SafSaveArgs::class.java)
        val resolver = activity.contentResolver
        Thread {
            try {
                // "wt" truncates when the user picked an existing document.
                val os = resolver.openOutputStream(uri, "wt") ?: throw java.io.IOException()
                os.bufferedWriter(Charsets.UTF_8).use { it.write(a.content) }
                invoke.resolve(JSObject().put("cancelled", false))
            } catch (e: Exception) {
                try { DocumentsContract.deleteDocument(resolver, uri) } catch (_: Exception) {}
                invoke.reject("Could not write the backup file")
            }
        }.start()
    }

    @Command
    fun safOpen(invoke: Invoke) {
        val intent = Intent(Intent.ACTION_OPEN_DOCUMENT).apply {
            addCategory(Intent.CATEGORY_OPENABLE)
            // Cloud providers report arbitrary types for a ".mysms.enc" file.
            type = "*/*"
            putExtra(
                Intent.EXTRA_MIME_TYPES,
                arrayOf("application/octet-stream", "application/json", "text/plain")
            )
        }
        startActivityForResult(invoke, intent, "safOpenResult")
    }

    @ActivityCallback
    private fun safOpenResult(invoke: Invoke, result: ActivityResult) {
        val uri = result.data?.data
        if (result.resultCode != Activity.RESULT_OK || uri == null) {
            invoke.resolve(JSObject().put("cancelled", true))
            return
        }
        val resolver = activity.contentResolver
        Thread {
            try {
                val name = displayName(uri)
                val input = resolver.openInputStream(uri) ?: throw java.io.IOException()
                val out = java.io.ByteArrayOutputStream()
                input.use { ins ->
                    val buf = ByteArray(64 * 1024)
                    while (true) {
                        val n = ins.read(buf)
                        if (n < 0) break
                        if (out.size() + n > MAX_BACKUP_BYTES) {
                            invoke.reject("Backup file is too large")
                            return@Thread
                        }
                        out.write(buf, 0, n)
                    }
                }
                invoke.resolve(
                    JSObject().put("cancelled", false).put("name", name)
                        .put("content", out.toString(Charsets.UTF_8.name()))
                )
            } catch (e: Exception) {
                invoke.reject("Could not read the backup file")
            }
        }.start()
    }

    private fun displayName(uri: Uri): String = try {
        activity.contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use {
            if (it.moveToFirst()) it.getString(0) else null
        } ?: ""
    } catch (_: Exception) {
        ""
    }
}
