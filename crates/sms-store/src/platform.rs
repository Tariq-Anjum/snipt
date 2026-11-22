//! Platform-facing store paths: inbox counters, the outgoing send queue and
//! provider (Android `content://sms`) synchronisation. None of this exists in
//! the TS oracle (store.ts has no send state or provider ids); outgoing rows
//! follow store.ts `appendOutgoingReply` for the parts it defines.

use std::collections::BTreeSet;

use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use sms_types::{MessageProtocol, RawSms, js, time::is_valid_timestamp_ms};

use crate::ids::{IdKind, allocate};
use crate::{Result, Store, StoreError, ingest::insert_message, threads::recompute_thread};

fn outgoing_status(tx: &rusqlite::Transaction<'_>, sms_id: i64) -> Result<Option<SendStatus>> {
    let row: Option<Option<String>> = tx
        .prepare_cached("SELECT send_status FROM messages WHERE id = ?1 AND is_outgoing = 1")?
        .query_row([sms_id], |r| r.get(0))
        .optional()?;
    Ok(row.map(|s| {
        s.as_deref()
            .and_then(SendStatus::parse)
            .unwrap_or(SendStatus::Queued)
    }))
}

/// Monotonic transition inside the caller's write transaction.
fn advance_status(
    tx: &rusqlite::Transaction<'_>,
    sms_id: i64,
    to: SendStatus,
) -> Result<Option<SendUpdate>> {
    let Some(cur) = outgoing_status(tx, sms_id)? else {
        return Ok(None);
    };
    let changed = cur.can_advance(to);
    if changed {
        tx.prepare_cached("UPDATE messages SET send_status = ?2 WHERE id = ?1")?
            .execute(params![sms_id, to.as_str()])?;
    }
    let provider_id: Option<i64> = tx
        .prepare_cached("SELECT provider_id FROM messages WHERE id = ?1")?
        .query_row([sms_id], |r| r.get(0))?;
    Ok(Some(SendUpdate {
        msg_id: sms_id,
        status: if changed { to } else { cur },
        changed,
        provider_id,
    }))
}

/// Prototype `MIN_HISTORY_CAPACITY`: the history size the app is built for.
pub const HISTORY_CAPACITY: i64 = 10_000;

/// Thread counts per inbox filter, with the same predicates as
/// `threads_page` / store.ts `filterThreads` (empty query).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct FilterCounts {
    #[serde(rename = "All")]
    pub all: i64,
    #[serde(rename = "Personal")]
    pub personal: i64,
    #[serde(rename = "Transactions")]
    pub transactions: i64,
    #[serde(rename = "Promotions")]
    pub promotions: i64,
    #[serde(rename = "Priority")]
    pub priority: i64,
    #[serde(rename = "Spam")]
    pub spam: i64,
    #[serde(rename = "RCS")]
    pub rcs: i64,
    #[serde(rename = "Starred")]
    pub starred: i64,
    #[serde(rename = "Archived")]
    pub archived: i64,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct InboxCounts {
    /// Every stored message (all threads, archived and spam included).
    pub total_messages: i64,
    /// Unread incoming messages in non-archived, non-Spam threads (the
    /// prototype's inbox badge).
    pub unread: i64,
    pub filters: FilterCounts,
    pub capacity: i64,
}

/// Send state of an outgoing message (`messages.send_status`).
///
/// Transitions are monotonic and enforced in the store
/// ([`SendStatus::can_advance`]): `queued < sending < sent < unconfirmed <
/// delivered`; `failed` and `delivered` are terminal. `unconfirmed` = every
/// part was sent and a delivery report arrived for each, but at least one
/// report could not be interpreted.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SendStatus {
    Queued,
    Sending,
    Sent,
    Unconfirmed,
    Delivered,
    Failed,
}

