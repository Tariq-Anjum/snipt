//! JNI surface for `app.snipt.telephony.NativeCore` (Kotlin). Used by the
//! background receivers (SMS_DELIVER, send status, notification actions) so
//! receipt works with no Activity or WebView; it shares the process-wide
//! [`AppState::shared`] core (one SQLite connection, one OTP vault) with the
//! Tauri window when both are alive.
//!
//! Rules:
//! - Every entry point runs its whole body inside `catch_unwind`: argument
//!   conversion (`JString` -> `String`, byte arrays), the core call, JSON
//!   encoding and the `String` -> `jstring` conversion. No panic crosses the
//!   FFI boundary.
//! - Failure contract: `null` / `false` / `-1`, never a Java exception. If a
//!   JNI call failed and left a Java exception pending (e.g. allocation
//!   failure in `NewStringUTF`), it is cleared before returning, so Kotlin
//!   callers only ever see the null/false result.
//! - Nothing is logged; JSON in, JSON out (camelCase, same types as the
//!   commands).

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::OnceLock;

use jni::JNIEnv;
use jni::objects::{JByteArray, JClass, JString};
use jni::sys::jbyteArray;
use jni::sys::{JNI_FALSE, JNI_TRUE, jboolean, jint, jlong, jstring};
use serde::{Deserialize, Serialize};
use sms_store::{IncomingSms, OutgoingSms, PartKind, PartOutcome, SendStatus, SendUpdate};
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

fn clear_pending(env: &mut JNIEnv) {
    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_clear();
    }
}

/// Whole-body guard returning a JSON string (or null).
fn json_call<'l, T: Serialize>(
    env: &mut JNIEnv<'l>,
    f: impl FnOnce(&mut JNIEnv<'l>) -> CmdResult<T>,
) -> jstring {
    let r = catch_unwind(AssertUnwindSafe(|| -> Option<jstring> {
        let v = f(env).ok()?;
        let s = serde_json::to_string(&v).ok()?;
        env.new_string(s).ok().map(|j| j.into_raw())
    }));
    match r {
        Ok(Some(p)) => p,
        _ => {
            clear_pending(env);
            std::ptr::null_mut()
        }
    }
}

/// Whole-body guard returning a boolean (false on error/panic).
fn bool_call<'l>(
    env: &mut JNIEnv<'l>,
    f: impl FnOnce(&mut JNIEnv<'l>) -> CmdResult<bool>,
) -> jboolean {
    let r = catch_unwind(AssertUnwindSafe(|| f(env)));
    match r {
        Ok(Ok(true)) => JNI_TRUE,
        Ok(Ok(false)) => JNI_FALSE,
        _ => {
            clear_pending(env);
            JNI_FALSE
        }
    }
}

