//! JNI surface for `app.snipt.telephony.NativeCore` (Kotlin). Used by the
//! background receivers (SMS_DELIVER, send status, notification actions) so
//! receipt works with no Activity or WebView; it shares the process-wide
//! [`AppState::shared`] core (one SQLite connection, one OTP vault) with the
//! Tauri window when both are alive.
//!
//! Rules: every entry point runs inside `catch_unwind` (no panic crosses the
//! FFI boundary); failures return `null`/`false` and are never logged with
//! message content; JSON in, JSON out (camelCase, same types as the commands).

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::OnceLock;

use jni::JNIEnv;
use jni::objects::{JByteArray, JClass, JString};
use jni::sys::{JNI_FALSE, JNI_TRUE, jboolean, jlong, jstring};
use serde::{Deserialize, Serialize};
use sms_store::{IncomingSms, OutgoingSms, SendStatus};
use zeroize::Zeroizing;

use crate::core::AppState;
use crate::error::{CmdError, CmdResult};

static STATE: OnceLock<AppState> = OnceLock::new();
type Emitter = Box<dyn Fn(&str, serde_json::Value) + Send + Sync>;
static EMITTER: OnceLock<Emitter> = OnceLock::new();

/// Called from the Tauri setup: lets native events reach the webview
/// (`listen('sms-received' | 'send-status' | 'sms-changed' | 'back')`).
pub fn set_emitter(f: Emitter) {
    let _ = EMITTER.set(f);
}

/// Called from the Tauri setup with the window's core, so a JNI call that
/// happens before `NativeCore.init` still reaches the same state.
pub fn set_state(state: AppState) {
    let _ = STATE.set(state);
}

fn emit(event: &str, payload: serde_json::Value) {
    if let Some(e) = EMITTER.get() {
        e(event, payload);
    }
}

fn state() -> CmdResult<&'static AppState> {
    STATE
        .get()
        .ok_or_else(|| CmdError("native core not initialised".into()))
}

fn jstr(env: &mut JNIEnv, s: &JString) -> CmdResult<String> {
    if s.is_null() {
        return Err(CmdError::invalid("null string"));
    }
    env.get_string(s)
        .map(Into::into)
        .map_err(|_| CmdError::invalid("bad string"))
}

fn out_json<T: Serialize>(env: &mut JNIEnv, v: &T) -> jstring {
    match serde_json::to_string(v) {
        Ok(s) => env
            .new_string(s)
            .map(|j| j.into_raw())
            .unwrap_or(std::ptr::null_mut()),
        Err(_) => std::ptr::null_mut(),
    }
}

/// Runs `f`, turning errors and panics into `None`.
fn guard<T>(f: impl FnOnce() -> CmdResult<T>) -> Option<T> {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(Ok(v)) => Some(v),
        _ => None,
    }
}

fn to_jbool(v: Option<bool>) -> jboolean {
    if v == Some(true) { JNI_TRUE } else { JNI_FALSE }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct IngestIn {
    message: IncomingSms,
    #[serde(default)]
    provider_id: Option<i64>,
}

/// What the receiver needs to post a notification. `body` is redacted (the
/// OTP is bullets); the code is fetched only by the Copy OTP action.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct IngestOut {
    inserted: bool,
    notify: bool,
    title: String,
    thread_id: i64,
    msg_id: i64,
    body: String,
    has_otp: bool,
    category: sms_types::Category,
}

fn ingest_one(input: &IngestIn) -> CmdResult<IngestOut> {
    let r = state()?.with_core(|c| c.ingest_incoming(&input.message, input.provider_id))?;
    let n = r.notification;
    Ok(IngestOut {
        inserted: r.inserted,
        notify: n.notify,
        title: n.title,
        thread_id: r.thread.thread_id,
        msg_id: n.message.sms.id,
        body: n.message.sms.body,
        has_otp: n.message.otp.is_some(),
        category: r.thread.classification.category,
    })
}