impl SendStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            SendStatus::Queued => "queued",
            SendStatus::Sending => "sending",
            SendStatus::Sent => "sent",
            SendStatus::Unconfirmed => "unconfirmed",
            SendStatus::Delivered => "delivered",
            SendStatus::Failed => "failed",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "queued" => SendStatus::Queued,
            "sending" => SendStatus::Sending,
            "sent" => SendStatus::Sent,
            "unconfirmed" => SendStatus::Unconfirmed,
            "delivered" => SendStatus::Delivered,
            "failed" => SendStatus::Failed,
            _ => return None,
        })
    }

    fn rank(self) -> u8 {
        match self {
            SendStatus::Queued => 0,
            SendStatus::Sending => 1,
            SendStatus::Sent => 2,
            SendStatus::Unconfirmed => 3,
            SendStatus::Delivered => 4,
            SendStatus::Failed => 5,
        }
    }

    /// Whether a message in state `self` may move to `to`. Same state is a
    /// no-op (false). `failed` and `delivered` are terminal; `failed` is
    /// reachable from every other state; otherwise only forward moves.
    pub fn can_advance(self, to: SendStatus) -> bool {
        match (self, to) {
            (a, b) if a == b => false,
            (SendStatus::Failed | SendStatus::Delivered, _) => false,
            (_, SendStatus::Failed) => true,
            (a, b) => b.rank() > a.rank(),
        }
    }
}

/// Which radio callback a per-part result came from.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum PartKind {
    Sent,
    Delivery,
}

/// Outcome of one part's callback. `Unknown` = a delivery report arrived but
/// its status could not be interpreted (shown as unconfirmed, never as
/// delivered).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum PartOutcome {
    Ok,
    Failed,
    Unknown,
}

impl PartKind {
    fn as_str(self) -> &'static str {
        match self {
            PartKind::Sent => "sent",
            PartKind::Delivery => "delivery",
        }
    }
}

impl PartOutcome {
    fn as_str(self) -> &'static str {
        match self {
            PartOutcome::Ok => "ok",
            PartOutcome::Failed => "failed",
            PartOutcome::Unknown => "unknown",
        }
    }
}

/// Persisted state after a send-state write.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SendUpdate {
    pub msg_id: i64,
    /// The message's stored status after the write.
    pub status: SendStatus,
    /// True when this write moved the status.
    pub changed: bool,
    pub provider_id: Option<i64>,
}

/// A message to queue for sending: either a reply in an existing thread or a
/// new message to an address.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OutgoingSms {
    #[serde(default)]
    pub thread_id: Option<i64>,
    /// New message: the recipient. With `thread_id`: the recipient the caller
    /// expects that thread to have; a mismatch is rejected (a stale
    /// notification must never reply into another conversation).
    #[serde(default)]
    pub address: Option<String>,
    pub body: String,
    pub sim_slot: u8,
    /// Reply to this message: it must still exist in `thread_id`.
    #[serde(default)]
    pub reply_to: Option<i64>,
}

/// Store id ↔ provider id pair.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderRef {
    pub id: i64,
    pub provider_id: i64,
}

impl Store {
    pub fn inbox_counts(&self, sim: Option<u8>) -> Result<InboxCounts> {
        let (pred, args) = match sim {
            Some(s) => ("WHERE sim_slot = ?1", vec![i64::from(s)]),
            None => ("", vec![]),
        };
        let sql = format!(
            "SELECT
                COALESCE(SUM(is_archived = 0 AND category <> 'Spam'), 0),
                COALESCE(SUM(is_archived = 0 AND category = 'Personal'), 0),
                COALESCE(SUM(is_archived = 0 AND category = 'Transactions'), 0),
                COALESCE(SUM(is_archived = 0 AND category = 'Promotions'), 0),
                COALESCE(SUM(is_archived = 0 AND category = 'Priority'), 0),
                COALESCE(SUM(is_archived = 0 AND category = 'Spam'), 0),
                COALESCE(SUM(is_archived = 0 AND is_rcs = 1), 0),
                COALESCE(SUM(is_archived = 0 AND has_starred = 1), 0),
                COALESCE(SUM(is_archived = 1), 0),
                COALESCE(SUM(CASE WHEN is_archived = 0 AND category <> 'Spam'
                                  THEN unread ELSE 0 END), 0)
             FROM threads {pred}"
        );
        let (filters, unread) =
            self.conn
                .prepare_cached(&sql)?
                .query_row(rusqlite::params_from_iter(args), |r| {
                    Ok((
                        FilterCounts {
                            all: r.get(0)?,
                            personal: r.get(1)?,
                            transactions: r.get(2)?,
                            promotions: r.get(3)?,
                            priority: r.get(4)?,
                            spam: r.get(5)?,
                            rcs: r.get(6)?,
                            starred: r.get(7)?,
                            archived: r.get(8)?,
                        },
                        r.get(9)?,
                    ))
                })?;
        Ok(InboxCounts {
            total_messages: self.message_count()?,
            unread,
            filters,
            capacity: HISTORY_CAPACITY,
        })
    }

