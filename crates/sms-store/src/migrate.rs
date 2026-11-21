//! Forward-only migrations keyed by `PRAGMA user_version`.

use rusqlite::Connection;

use crate::{StoreError, schema};

/// Ordered migrations; index i migrates user_version i → i+1.
const MIGRATIONS: &[&str] = &[schema::SCHEMA_V1];

pub const LATEST_VERSION: i64 = MIGRATIONS.len() as i64;

pub fn migrate(conn: &mut Connection) -> Result<(), StoreError> {
    let current: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if current > LATEST_VERSION {
        return Err(StoreError::FutureSchema(current));
    }
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(current as usize) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", (i + 1) as i64)?;
        tx.commit()?;
    }
    Ok(())
}
