//! Monotonic message / thread id allocation (schema v2 `id_alloc`).
//!
//! Ids are never reused: the high-water mark survives deleting the newest
//! row, so a stale notification action, send callback or UI reference can
//! never address a different, newer message or thread. Every id is bounded
//! by [`MAX_ID`] (JavaScript's `Number.MAX_SAFE_INTEGER`: the UI and the TS
//! backup format carry ids as JS numbers), so `+ 1` can never overflow into
//! a SQLite REAL.

use rusqlite::{OptionalExtension, Transaction, types::Value};

use crate::{Result, StoreError};

/// Largest accepted message or thread id (2^53 - 1).
pub const MAX_ID: i64 = 9_007_199_254_740_991;

/// Largest id taken over verbatim from outside the allocator (backup
/// restore, explicit-id batch ingest). Larger external ids are remapped
/// (restore) or refused (batch ingest), so an archive can raise the
/// high-water mark to at most `2^40 + 1` and the allocator keeps
/// `MAX_ID - 2^40` (~9e15) ids of headroom: an external id can never
/// exhaust it (R4 #2). TS ids are small sequential numbers, far below this.
pub const MAX_EXTERNAL_ID: i64 = 1 << 40;

/// True for ids the store accepts (1..=MAX_ID).
pub fn valid_id(id: i64) -> bool {
    (1..=MAX_ID).contains(&id)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum IdKind {
    Message,
    Thread,
}

impl IdKind {
    fn name(self) -> &'static str {
        match self {
            IdKind::Message => "message",
            IdKind::Thread => "thread",
        }
    }

    /// store.ts starts new message ids after 1000 and thread ids after 100.
    fn floor(self) -> i64 {
        match self {
            IdKind::Message => 1000,
            IdKind::Thread => 100,
        }
    }

    fn col(self) -> &'static str {
        match self {
            IdKind::Message => "id",
            IdKind::Thread => "thread_id",
        }
    }

    fn max_sql(self) -> &'static str {
        match self {
            IdKind::Message => "SELECT MAX(id) FROM messages",
            IdKind::Thread => "SELECT MAX(thread_id) FROM messages",
        }
    }
}

fn exhausted() -> StoreError {
    StoreError::Invalid("id space exhausted")
}

/// The persisted high-water mark (`None` when nothing was ever allocated or
/// noted, e.g. a fresh store).
pub(crate) fn stored_next(tx: &Transaction<'_>, kind: IdKind) -> Result<Option<i64>> {
    Ok(tx
        .prepare_cached("SELECT next FROM id_alloc WHERE name = ?1")?
        .query_row([kind.name()], |r| r.get(0))
        .optional()?)
}

/// Whether an external id (restore) may be stored verbatim given the
/// high-water mark `next0` read before the restore started: it must be in
/// the external range and never handed out or retired before (`>= next0`).
/// Ids below the mark are either in use or retired (a deleted message's
/// stale notification or send callback could match a reused one), so they
/// are remapped instead (R4 #3).
pub(crate) fn keepable(id: i64, next0: Option<i64>) -> bool {
    (1..=MAX_EXTERNAL_ID).contains(&id) && next0.is_none_or(|n| id >= n)
}