    /// Store an outgoing SMS with `send_status = 'queued'`. A reply copies
    /// sender / contact / known-contact from the thread's first message
    /// (store.ts `appendOutgoingReply`); a new address joins the thread of an
    /// earlier message from the same sender (case-insensitive) or starts one.
    /// The radio send is the platform's job; it reports back via
    /// [`Store::set_send_status`].
    pub fn queue_outgoing(&mut self, out: &OutgoingSms, now_ms: i64) -> Result<RawSms> {
        if out.sim_slot > 1 {
            return Err(StoreError::Invalid("simSlot must be 0 or 1"));
        }
        if !is_valid_timestamp_ms(now_ms) {
            return Err(StoreError::Invalid("timestampMs out of range"));
        }
        let body = js::trim(&out.body).to_string();
        if body.is_empty() {
            return Err(StoreError::Invalid("empty body"));
        }
        let tx = crate::write_tx(&mut self.conn)?;
        type Origin = (i64, String, Option<String>, bool);
        let origin: Option<Origin> = match (&out.thread_id, &out.address) {
            (Some(t), expected) => {
                let o: Origin = tx
                    .prepare_cached(
                        "SELECT thread_id, sender, contact_name, is_known_contact FROM messages
                         WHERE thread_id = ?1 ORDER BY ins_seq LIMIT 1",
                    )?
                    .query_row([t], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
                    .optional()?
                    .ok_or(StoreError::Invalid("unknown thread"))?;
                if let Some(addr) = expected
                    && js::trim(addr).to_lowercase() != o.1.to_lowercase()
                {
                    return Err(StoreError::Invalid("recipient mismatch"));
                }
                if let Some(src) = out.reply_to {
                    let in_thread: Option<i64> = tx
                        .prepare_cached("SELECT thread_id FROM messages WHERE id = ?1")?
                        .query_row([src], |r| r.get(0))
                        .optional()?;
                    if in_thread != Some(*t) {
                        return Err(StoreError::Invalid("reply source no longer exists"));
                    }
                }
                Some(o)
            }
            (None, Some(addr)) => {
                if out.reply_to.is_some() {
                    return Err(StoreError::Invalid("replyTo needs threadId"));
                }
                let addr = js::trim(addr).to_string();
                if addr.is_empty() {
                    return Err(StoreError::Invalid("empty address"));
                }
                let o = tx
                    .prepare_cached(
                        "SELECT thread_id, sender, contact_name, is_known_contact FROM messages
                         WHERE sender_lc = ?1 ORDER BY ins_seq LIMIT 1",
                    )?
                    .query_row([addr.to_lowercase()], |r| {
                        Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
                    })
                    .optional()?;
                match o {
                    Some(o) => Some(o),
                    None => Some((allocate(&tx, IdKind::Thread)?, addr, None, false)),
                }
            }
            (None, None) => None,
        };
        let (thread_id, sender, contact_name, known) =
            origin.ok_or(StoreError::Invalid("threadId or address required"))?;
        let next_id = allocate(&tx, IdKind::Message)?;
        let sms = RawSms {
            id: next_id,
            thread_id,
            sender,
            contact_name,
            body,
            timestamp_ms: now_ms,
            is_known_contact: known,
            is_outgoing: true,
            sim_slot: out.sim_slot,
            is_read: Some(true),
            is_starred: Some(false),
            protocol: Some(MessageProtocol::Sms),
            rcs_status: None,
            rcs_rich_card: None,
        };
        let a = self.engine.analyze(&sms);
        insert_message(&tx, &sms, &a)?;
        tx.execute(
            "UPDATE messages SET send_status = ?2 WHERE id = ?1",
            params![sms.id, SendStatus::Queued.as_str()],
        )?;
        recompute_thread(&tx, thread_id)?;
        tx.commit()?;
        Ok(sms)
    }

    /// Move an outgoing message's send state (monotonic, see
    /// [`SendStatus::can_advance`]); used for failures before a send attempt
    /// exists. Returns the persisted state, or `None` when `sms_id` is not an
    /// outgoing message.
    pub fn set_send_status(
        &mut self,
        sms_id: i64,
        status: SendStatus,
    ) -> Result<Option<SendUpdate>> {
        let tx = crate::write_tx(&mut self.conn)?;
        let r = advance_status(&tx, sms_id, status)?;
        tx.commit()?;
        Ok(r)
    }

    /// Start the radio hand-off of a queued outgoing message: a new attempt
    /// id (AUTOINCREMENT, never reused) that every callback carries. Only a
    /// `queued` message can start an attempt; the message becomes `sending`.
    pub fn begin_send_attempt(&mut self, sms_id: i64, parts: u32, now_ms: i64) -> Result<i64> {
        if parts == 0 || parts > 255 {
            return Err(StoreError::Invalid("parts out of range"));
        }
        let tx = crate::write_tx(&mut self.conn)?;
        let st =
            outgoing_status(&tx, sms_id)?.ok_or(StoreError::Invalid("not an outgoing message"))?;
        if st != SendStatus::Queued {
            return Err(StoreError::Invalid("message is not queued"));
        }
        tx.prepare_cached(
            "INSERT INTO send_attempts(msg_id, parts, created_ms) VALUES (?1, ?2, ?3)",
        )?
        .execute(params![sms_id, parts, now_ms])?;
        let attempt = tx.last_insert_rowid();
        advance_status(&tx, sms_id, SendStatus::Sending)?;
        tx.commit()?;
        Ok(attempt)
    }

    /// Bind the provider row the platform wrote for an attempt. Binding is
    /// set once: the same id again is a no-op (true); a different id, or a
    /// provider id already bound to another message, is refused (false).
    pub fn bind_send_provider(&mut self, attempt_id: i64, provider_id: i64) -> Result<bool> {
        let tx = crate::write_tx(&mut self.conn)?;
        let row: Option<(i64, Option<i64>)> = tx
            .prepare_cached("SELECT msg_id, provider_id FROM send_attempts WHERE attempt_id = ?1")?
            .query_row([attempt_id], |r| Ok((r.get(0)?, r.get(1)?)))
            .optional()?;
        let Some((msg_id, bound)) = row else {
            return Ok(false);
        };
        if let Some(p) = bound {
            return Ok(p == provider_id);
        }
        let owner: Option<i64> = tx
            .prepare_cached("SELECT id FROM messages WHERE provider_id = ?1")?
            .query_row([provider_id], |r| r.get(0))
            .optional()?;
        let msg_pid: Option<Option<i64>> = tx
            .prepare_cached("SELECT provider_id FROM messages WHERE id = ?1")?
            .query_row([msg_id], |r| r.get(0))
            .optional()?;
        let ok = match (owner, msg_pid.flatten()) {
            (Some(o), _) if o != msg_id => false,
            (_, Some(p)) if p != provider_id => false,
            _ => true,
        };
        if ok {
            tx.execute(
                "UPDATE send_attempts SET provider_id = ?2 WHERE attempt_id = ?1",
                params![attempt_id, provider_id],
            )?;
            tx.execute(
                "UPDATE messages SET provider_id = ?2, provider_kind = 'sms' WHERE id = ?1",
                params![msg_id, provider_id],
            )?;
        }
        tx.commit()?;
        Ok(ok)
    }

    /// Persist one part's sent/delivery callback (first result per part and
    /// kind wins; duplicates are ignored), derive the attempt's state from
    /// all persisted results, and advance the message monotonically. `None`
    /// for an unknown attempt or a part index outside the attempt.
    pub fn record_send_part(
        &mut self,
        attempt_id: i64,
        part: u32,
        kind: PartKind,
        outcome: PartOutcome,
    ) -> Result<Option<SendUpdate>> {
        let tx = crate::write_tx(&mut self.conn)?;
        let row: Option<(i64, u32)> = tx
            .prepare_cached("SELECT msg_id, parts FROM send_attempts WHERE attempt_id = ?1")?
            .query_row([attempt_id], |r| Ok((r.get(0)?, r.get(1)?)))
            .optional()?;
        let Some((msg_id, parts)) = row else {
            return Ok(None);
        };
        if part >= parts {
            return Ok(None);
        }
        tx.prepare_cached(
            "INSERT OR IGNORE INTO send_part_results(attempt_id, part, kind, outcome)
             VALUES (?1, ?2, ?3, ?4)",
        )?
        .execute(params![attempt_id, part, kind.as_str(), outcome.as_str()])?;
        let mut sent_ok = 0u32;
        let mut sent_failed = false;
        let mut del_ok = 0u32;
        let mut del_unknown = 0u32;
        let mut del_failed = false;
        {
            let mut st = tx.prepare_cached(
                "SELECT kind, outcome FROM send_part_results WHERE attempt_id = ?1",
            )?;
            let rows = st.query_map([attempt_id], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })?;
            for row in rows {
                let (k, o) = row?;
                match (k.as_str(), o.as_str()) {
                    ("sent", "ok") => sent_ok += 1,
                    ("sent", _) => sent_failed = true,
                    ("delivery", "ok") => del_ok += 1,
                    ("delivery", "failed") => del_failed = true,
                    ("delivery", _) => del_unknown += 1,
                    _ => {}
                }
            }
        }
        let target = if sent_failed || del_failed {
            SendStatus::Failed
        } else if sent_ok < parts {
            SendStatus::Sending
        } else if del_ok == parts {
            SendStatus::Delivered
        } else if del_ok + del_unknown == parts {
            SendStatus::Unconfirmed
        } else {
            SendStatus::Sent
        };
        let r = advance_status(&tx, msg_id, target)?;
        tx.commit()?;
        Ok(r)
    }