/// Whole-body guard returning a long (-1 on error/panic).
fn long_call<'l>(env: &mut JNIEnv<'l>, f: impl FnOnce(&mut JNIEnv<'l>) -> CmdResult<i64>) -> jlong {
    let r = catch_unwind(AssertUnwindSafe(|| f(env)));
    match r {
        Ok(Ok(v)) => v,
        _ => {
            clear_pending(env);
            -1
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct IngestIn {
    message: IncomingSms,
    #[serde(default)]
    provider_id: Option<i64>,
    /// Provider READ flag of a replayed / imported row: a newly stored
    /// message is marked read at once instead of showing unread until the
    /// next reconcile.
    #[serde(default)]
    read: bool,
}

/// What the receiver needs to post a notification. `body` is redacted (the
/// OTP is bullets); the code is fetched only by the Copy OTP action.
/// `address` is the stored sender: notification actions carry it and the
/// core re-validates it (`actionTarget`) before acting.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct IngestOut {
    inserted: bool,
    notify: bool,
    title: String,
    thread_id: i64,
    msg_id: i64,
    address: String,
    body: String,
    has_otp: bool,
    category: sms_types::Category,
}

fn ingest_one(input: &IngestIn) -> CmdResult<IngestOut> {
    let r = state()?.with_core(|c| {
        let r = c.ingest_incoming(&input.message, input.provider_id)?;
        if r.inserted && input.read {
            c.mark_read(r.notification.message.sms.id)?;
        }
        Ok(r)
    })?;
    Ok(ingest_out(r))
}

fn ingest_out(r: crate::core::IncomingResult) -> IngestOut {
    let n = r.notification;
    IngestOut {
        inserted: r.inserted,
        notify: n.notify,
        title: n.title,
        thread_id: r.thread.thread_id,
        msg_id: n.message.sms.id,
        address: n.message.sms.sender,
        body: n.message.sms.body,
        has_otp: n.message.otp.is_some(),
        category: r.thread.classification.category,
    }
}

/// `init(dbPath, dek)`: open (or join) the shared core; install the
/// Keystore-unwrapped 32-byte vault key when given. The Java array is copied
/// into a zeroizing buffer; Kotlin wipes its own copy. Without a key the
/// vault stays not-ready (vault calls fail; SMS receipt still works).
#[unsafe(no_mangle)]
pub extern "system" fn Java_app_snipt_telephony_NativeCore_init<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    db_path: JString<'l>,
    dek: JByteArray<'l>,
) -> jboolean {
    bool_call(&mut env, |env| {
        let path = jstr(env, &db_path)?;
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
    })
}

/// `ingest({message, providerId}) -> IngestOut JSON | null`.
#[unsafe(no_mangle)]
pub extern "system" fn Java_app_snipt_telephony_NativeCore_ingest<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    json: JString<'l>,
) -> jstring {
    json_call(&mut env, |env| {
        let input: IngestIn = serde_json::from_str(&jstr(env, &json)?)
            .map_err(|_| CmdError::invalid("ingest json"))?;
        let out = ingest_one(&input)?;
        if out.inserted {
            emit(
                "sms-received",
                serde_json::json!({ "threadId": out.thread_id }),
            );
        }
        Ok(out)
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BatchOut {
    inserted: usize,
    skipped: usize,
    failed: usize,
}

/// `ingestBatch([{message, providerId}, ...]) -> {inserted, skipped, failed}`
/// (first-run provider import and reconcile replay; idempotent through
/// provider ids).
#[unsafe(no_mangle)]
pub extern "system" fn Java_app_snipt_telephony_NativeCore_ingestBatch<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    json: JString<'l>,
) -> jstring {
    json_call(&mut env, |env| {
        let items: Vec<IngestIn> = serde_json::from_str(&jstr(env, &json)?)
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
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct QueuedOut {
    id: i64,
    thread_id: i64,
    address: String,
}

/// `queueOutgoing(OutgoingSms) -> {id, threadId, address} | null`
/// (notification inline reply with `threadId` + `address` + `replyTo`,
/// call-reply service with `address`).
#[unsafe(no_mangle)]
pub extern "system" fn Java_app_snipt_telephony_NativeCore_queueOutgoing<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    json: JString<'l>,
) -> jstring {
    json_call(&mut env, |env| {
        let input: OutgoingSms = serde_json::from_str(&jstr(env, &json)?)
            .map_err(|_| CmdError::invalid("outgoing json"))?;
        let q = state()?.with_core(|c| c.queue_outgoing(&input))?;
        emit("sms-changed", serde_json::json!({}));
        Ok(QueuedOut {
            id: q.message.sms.id,
            thread_id: q.thread.thread_id,
            address: q.message.sms.sender.clone(),
        })
    })
}

fn emit_send(u: &SendUpdate) {
    if u.changed {
        emit(
            "send-status",
            serde_json::json!({ "id": u.msg_id, "status": u.status.as_str() }),
        );
    }
}

/// `beginSend(id, parts) -> attemptId | -1`: opens a send attempt for a
/// queued message (the message becomes `sending`).
#[unsafe(no_mangle)]
pub extern "system" fn Java_app_snipt_telephony_NativeCore_beginSend<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    id: jlong,
    parts: jint,
) -> jlong {
    long_call(&mut env, |_| {
        let parts = u32::try_from(parts).map_err(|_| CmdError::invalid("parts"))?;
        let a = state()?.with_core(|c| c.begin_send_attempt(id, parts))?;
        emit(
            "send-status",
            serde_json::json!({ "id": id, "status": SendStatus::Sending.as_str() }),
        );
        Ok(a)
    })
}

/// `bindSend(attemptId, providerId) -> bool`: the provider row of an attempt,
/// bound once (a different id later is refused).
#[unsafe(no_mangle)]
pub extern "system" fn Java_app_snipt_telephony_NativeCore_bindSend<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    attempt_id: jlong,
    provider_id: jlong,
) -> jboolean {
    bool_call(&mut env, |_| {
        if provider_id < 0 {
            return Ok(false);
        }
        state()?.with_core(|c| c.bind_send_provider(attempt_id, provider_id))
    })
}

/// `sendPartResult(attemptId, part, kind, outcome) -> SendUpdate JSON | null`
/// with kind `sent|delivery` and outcome `ok|failed|unknown`.
#[unsafe(no_mangle)]
pub extern "system" fn Java_app_snipt_telephony_NativeCore_sendPartResult<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    attempt_id: jlong,
    part: jint,
    kind: JString<'l>,
    outcome: JString<'l>,
) -> jstring {
    json_call(&mut env, |env| {
        let part = u32::try_from(part).map_err(|_| CmdError::invalid("part"))?;
        let kind = match jstr(env, &kind)?.as_str() {
            "sent" => PartKind::Sent,
            "delivery" => PartKind::Delivery,
            _ => return Err(CmdError::invalid("kind")),
        };
        let outcome = match jstr(env, &outcome)?.as_str() {
            "ok" => PartOutcome::Ok,
            "failed" => PartOutcome::Failed,
            "unknown" => PartOutcome::Unknown,
            _ => return Err(CmdError::invalid("outcome")),
        };
        let u = state()?
            .with_core(|c| c.record_send_part(attempt_id, part, kind, outcome))?
            .ok_or_else(|| CmdError::invalid("attempt"))?;
        emit_send(&u);
        Ok(u)
    })
}

/// `endSend(attemptId, handedOff) -> SendUpdate JSON | null`: closes the
/// attempt's hand-off (the message is deletable again); `handedOff=false`
/// (binding refused, radio rejected) marks it `failed`.
#[unsafe(no_mangle)]
pub extern "system" fn Java_app_snipt_telephony_NativeCore_endSend<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    attempt_id: jlong,
    handed_off: jboolean,
) -> jstring {
    json_call(&mut env, |_| {
        let u = state()?
            .with_core(|c| c.end_send_handoff(attempt_id, handed_off != 0))?
            .ok_or_else(|| CmdError::invalid("attempt"))?;
        emit_send(&u);
        Ok(u)
    })
}