/// `init(dbPath, dek)`: open (or join) the shared core; install the
/// Keystore-unwrapped 32-byte vault key when given. The Java array is copied
/// into a zeroizing buffer; Kotlin wipes its own copy.
#[unsafe(no_mangle)]
pub extern "system" fn Java_app_snipt_telephony_NativeCore_init<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    db_path: JString<'l>,
    dek: JByteArray<'l>,
) -> jboolean {
    to_jbool(guard(|| {
        let path = jstr(&mut env, &db_path)?;
        let st = match STATE.get() {
            Some(s) => s.clone(),
            None => {
                let s = AppState::shared(&path)?;
                set_state(s.clone());
                s
            }
        };
        if !dek.is_null() {
            let bytes = Zeroizing::new(
                env.convert_byte_array(&dek)
                    .map_err(|_| CmdError::invalid("dek"))?,
            );
            let key: [u8; 32] = bytes
                .as_slice()
                .try_into()
                .map_err(|_| CmdError::invalid("dek length"))?;
            let key = Zeroizing::new(key);
            st.with_core(|c| {
                c.set_vault_key(*key);
                Ok(())
            })?;
        }
        Ok(true)
    }))
}

/// `ingest({message, providerId}) -> IngestOut JSON | null`.
#[unsafe(no_mangle)]
pub extern "system" fn Java_app_snipt_telephony_NativeCore_ingest<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    json: JString<'l>,
) -> jstring {
    let r = guard(|| {
        let input: IngestIn = serde_json::from_str(&jstr(&mut env, &json)?)
            .map_err(|_| CmdError::invalid("ingest json"))?;
        let out = ingest_one(&input)?;
        if out.inserted {
            emit(
                "sms-received",
                serde_json::json!({ "threadId": out.thread_id }),
            );
        }
        Ok(out)
    });
    match r {
        Some(v) => out_json(&mut env, &v),
        None => std::ptr::null_mut(),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BatchOut {
    inserted: usize,
    skipped: usize,
    failed: usize,
}

/// `ingestBatch([{message, providerId}, ...]) -> {inserted, skipped, failed}`
/// (first-run provider import; idempotent through provider ids).
#[unsafe(no_mangle)]
pub extern "system" fn Java_app_snipt_telephony_NativeCore_ingestBatch<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    json: JString<'l>,
) -> jstring {
    let r = guard(|| {
        let items: Vec<IngestIn> = serde_json::from_str(&jstr(&mut env, &json)?)
            .map_err(|_| CmdError::invalid("batch json"))?;
        let mut out = BatchOut {
            inserted: 0,
            skipped: 0,
            failed: 0,
        };
        for it in &items {
            match ingest_one(it) {
                Ok(o) if o.inserted => out.inserted += 1,
                Ok(_) => out.skipped += 1,
                Err(_) => out.failed += 1,
            }
        }
        if out.inserted > 0 {
            emit("sms-changed", serde_json::json!({}));
        }
        Ok(out)
    });
    match r {
        Some(v) => out_json(&mut env, &v),
        None => std::ptr::null_mut(),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct QueuedOut {
    id: i64,
    thread_id: i64,
    address: String,
}

/// `queueOutgoing(OutgoingSms) -> {id, threadId, address} | null`
/// (notification inline reply, compose trampoline).
#[unsafe(no_mangle)]
pub extern "system" fn Java_app_snipt_telephony_NativeCore_queueOutgoing<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    json: JString<'l>,
) -> jstring {
    let r = guard(|| {
        let input: OutgoingSms = serde_json::from_str(&jstr(&mut env, &json)?)
            .map_err(|_| CmdError::invalid("outgoing json"))?;
        let q = state()?.with_core(|c| c.queue_outgoing(&input))?;
        emit("sms-changed", serde_json::json!({}));
        Ok(QueuedOut {
            id: q.message.sms.id,
            thread_id: q.thread.thread_id,
            address: q.message.sms.sender.clone(),
        })
    });
    match r {
        Some(v) => out_json(&mut env, &v),
        None => std::ptr::null_mut(),
    }
}

/// `setSendStatus(id, status, providerId)`; `providerId < 0` = none.
#[unsafe(no_mangle)]
pub extern "system" fn Java_app_snipt_telephony_NativeCore_setSendStatus<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    id: jlong,
    status: JString<'l>,
    provider_id: jlong,
) -> jboolean {
    to_jbool(guard(|| {
        let s = jstr(&mut env, &status)?;
        let st = SendStatus::parse(&s).ok_or_else(|| CmdError::invalid("status"))?;
        let pid = (provider_id >= 0).then_some(provider_id);
        let ok = state()?.with_core(|c| c.set_send_status(id, st, pid))?;
        emit(
            "send-status",
            serde_json::json!({ "id": id, "status": st.as_str() }),
        );
        Ok(ok)
    }))
}

/// Notification "Mark read".
#[unsafe(no_mangle)]
pub extern "system" fn Java_app_snipt_telephony_NativeCore_markRead<'l>(
    _env: JNIEnv<'l>,
    _class: JClass<'l>,
    id: jlong,
) -> jboolean {
    to_jbool(guard(|| {
        let ok = state()?.with_core(|c| c.mark_read(id))?;
        emit("sms-changed", serde_json::json!({}));
        Ok(ok)
    }))
}