/// Schema v4 repair for one id kind (R4 #2/#3):
/// - seeds the high-water mark from the stored maximum, so deleting the
///   newest legacy (v1) row before the first v2 allocation can no longer
///   hand its id out again;
/// - renumbers rows whose id is outside the external range (or not an
///   integer: an older build's REAL arithmetic) into fresh allocator ids,
///   with every reference, so a saturated legacy store can allocate again;
/// - clamps a saturated mark to `MAX_EXTERNAL_ID + 1`. Ids above that were
///   never handed out by the allocator; reaching them again would take
///   ~9e15 allocations.
///
/// A store with no rows and no mark is left unseeded (a restore into it
/// keeps every archive id). Caller runs inside the migration transaction
/// with `defer_foreign_keys` on.
pub(crate) fn repair_for_v4(tx: &Transaction<'_>, kind: IdKind) -> Result<usize> {
    let col = kind.col();
    let cap = MAX_EXTERNAL_ID;
    let stored = stored_next(tx, kind)?;
    let max_ok: Option<i64> = tx.query_row(
        &format!(
            "SELECT MAX({col}) FROM messages WHERE typeof({col}) = 'integer' AND {col} BETWEEN 1 AND ?1"
        ),
        [cap],
        |r| r.get(0),
    )?;
    let bad: Vec<Value> = tx
        .prepare(&format!(
            "SELECT DISTINCT {col} FROM messages
             WHERE typeof({col}) <> 'integer' OR {col} < 1 OR {col} > ?1 ORDER BY {col}"
        ))?
        .query_map([cap], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    if stored.is_none() && max_ok.is_none() && bad.is_empty() {
        return Ok(0);
    }
    let mut next = stored
        .unwrap_or(0)
        .min(cap + 1)
        .max(max_ok.map_or(0, |m| m + 1));
    if !bad.is_empty() {
        next = next.max(kind.floor() + 1);
    }
    for old in &bad {
        let new = next;
        next += 1;
        match kind {
            IdKind::Message => {
                for sql in [
                    "UPDATE messages SET id = ?2 WHERE id = ?1",
                    "UPDATE transactions SET msg_id = ?2 WHERE msg_id = ?1",
                    "UPDATE reminders_done SET msg_id = ?2 WHERE msg_id = ?1",
                    "UPDATE send_attempts SET msg_id = ?2 WHERE msg_id = ?1",
                    "UPDATE mms_parts SET msg_id = ?2 WHERE msg_id = ?1",
                    "UPDATE mms_pending SET msg_id = ?2 WHERE msg_id = ?1",
                    "UPDATE threads SET latest_msg_id = ?2 WHERE latest_msg_id = ?1",
                ] {
                    tx.execute(sql, rusqlite::params![old, new])?;
                }
            }
            IdKind::Thread => {
                for sql in [
                    "UPDATE messages SET thread_id = ?2 WHERE thread_id = ?1",
                    "DELETE FROM threads WHERE thread_id = ?1 AND ?2 IS NOT NULL",
                    "UPDATE archived_threads SET thread_id = ?2 WHERE thread_id = ?1",
                ] {
                    tx.execute(sql, rusqlite::params![old, new])?;
                }
                crate::threads::recompute_thread(tx, new)?;
            }
        }
    }
    if kind == IdKind::Message && !bad.is_empty() {
        // The FTS index is keyed by message id.
        tx.execute_batch("INSERT INTO messages_fts(messages_fts) VALUES ('rebuild');")?;
    }
    tx.execute(
        "INSERT INTO id_alloc(name, next) VALUES (?1, ?2)
         ON CONFLICT(name) DO UPDATE SET next = excluded.next",
        rusqlite::params![kind.name(), next],
    )?;
    Ok(bad.len())
}

/// Next unused id of `kind`; raises the high-water mark in the same
/// transaction.
pub(crate) fn allocate(tx: &Transaction<'_>, kind: IdKind) -> Result<i64> {
    let stored = stored_next(tx, kind)?;
    let max = match tx
        .prepare_cached(kind.max_sql())?
        .query_row([], |r| r.get::<_, Value>(0))?
    {
        Value::Null => kind.floor(),
        Value::Integer(i) => i.max(kind.floor()),
        // A non-integer id (written by an older build's REAL arithmetic)
        // cannot be continued safely.
        _ => return Err(exhausted()),
    };
    let next = max
        .checked_add(1)
        .ok_or_else(exhausted)?
        .max(stored.unwrap_or(0));
    if next > MAX_ID {
        return Err(exhausted());
    }
    note_used(tx, kind, next)?;
    Ok(next)
}

/// Record that `id` (already validated by [`valid_id`]) is in use.
pub(crate) fn note_used(tx: &Transaction<'_>, kind: IdKind, id: i64) -> Result<()> {
    debug_assert!(valid_id(id));
    tx.prepare_cached(
        "INSERT INTO id_alloc(name, next) VALUES (?1, ?2)
         ON CONFLICT(name) DO UPDATE SET next = MAX(next, excluded.next)",
    )?
    .execute(rusqlite::params![kind.name(), id + 1])?;
    Ok(())
}
