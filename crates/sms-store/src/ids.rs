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

/// Next unused id of `kind`; raises the high-water mark in the same
/// transaction.
pub(crate) fn allocate(tx: &Transaction<'_>, kind: IdKind) -> Result<i64> {
    let stored: Option<i64> = tx
        .prepare_cached("SELECT next FROM id_alloc WHERE name = ?1")?
        .query_row([kind.name()], |r| r.get(0))
        .optional()?;
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