/// Notification "Delete" (the provider row is deleted by Kotlin first).
#[unsafe(no_mangle)]
pub extern "system" fn Java_app_snipt_telephony_NativeCore_deleteMessage<'l>(
    _env: JNIEnv<'l>,
    _class: JClass<'l>,
    id: jlong,
) -> jboolean {
    to_jbool(guard(|| {
        let n = state()?.with_core(|c| c.delete_messages(&[id]))?;
        emit("sms-changed", serde_json::json!({}));
        Ok(n > 0)
    }))
}

/// Notification "Copy OTP": the plaintext code, or null. Kotlin puts it on
/// the clipboard with EXTRA_IS_SENSITIVE and never logs it.
#[unsafe(no_mangle)]
pub extern "system" fn Java_app_snipt_telephony_NativeCore_otpCode<'l>(
    env: JNIEnv<'l>,
    _class: JClass<'l>,
    id: jlong,
) -> jstring {
    match guard(|| state()?.with_core(|c| c.otp_code(id))).flatten() {
        Some(code) => {
            let code = Zeroizing::new(code);
            env.new_string(code.as_str())
                .map(|j| j.into_raw())
                .unwrap_or(std::ptr::null_mut())
        }
        None => std::ptr::null_mut(),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReconcileIn {
    present: Vec<i64>,
    read: Vec<i64>,
}

/// `reconcile({present, read}) -> {deleted, markedRead} | null`.
#[unsafe(no_mangle)]
pub extern "system" fn Java_app_snipt_telephony_NativeCore_reconcile<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    json: JString<'l>,
) -> jstring {
    let r = guard(|| {
        let input: ReconcileIn = serde_json::from_str(&jstr(&mut env, &json)?)
            .map_err(|_| CmdError::invalid("reconcile json"))?;
        let rep = state()?.with_core(|c| c.provider_reconcile(&input.present, &input.read))?;
        if rep.deleted + rep.marked_read > 0 {
            emit("sms-changed", serde_json::json!({}));
        }
        Ok(rep)
    });
    match r {
        Some(v) => out_json(&mut env, &v),
        None => std::ptr::null_mut(),
    }
}

/// Native UI events with no payload (`back`, `compose`, `sms-changed`).
/// Only known names pass.
#[unsafe(no_mangle)]
pub extern "system" fn Java_app_snipt_telephony_NativeCore_emit<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    event: JString<'l>,
) {
    let _ = guard(|| {
        let e = jstr(&mut env, &event)?;
        if matches!(e.as_str(), "back" | "compose" | "sms-changed") {
            emit(&e, serde_json::json!({}));
        }
        Ok(())
    });
}