    /// Send states of the given messages (only rows that have one).
    pub fn send_statuses(&self, ids: &[i64]) -> Result<Vec<(i64, SendStatus)>> {
        let mut stmt = self
            .conn
            .prepare_cached("SELECT send_status FROM messages WHERE id = ?1")?;
        let mut out = Vec::new();
        for id in ids {
            let s: Option<Option<String>> = stmt.query_row([id], |r| r.get(0)).optional()?;
            if let Some(st) = s.flatten().as_deref().and_then(SendStatus::parse) {
                out.push((*id, st));
            }
        }
        Ok(out)
    }

    /// Provider ids recorded for the given store ids (ids without one are skipped).
    pub fn provider_refs(&self, ids: &[i64]) -> Result<Vec<ProviderRef>> {
        let mut stmt = self
            .conn
            .prepare_cached("SELECT provider_id FROM messages WHERE id = ?1")?;
        let mut out = Vec::new();
        for id in ids {
            let p: Option<Option<i64>> = stmt.query_row([id], |r| r.get(0)).optional()?;
            if let Some(pid) = p.flatten() {
                out.push(ProviderRef {
                    id: *id,
                    provider_id: pid,
                });
            }
        }
        Ok(out)
    }

    fn ids_for_providers(&self, provider_ids: &[i64]) -> Result<Vec<i64>> {
        let mut stmt = self
            .conn
            .prepare_cached("SELECT id FROM messages WHERE provider_id = ?1")?;
        let mut out = Vec::new();
        for p in provider_ids {
            if let Some(id) = stmt.query_row([p], |r| r.get(0)).optional()? {
                out.push(id);
            }
        }
        Ok(out)
    }

