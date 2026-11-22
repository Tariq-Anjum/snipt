//! Forward-only migrations keyed by `PRAGMA user_version`.

use rusqlite::{Connection, Transaction};

use crate::{StoreError, schema};

enum Step {
    Sql(&'static str),
    /// SQL followed by a Rust data repair, in the same transaction.
    SqlThen(&'static str, fn(&Transaction<'_>) -> Result<(), StoreError>),
}

/// Ordered migrations; index i migrates user_version i → i+1.
const MIGRATIONS: &[Step] = &[
    Step::Sql(schema::SCHEMA_V1),
    Step::Sql(schema::SCHEMA_V2),
    Step::Sql(schema::SCHEMA_V3),
    Step::SqlThen(schema::SCHEMA_V4, schema::repair_v4),
    // v5: finance rules fixed (balance/limit amounts, due notices, biller
    // receipts, card-on-UPI, merchants); re-analyse stored messages once.
    Step::SqlThen("SELECT 1;", repair_v5),
    // v6: own-account transfers (InfoBIL*INFT*) are not spending; re-analyse.
    Step::SqlThen("SELECT 1;", repair_v5),
    // v7: MMS identity/in-flight/display eligibility (R5 #5-#7) and the
    // service-notice classification rules: re-analyse once more.
    Step::SqlThen(schema::SCHEMA_V7, repair_v5),
];

fn repair_v5(tx: &Transaction<'_>) -> Result<(), StoreError> {
    crate::reanalyze::reanalyze_all(tx).map(|_| ())
}

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
    for (i, step) in MIGRATIONS.iter().enumerate().skip(current as usize) {
        match step {
            Step::Sql(sql) => tx.execute_batch(sql)?,
            Step::SqlThen(sql, repair) => {
                tx.execute_batch(sql)?;
                repair(&tx)?;
            }
        }
        tx.pragma_update(None, "user_version", (i + 1) as i64)?;
    }
    tx.commit()?;
    Ok(())
}