/// `failSend(id) -> SendUpdate JSON | null`: hand-off failed before or
/// without callbacks (monotonic; a delivered message stays delivered).
#[unsafe(no_mangle)]
pub extern "system" fn Java_app_snipt_telephony_NativeCore_failSend<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    id: jlong,
) -> jstring {
    json_call(&mut env, |_| {
        let u = state()?
            .with_core(|c| c.set_send_status(id, SendStatus::Failed))?
            .ok_or_else(|| CmdError::invalid("id"))?;
        emit_send(&u);
        Ok(u)
    })
}

/// `actionTarget(id, address) -> {msgId, threadId, providerId?, isRead} | null`:
/// a notification action's source, only if that message still exists and
/// was received from `address`.
#[unsafe(no_mangle)]
pub extern "system" fn Java_app_snipt_telephony_NativeCore_actionTarget<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    id: jlong,
    address: JString<'l>,
) -> jstring {
    json_call(&mut env, |env| {
        let address = jstr(env, &address)?;
        state()?
            .with_core(|c| c.action_target(id, &address))?
            .ok_or_else(|| CmdError::invalid("stale action"))
    })
}

/// Notification "Mark read" (after the provider row was marked).
#[unsafe(no_mangle)]
pub extern "system" fn Java_app_snipt_telephony_NativeCore_markRead<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    id: jlong,
) -> jboolean {
    bool_call(&mut env, |_| {
        let ok = state()?.with_core(|c| c.mark_read(id))?;
        emit("sms-changed", serde_json::json!({}));
        Ok(ok)
    })
}

