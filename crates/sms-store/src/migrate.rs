//! Forward-only migrations keyed by `PRAGMA user_version`.

use rusqlite::Connection;

use crate::{StoreError, schema};

/// Ordered migrations; index i migrates user_version i → i+1.
const MIGRATIONS: &[&str] = &[schema::SCHEMA_V1, schema::SCHEMA_V2];

pub const LATEST_VERSION: i64 = MIGRATIONS.len() as i64;

/// Applies pending migrations. `user_version` is read inside a `BEGIN
/// IMMEDIATE` transaction and all pending steps plus the version bump commit
/// under that same lock, so two simultaneous first opens cannot both decide to
/// run v0→v1 (the loser waits on `busy_timeout`, then sees the new version).
pub fn migrate(conn: &mut Connection) -> Result<(), StoreError> {
    let tx = crate::write_tx(conn)?;
    let current: i64 = tx.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if current > LATEST_VERSION {
        return Err(StoreError::FutureSchema(current));
    }
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(current as usize) {
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", (i + 1) as i64)?;
    }
    tx.commit()?;
    Ok(())
}
