//! Backup restore as one atomic merge (R2 #2).
//!
//! Rules:
//! - Everything is validated before the first write: message / thread /
//!   archived-thread / completed-reminder ids must be in `1..=MAX_ID`,
//!   message ids unique within the archive, SIM slots 0/1, timestamps in the
//!   JS `Date` range. A bad archive changes nothing.
//! - A message counts as "already present" only when a stored row has the
//!   same stable identity: sender (case-insensitive), timestamp, body and
//!   direction. An id collision with a *different* message is not presence:
//!   the incoming row gets a fresh id (monotonic allocator) and is kept.
//! - Thread ids are mapped per incoming thread: kept when the local thread
//!   with that id belongs to the same sender (or the id is unused), joined to
//!   the sender's existing local thread otherwise, else given a fresh id.
//!   Archived-thread and completed-reminder references follow the maps;
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

use crate::ids::{IdKind, allocate, valid_id};
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
    /// Incoming messages stored under a new id (id collision, different message).
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

impl Store {
    /// Merge a decoded backup into the store atomically (see module docs).
    pub fn restore_merge(&mut self, input: &RestoreInput<'_>) -> Result<RestoreOutcome> {
        validate(input)?;
        let tx = crate::write_tx(&mut self.conn)?;
        let mut out = RestoreOutcome::default();

        // ---- thread map ------------------------------------------------
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
        // Incoming ids that are free locally are reserved up front, so a
        // fresh id handed to a colliding row never takes one of them (no
        // cascading remaps).
        let mut claimed_threads: HashSet<i64> = HashSet::new();
        for (t, _) in &owner {
            if !row_exists(
                &tx,
                "SELECT 1 FROM messages WHERE thread_id = ?1 LIMIT 1",
                *t,
            )? {
                claimed_threads.insert(*t);
            }
        }
        let mut targets: HashSet<i64> = HashSet::new();
        for (t, sender_lc) in &owner {
            let local_owner: Option<String> = tx
                .prepare_cached(
                    "SELECT sender_lc FROM messages WHERE thread_id = ?1 ORDER BY ins_seq LIMIT 1",
                )?
                .query_row([t], |r| r.get(0))
                .optional()?;
            let target = if local_owner.as_deref() == Some(sender_lc.as_str()) {
                *t
            } else if let Some(u) = tx
                .prepare_cached(
                    "SELECT thread_id FROM messages WHERE sender_lc = ?1 ORDER BY ins_seq LIMIT 1",
                )?
                .query_row([sender_lc], |r| r.get::<_, i64>(0))
                .optional()?
            {
                u
            } else if local_owner.is_none() && !targets.contains(t) {
                *t
            } else {
                fresh(&tx, IdKind::Thread, &claimed_threads)?
            };
            if target != *t {
                out.remapped_threads += 1;
            }
            claimed_threads.insert(target);
            targets.insert(target);
            thread_map.insert(*t, target);
        }

        // ---- messages ----------------------------------------------------
        let mut msg_map: HashMap<i64, i64> = HashMap::new();
        let mut claimed_msgs: HashSet<i64> = HashSet::new();
        for m in input.messages {
            if !row_exists(&tx, "SELECT 1 FROM messages WHERE id = ?1", m.id)? {
                claimed_msgs.insert(m.id);
            }
        }
        let mut used: HashSet<i64> = HashSet::new();
        let mut touched = BTreeSet::new();
        for m in input.messages {
            let same: Option<i64> = tx
                .prepare_cached(
                    "SELECT id FROM messages
                     WHERE sender_lc = ?1 AND ts_ms = ?2 AND body = ?3 AND is_outgoing = ?4
                     ORDER BY id LIMIT 1",
                )?
                .query_row(
                    params![
                        m.sender.to_lowercase(),
                        m.timestamp_ms,
                        m.body,
                        m.is_outgoing
                    ],
                    |r| r.get(0),
                )
                .optional()?;
            if let Some(existing) = same {
                msg_map.insert(m.id, existing);
                out.ingest.skipped_existing += 1;
                continue;
            }
            let id = if !used.contains(&m.id)
                && !row_exists(&tx, "SELECT 1 FROM messages WHERE id = ?1", m.id)?
            {
                m.id
            } else {
                out.remapped_messages += 1;
                fresh(&tx, IdKind::Message, &claimed_msgs)?
            };
            claimed_msgs.insert(id);
            used.insert(id);
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