/// Notification "Delete" (only after the provider row was deleted).
#[unsafe(no_mangle)]
pub extern "system" fn Java_app_snipt_telephony_NativeCore_deleteMessage<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    id: jlong,
) -> jboolean {
    bool_call(&mut env, |_| {
        let n = state()?.with_core(|c| c.delete_messages(&[id]))?;
        emit("sms-changed", serde_json::json!({}));
        Ok(n > 0)
    })
}

/// Notification "Copy OTP": the plaintext code, or null. Kotlin puts it on
/// the clipboard with EXTRA_IS_SENSITIVE and never logs it.
#[unsafe(no_mangle)]
pub extern "system" fn Java_app_snipt_telephony_NativeCore_otpCode<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    id: jlong,
) -> jstring {
    let r = catch_unwind(AssertUnwindSafe(|| -> Option<jstring> {
        let code = Zeroizing::new(state().ok()?.with_core(|c| c.otp_code(id)).ok()??);
        env.new_string(code.as_str()).ok().map(|j| j.into_raw())
    }));
    match r {
        Ok(Some(p)) => p,
        _ => {
            clear_pending(&mut env);
            std::ptr::null_mut()
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReconcileIn {
    /// `(storeId, providerId)` pairs captured by `reconcileBegin` *before*
    /// the provider was queried.
    candidates: Vec<sms_store::ProviderRef>,
    /// Tombstones from the same snapshot; only these may be pruned. Absent
    /// = prune nothing.
    #[serde(default)]
    tombstones: Vec<i64>,
    present: Vec<i64>,
    read: Vec<i64>,
    /// Provider inbox ids (subset of `present`) for the missed-row replay.
    #[serde(default)]
    inbox: Vec<i64>,
}

/// `reconcileBegin() -> {candidates: [{id, providerId}], tombstones: [id]} |
/// null`: the local snapshot, captured before the provider query.
#[unsafe(no_mangle)]
pub extern "system" fn Java_app_snipt_telephony_NativeCore_reconcileBegin<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
) -> jstring {
    json_call(&mut env, |_| state()?.with_core(|c| c.provider_snapshot()))
}

/// `reconcile({candidates, present, read, inbox}) -> {deleted, markedRead,
/// missing: [providerId]} | null`. `missing` = provider inbox rows the
/// store never ingested (and did not delete): Kotlin replays them.
#[unsafe(no_mangle)]
pub extern "system" fn Java_app_snipt_telephony_NativeCore_reconcile<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    json: JString<'l>,
) -> jstring {
    json_call(&mut env, |env| {
        let input: ReconcileIn = serde_json::from_str(&jstr(env, &json)?)
            .map_err(|_| CmdError::invalid("reconcile json"))?;
        let rep = state()?.with_core(|c| {
            c.provider_reconcile(
                &input.candidates,
                &input.tombstones,
                &input.present,
                &input.read,
                &input.inbox,
            )
        })?;
        if rep.deleted + rep.marked_read > 0 {
            emit("sms-changed", serde_json::json!({}));
        }
        Ok(rep)
    })
}

/// `providerMissing([providerId]) -> [providerId] | null`: of the given
/// provider inbox ids, those the store neither holds nor deleted locally
/// (explicit "import missing messages"; ignores the v1 replay floor).
#[unsafe(no_mangle)]
pub extern "system" fn Java_app_snipt_telephony_NativeCore_providerMissing<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    json: JString<'l>,
) -> jstring {
    json_call(&mut env, |env| {
        let ids: Vec<i64> =
            serde_json::from_str(&jstr(env, &json)?).map_err(|_| CmdError::invalid("ids json"))?;
        state()?.with_core(|c| c.provider_missing(&ids))
    })
}