    /// The platform reports messages read elsewhere; returns rows changed.
    pub fn provider_mark_read(&mut self, provider_ids: &[i64]) -> Result<usize> {
        let tx = crate::write_tx(&mut self.conn)?;
        let mut touched = BTreeSet::new();
        let mut changed = 0;
        for p in provider_ids {
            let row: Option<(i64, i64)> = tx
                .prepare_cached(
                    "SELECT id, thread_id FROM messages
                     WHERE provider_id = ?1 AND COALESCE(is_read, 0) = 0",
                )?
                .query_row([p], |r| Ok((r.get(0)?, r.get(1)?)))
                .optional()?;
            if let Some((id, t)) = row {
                changed += tx
                    .prepare_cached("UPDATE messages SET is_read = 1 WHERE id = ?1")?
                    .execute([id])?;
                touched.insert(t);
            }
        }
        for t in touched {
            recompute_thread(&tx, t)?;
        }
        tx.commit()?;
        Ok(changed)
    }

    /// The platform reports messages deleted elsewhere; returns rows deleted.
    pub fn provider_delete(&mut self, provider_ids: &[i64]) -> Result<usize> {
        let ids = self.ids_for_providers(provider_ids)?;
        self.delete_messages(&ids)
    }

    /// Local candidate set for [`Store::provider_reconcile`]: every stored
    /// `(id, provider_id)`. Must be captured *before* the provider query.
    pub fn provider_candidates(&self) -> Result<Vec<ProviderRef>> {
        let mut st = self.conn.prepare_cached(
            "SELECT id, provider_id FROM messages WHERE provider_id IS NOT NULL ORDER BY id",
        )?;
        let v = st
            .query_map([], |r| {
                Ok(ProviderRef {
                    id: r.get(0)?,
                    provider_id: r.get(1)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(v)
    }

    /// Provider -> store sync without the snapshot race (R2/R3): only
    /// `candidates` (captured before the provider was queried) can be
    /// deleted, and only while the stored row still carries the same
    /// provider id. A message ingested after the candidate snapshot is never
    /// a candidate, so it survives even though the older provider snapshot
    /// lacks it. `read` marks rows read. `missing` lists provider inbox ids
    /// (`inbox`, subset of `present`) the store never ingested and did not
    /// delete locally (tombstones), above the v1 replay floor: the platform
    /// replays them (a receipt whose ingest was lost). An empty `present`
    /// with candidates is refused (a failed or permission-less provider query
    /// must not wipe the inbox). One transaction.
    pub fn provider_reconcile(
        &mut self,
        candidates: &[ProviderRef],
        present: &[i64],
        read: &[i64],
        inbox: &[i64],
    ) -> Result<Reconciled> {
        if present.is_empty() && !candidates.is_empty() {
            return Err(StoreError::Invalid("empty provider snapshot"));
        }
        let present: std::collections::HashSet<i64> = present.iter().copied().collect();
        let tx = crate::write_tx(&mut self.conn)?;
        let mut touched = BTreeSet::new();
        let mut deleted = 0;
        for c in candidates
            .iter()
            .filter(|c| !present.contains(&c.provider_id))
        {
            let t: Option<i64> = tx
                .prepare_cached(
                    "SELECT thread_id FROM messages WHERE id = ?1 AND provider_id = ?2",
                )?
                .query_row(params![c.id, c.provider_id], |r| r.get(0))
                .optional()?;
            if let Some(t) = t {
                deleted += tx
                    .prepare_cached("DELETE FROM messages WHERE id = ?1")?
                    .execute([c.id])?;
                tx.prepare_cached("DELETE FROM reminders_done WHERE msg_id = ?1")?
                    .execute([c.id])?;
                touched.insert(t);
            }
        }
        let mut marked_read = 0;
        for p in read {
            let row: Option<(i64, i64)> = tx
                .prepare_cached(
                    "SELECT id, thread_id FROM messages
                     WHERE provider_id = ?1 AND COALESCE(is_read, 0) = 0",
                )?
                .query_row([p], |r| Ok((r.get(0)?, r.get(1)?)))
                .optional()?;
            if let Some((id, t)) = row {
                marked_read += tx
                    .prepare_cached("UPDATE messages SET is_read = 1 WHERE id = ?1")?
                    .execute([id])?;
                touched.insert(t);
            }
        }
        for t in touched {
            recompute_thread(&tx, t)?;
        }
        // Tombstones of provider rows that are gone everywhere are done.
        let tombs: Vec<i64> = tx
            .prepare_cached("SELECT provider_id FROM provider_tombstones")?
            .query_map([], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        for t in tombs.into_iter().filter(|t| !present.contains(t)) {
            tx.prepare_cached("DELETE FROM provider_tombstones WHERE provider_id = ?1")?
                .execute([t])?;
        }
        let floor: i64 = tx
            .prepare_cached("SELECT value FROM meta WHERE key = 'provider_replay_floor'")?
            .query_row([], |r| r.get::<_, String>(0))
            .optional()?
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        let mut missing = Vec::new();
        for p in inbox.iter().filter(|p| **p > floor && present.contains(p)) {
            let known: Option<i64> = tx
                .prepare_cached(
                    "SELECT 1 FROM messages WHERE provider_id = ?1
                     UNION ALL SELECT 1 FROM provider_tombstones WHERE provider_id = ?1
                     LIMIT 1",
                )?
                .query_row([p], |r| r.get(0))
                .optional()?;
            if known.is_none() {
                missing.push(*p);
            }
        }
        tx.commit()?;
        Ok(Reconciled {
            deleted,
            marked_read,
            missing,
        })
    }
}

/// Result of [`Store::provider_reconcile`].
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Reconciled {
    pub deleted: usize,
    pub marked_read: usize,
    /// Provider inbox ids to replay (ingest with their provider id).
    pub missing: Vec<i64>,
}
