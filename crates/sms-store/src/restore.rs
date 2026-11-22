//! Backup restore as one atomic merge (R2 #2, R4 #1-#3).
//!
//! Rules:
//! - Everything is validated before the first write: message / thread /
//!   archived-thread / completed-reminder ids must be in `1..=MAX_ID`,
//!   message ids unique within the archive, SIM slots 0/1, timestamps in the
//!   JS `Date` range. A bad archive changes nothing.
//! - Presence is a multiset match against rows that existed *before* the
//!   restore: an incoming message matches one not-yet-matched stored row
//!   with the same identity (sender case-insensitive, timestamp, body,
//!   direction, SIM slot, transport). Rows inserted by this restore are
//!   never match candidates, so N identical archive rows against M stored
//!   ones insert `max(0, N - M)`; rows differing only by SIM or transport
//!   are distinct.
//! - Thread mapping is derived from the matched rows: an incoming thread
//!   whose matched messages live in exactly one local thread maps there; in
//!   several local threads the archive is ambiguous and refused (nothing is
//!   written). An incoming thread without matches keeps its id when the
//!   local thread with that id has the same sender, otherwise joins the
//!   sender's existing local thread, otherwise keeps its id when it is
//!   free and keepable, else gets a fresh id.
//! - An archive id is stored verbatim only when it is free locally and
//!   keepable ([`crate::ids::keepable`]: at most `MAX_EXTERNAL_ID`, and not
//!   below the allocator's high-water mark, i.e. never used or retired
//!   locally). Everything else is remapped through the allocator, so an
//!   archive can neither exhaust the id space nor bring back a retired id a
//!   stale notification action or send callback still names.
//! - Archived-thread and completed-reminder references follow the maps;
//!   references to ids not in the archive are ignored.
//! - All writes (messages, thread recompute, archive flags, blocked senders,
//!   reminders, settings) commit in one `BEGIN IMMEDIATE` transaction.
//!
//! Restoring into an empty store keeps every id unchanged, so TS-written
//! `.mysms.enc` archives restore exactly as before (parity fixtures).

use std::collections::{BTreeSet, HashMap, HashSet};

use rusqlite::{OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use sms_types::{RawSms, time::is_valid_timestamp_ms};

use crate::ids::{IdKind, allocate, keepable, stored_next, valid_id};
use crate::ingest::{IngestReport, insert_message};
use crate::threads::recompute_thread;
use crate::{Result, Store, StoreError};

/// Decoded archive parts to merge. `settings` are `(key, json)` rows the
/// caller already validated.
pub struct RestoreInput<'a> {
    pub messages: &'a [RawSms],
    pub archived_thread_ids: &'a [i64],
    pub blocked_senders: &'a [String],
    pub completed_reminder_ids: &'a [i64],
    pub settings: &'a [(&'a str, String)],
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct RestoreOutcome {
    pub ingest: IngestReport,
    /// Incoming messages stored under a new id (id taken, retired locally
    /// or outside the external id range).
    pub remapped_messages: usize,
    /// Incoming threads stored under another thread id.
    pub remapped_threads: usize,
    pub archived_threads: usize,
    pub blocked_senders: usize,
}

fn validate(input: &RestoreInput<'_>) -> Result<()> {
    let mut seen = HashSet::with_capacity(input.messages.len());
    for m in input.messages {
        if !valid_id(m.id) || !valid_id(m.thread_id) {
            return Err(StoreError::Invalid("archive id out of range"));
        }
        if !seen.insert(m.id) {
            return Err(StoreError::Invalid("duplicate message id in archive"));
        }
        if m.sim_slot > 1 {
            return Err(StoreError::Invalid("simSlot must be 0 or 1"));
        }
        if !is_valid_timestamp_ms(m.timestamp_ms) {
            return Err(StoreError::Invalid("timestampMs out of range"));
        }
    }
    if input
        .archived_thread_ids
        .iter()
        .chain(input.completed_reminder_ids)
        .any(|id| !valid_id(*id))
    {
        return Err(StoreError::Invalid("archive reference id out of range"));
    }
    Ok(())
}

fn row_exists(tx: &Transaction<'_>, sql: &str, id: i64) -> Result<bool> {
    Ok(tx
        .prepare_cached(sql)?
        .query_row([id], |_| Ok(()))
        .optional()?
        .is_some())
}