/// Native UI events with no payload (`back`, `compose`, `sms-changed`).
/// Only known names pass.
#[unsafe(no_mangle)]
pub extern "system" fn Java_app_snipt_telephony_NativeCore_emit<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    event: JString<'l>,
) {
    let _ = bool_call(&mut env, |env| {
        let e = jstr(env, &event)?;
        if matches!(e.as_str(), "back" | "compose" | "sms-changed") {
            emit(&e, serde_json::json!({}));
        }
        Ok(true)
    });
}

// ---- MMS receive (S15) ------------------------------------------------

/// Largest PDU accepted over JNI (same cap as the parser / MmsService).
fn pdu_bytes(env: &mut JNIEnv, a: &JByteArray) -> CmdResult<Vec<u8>> {
    if a.is_null() {
        return Err(CmdError::invalid("null pdu"));
    }
    let n = env
        .get_array_length(a)
        .map_err(|_| CmdError::invalid("pdu"))?;
    if n < 0 || n as usize > sms_mms::MAX_PDU_BYTES {
        return Err(CmdError::invalid("pdu size"));
    }
    env.convert_byte_array(a)
        .map_err(|_| CmdError::invalid("pdu"))
}

fn opt_id(v: jlong) -> Option<i64> {
    (v >= 0).then_some(v)
}

/// `mmsNotification(pdu, subId, slot) -> {action, pendingId, location, subId} | null`
/// for a WAP_PUSH_DELIVER `data` extra (M-Notification.ind).
#[unsafe(no_mangle)]
pub extern "system" fn Java_app_snipt_telephony_NativeCore_mmsNotification<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    pdu: JByteArray<'l>,
    sub_id: jint,
    slot: jint,
) -> jstring {
    json_call(&mut env, |env| {
        let bytes = pdu_bytes(env, &pdu)?;
        let slot = u8::try_from(slot.clamp(0, 1)).unwrap_or(0);
        state()?.with_core(|c| c.mms_notification(&bytes, opt_id(i64::from(sub_id)), slot))
    })
}

/// `mmsAttempt(pendingId) -> attempts | -1` (not pending any more).
#[unsafe(no_mangle)]
pub extern "system" fn Java_app_snipt_telephony_NativeCore_mmsAttempt<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    pending_id: jlong,
) -> jlong {
    long_call(&mut env, |_| {
        state()?.with_core(|c| c.mms_attempt(pending_id))
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MaintenanceOut {
    retry: Vec<crate::core::MmsNoticeOut>,
    placeholders: Vec<IngestOut>,
}

/// `mmsMaintenance() -> {retry: [notice], placeholders: [IngestOut]}`:
/// downloads to retry now, and placeholder messages stored for downloads
/// that expired or ran out of attempts (app start / resume / next push).
#[unsafe(no_mangle)]
pub extern "system" fn Java_app_snipt_telephony_NativeCore_mmsMaintenance<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
) -> jstring {
    json_call(&mut env, |_| {
        let (retry, ph) =
            state()?.with_core(|c| Ok((c.mms_retryable()?, c.mms_give_up_exhausted()?)))?;
        let placeholders: Vec<IngestOut> = ph.into_iter().map(ingest_out).collect();
        if placeholders.iter().any(|p| p.inserted) {
            emit("sms-changed", serde_json::json!({}));
        }
        Ok(MaintenanceOut {
            retry,
            placeholders,
        })
    })
}

/// `mmsRetrieved(pendingId, pdu) -> MmsRetrieved JSON | null`: headers and
/// parts (PDU ranges) for the Telephony provider write.
#[unsafe(no_mangle)]
pub extern "system" fn Java_app_snipt_telephony_NativeCore_mmsRetrieved<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    pending_id: jlong,
    pdu: JByteArray<'l>,
) -> jstring {
    json_call(&mut env, |env| {
        let bytes = pdu_bytes(env, &pdu)?;
        state()?.with_core(|c| c.mms_retrieved(pending_id, &bytes))
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MmsIngestJni {
    ingest: IngestOut,
    thumbs: Vec<crate::core::mms_types::ThumbJob>,
}

/// `mmsIngest(pendingId, pdu, mmsId) -> {ingest: IngestOut, thumbs} | null`
/// (`mmsId` -1 when the provider write failed).
#[unsafe(no_mangle)]
pub extern "system" fn Java_app_snipt_telephony_NativeCore_mmsIngest<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    pending_id: jlong,
    pdu: JByteArray<'l>,
    mms_id: jlong,
) -> jstring {
    json_call(&mut env, |env| {
        let bytes = pdu_bytes(env, &pdu)?;
        let out = state()?.with_core(|c| c.mms_ingest(pending_id, &bytes, opt_id(mms_id)))?;
        let ingest = ingest_out(out.result);
        if ingest.inserted {
            emit(
                "sms-received",
                serde_json::json!({ "threadId": ingest.thread_id }),
            );
        }
        Ok(MmsIngestJni {
            ingest,
            thumbs: out.thumbs,
        })
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MmsFailedOut {
    outcome: sms_store::FailOutcome,
    placeholder: Option<IngestOut>,
}

/// `mmsFailed(pendingId) -> {outcome: retry|gaveUp|ignored, placeholder?}`.
#[unsafe(no_mangle)]
pub extern "system" fn Java_app_snipt_telephony_NativeCore_mmsFailed<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    pending_id: jlong,
) -> jstring {
    json_call(&mut env, |_| {
        let (outcome, ph) = state()?.with_core(|c| c.mms_failed(pending_id))?;
        let placeholder = ph.map(ingest_out);
        if let Some(p) = &placeholder
            && p.inserted
        {
            emit(
                "sms-received",
                serde_json::json!({ "threadId": p.thread_id }),
            );
        }
        Ok(MmsFailedOut {
            outcome,
            placeholder,
        })
    })
}

/// `mmsThumb(partId, jpeg) -> bool`: platform-made thumbnail (JPEG, capped).
#[unsafe(no_mangle)]
pub extern "system" fn Java_app_snipt_telephony_NativeCore_mmsThumb<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    part_id: jlong,
    jpeg: JByteArray<'l>,
) -> jboolean {
    bool_call(&mut env, |env| {
        let bytes = pdu_bytes(env, &jpeg)?;
        let ok = state()?.with_core(|c| c.mms_set_thumb(part_id, &bytes))?;
        if ok {
            emit("sms-changed", serde_json::json!({}));
        }
        Ok(ok)
    })
}

/// `mmsNotifyResp(pendingId) -> byte[] | null`: M-NotifyResp.ind (Retrieved).
#[unsafe(no_mangle)]
pub extern "system" fn Java_app_snipt_telephony_NativeCore_mmsNotifyResp<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    pending_id: jlong,
) -> jbyteArray {
    let r = catch_unwind(AssertUnwindSafe(|| -> Option<jbyteArray> {
        let b = state()
            .ok()?
            .with_core(|c| c.mms_notify_resp(pending_id))
            .ok()?;
        env.byte_array_from_slice(&b).ok().map(|a| a.into_raw())
    }));
    match r {
        Ok(Some(p)) => p,
        _ => {
            clear_pending(&mut env);
            std::ptr::null_mut()
        }
    }
}