/// Fresh id not used locally and not claimed by this restore.
fn fresh(tx: &Transaction<'_>, kind: IdKind, claimed: &HashSet<i64>) -> Result<i64> {
    let sql = match kind {
        IdKind::Message => "SELECT 1 FROM messages WHERE id = ?1",
        IdKind::Thread => "SELECT 1 FROM messages WHERE thread_id = ?1 LIMIT 1",
    };
    loop {
        let id = allocate(tx, kind)?;
        if !claimed.contains(&id) && !row_exists(tx, sql, id)? {
            return Ok(id);
        }
    }
}

/// Transport as stored (`protocol`; NULL = SMS).
fn transport(m: &RawSms) -> &'static str {
    match m.protocol {
        Some(sms_types::MessageProtocol::Rcs) => "RCS",
        _ => "SMS",
    }
}

impl Store {
    /// Merge a decoded backup into the store atomically (see module docs).
    pub fn restore_merge(&mut self, input: &RestoreInput<'_>) -> Result<RestoreOutcome> {
        validate(input)?;
        let tx = crate::write_tx(&mut self.conn)?;
        let mut out = RestoreOutcome::default();
        let msg_next0 = stored_next(&tx, IdKind::Message)?;
        let thread_next0 = stored_next(&tx, IdKind::Thread)?;

        // ---- presence: multiset match against pre-existing rows ----------
        // Nothing is inserted in this phase, so only rows that existed
        // before the restore can match.
        // Two passes (R5 #2): exact identity matches (same id) are reserved
        // across the whole archive first, so a self-restore never maps a row
        // onto its byte-identical twin; the remaining duplicates then prefer
        // a candidate in the same thread. A pick among several equal
        // candidates with no identity or thread affinity is ambiguous: it
        // counts as present but carries no per-message flag.
        let mut consumed: HashSet<i64> = HashSet::new();
        let mut msg_map: HashMap<i64, i64> = HashMap::new();
        let mut ambiguous: HashSet<i64> = HashSet::new();
        let mut matched_threads: HashMap<i64, BTreeSet<i64>> = HashMap::new();
        let mut cands_of: Vec<Vec<(i64, i64)>> = Vec::with_capacity(input.messages.len());
        for m in input.messages {
            let cands: Vec<(i64, i64)> = tx
                .prepare_cached(
                    "SELECT id, thread_id FROM messages
                     WHERE sender_lc = ?1 AND ts_ms = ?2 AND body = ?3 AND is_outgoing = ?4
                       AND sim_slot = ?5 AND COALESCE(protocol, 'SMS') = ?6
                     ORDER BY id",
                )?
                .query_map(
                    params![
                        m.sender.to_lowercase(),
                        m.timestamp_ms,
                        m.body,
                        m.is_outgoing,
                        m.sim_slot,
                        transport(m)
                    ],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )?
                .collect::<rusqlite::Result<_>>()?;
            cands_of.push(cands);
        }
        for (m, cands) in input.messages.iter().zip(&cands_of) {
            if let Some(&(id, t)) = cands.iter().find(|(id, _)| *id == m.id)
                && consumed.insert(id)
            {
                msg_map.insert(m.id, id);
                matched_threads.entry(m.thread_id).or_default().insert(t);
                out.ingest.skipped_existing += 1;
            }
        }
        for (m, cands) in input.messages.iter().zip(&cands_of) {
            if msg_map.contains_key(&m.id) {
                continue;
            }
            let free: Vec<(i64, i64)> = cands
                .iter()
                .copied()
                .filter(|(id, _)| !consumed.contains(id))
                .collect();
            let pick = free
                .iter()
                .find(|(_, t)| *t == m.thread_id)
                .or_else(|| free.first())
                .copied();
            if let Some((id, t)) = pick {
                if free.len() > 1 && t != m.thread_id {
                    ambiguous.insert(m.id);
                }
                consumed.insert(id);
                msg_map.insert(m.id, id);
                matched_threads.entry(m.thread_id).or_default().insert(t);
                out.ingest.skipped_existing += 1;
            }
        }

        // ---- thread map --------------------------------------------------
        let mut owner: Vec<(i64, String)> = Vec::new();
        {
            let mut seen = HashSet::new();
            for m in input.messages {
                if seen.insert(m.thread_id) {
                    owner.push((m.thread_id, m.sender.to_lowercase()));
                }
            }
        }
        let mut thread_map: HashMap<i64, i64> = HashMap::new();
        // Incoming ids kept verbatim are reserved up front, so a fresh id
        // handed to a remapped thread never takes one of them.
        let mut claimed_threads: HashSet<i64> = HashSet::new();
        for (t, _) in &owner {
            if keepable(*t, thread_next0)
                && !row_exists(
                    &tx,
                    "SELECT 1 FROM messages WHERE thread_id = ?1 LIMIT 1",
                    *t,
                )?
            {
                claimed_threads.insert(*t);
            }
        }
        let mut targets: HashSet<i64> = HashSet::new();
        for (t, sender_lc) in &owner {
            let target = if let Some(local) = matched_threads.get(t) {
                if local.len() > 1 {
                    return Err(StoreError::Invalid(
                        "archive thread matches messages in several local threads",
                    ));
                }
                *local.iter().next().expect("non-empty")
            } else {
                let local_owner: Option<String> = tx
                    .prepare_cached(
                        "SELECT sender_lc FROM messages WHERE thread_id = ?1 ORDER BY ins_seq LIMIT 1",
                    )?
                    .query_row([t], |r| r.get(0))
                    .optional()?;
                if local_owner.as_deref() == Some(sender_lc.as_str()) {
                    *t
                } else if let Some(u) = tx
                    .prepare_cached(
                        "SELECT thread_id FROM messages WHERE sender_lc = ?1 ORDER BY ins_seq LIMIT 1",
                    )?
                    .query_row([sender_lc], |r| r.get::<_, i64>(0))
                    .optional()?
                {
                    u
                } else if local_owner.is_none()
                    && !targets.contains(t)
                    && claimed_threads.contains(t)
                {
                    *t
                } else {
                    fresh(&tx, IdKind::Thread, &claimed_threads)?
                }
            };
            if target != *t {
                out.remapped_threads += 1;
            }
            claimed_threads.insert(target);
            targets.insert(target);
            thread_map.insert(*t, target);
        }

        // ---- messages ----------------------------------------------------
        let mut claimed_msgs: HashSet<i64> = HashSet::new();
        for m in input.messages {
            if !msg_map.contains_key(&m.id)
                && keepable(m.id, msg_next0)
                && !row_exists(&tx, "SELECT 1 FROM messages WHERE id = ?1", m.id)?
            {
                claimed_msgs.insert(m.id);
            }
        }
        let mut touched = BTreeSet::new();
        for m in input.messages {
            if msg_map.contains_key(&m.id) {
                continue;
            }
            let id = if claimed_msgs.contains(&m.id) {
                m.id
            } else {
                out.remapped_messages += 1;
                fresh(&tx, IdKind::Message, &claimed_msgs)?
            };
            let thread_id = thread_map[&m.thread_id];
            let sms = RawSms {
                id,
                thread_id,
                ..m.clone()
            };
            let a = self.engine.analyze(&sms);
            if !insert_message(&tx, &sms, &a)? {
                return Err(StoreError::Invalid("restore id collision"));
            }
            msg_map.insert(m.id, id);
            touched.insert(thread_id);
            out.ingest.inserted += 1;
        }

        // ---- references, flags, settings ---------------------------------
        for t in input.archived_thread_ids {
            if let Some(mapped) = thread_map.get(t) {
                tx.prepare_cached("INSERT OR IGNORE INTO archived_threads(thread_id) VALUES (?1)")?
                    .execute([mapped])?;
                touched.insert(*mapped);
                out.archived_threads += 1;
            }
        }
        for s in input.blocked_senders {
            if s.trim().is_empty() {
                continue;
            }
            let lc = s.to_lowercase();
            tx.prepare_cached("INSERT OR IGNORE INTO blocked_senders(sender) VALUES (?1)")?
                .execute([&lc])?;
            let ids: Vec<i64> = tx
                .prepare_cached("SELECT thread_id FROM threads WHERE sender_lc = ?1")?
                .query_map([&lc], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            touched.extend(ids);
            out.blocked_senders += 1;
        }
        for r in input.completed_reminder_ids {
            if ambiguous.contains(r) {
                continue;
            }
            if let Some(mapped) = msg_map.get(r) {
                tx.prepare_cached("INSERT OR IGNORE INTO reminders_done(msg_id) VALUES (?1)")?
                    .execute([mapped])?;
            }
        }
        for (key, json) in input.settings {
            tx.prepare_cached(
                "INSERT INTO settings(key, json) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET json = excluded.json",
            )?
            .execute(params![key, json])?;
        }
        for t in &touched {
            recompute_thread(&tx, *t)?;
        }
        out.ingest.threads_touched = touched.len();
        tx.commit()?;
        Ok(out)
    }
}
