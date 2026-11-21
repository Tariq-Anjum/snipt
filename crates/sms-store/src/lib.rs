//! mySMS SQLite store: bundled SQLite (same engine on Android and Linux),
//! FTS5 trigram search, incrementally maintained `threads` table whose rules
//! equal store.ts `buildThreads`, keyset paging, finance queries.
//!
//! All SQL uses bound parameters only.

#![forbid(unsafe_code)]

use std::path::Path;
use std::time::Duration;

use rusqlite::{Connection, Transaction, TransactionBehavior};
use sms_engine::SmsEngine;

pub mod finance;
pub mod ingest;
pub mod migrate;
pub mod query;
pub mod schema;
pub mod search;
pub mod threads;

pub use ingest::{IncomingSms, IngestReport};
pub use query::MessageCursor;
pub use threads::{ThreadCursor, ThreadPage, ThreadRow};

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("database schema version {0} is newer than this app supports")]
    FutureSchema(i64),
    #[error("invalid input: {0}")]
    Invalid(&'static str),
}

pub type Result<T> = std::result::Result<T, StoreError>;

/// Write transaction that takes the RESERVED lock up front (`BEGIN
/// IMMEDIATE`). Every read-modify-write path must use this: a deferred
/// transaction that reads first cannot upgrade to a writer after another
/// connection commits (`SQLITE_BUSY_SNAPSHOT` in WAL mode, which
/// `busy_timeout` cannot resolve). With IMMEDIATE, contention waits on the
/// busy timeout instead.
pub(crate) fn write_tx(conn: &mut Connection) -> rusqlite::Result<Transaction<'_>> {
    conn.transaction_with_behavior(TransactionBehavior::Immediate)
}

pub struct Store {
    pub(crate) conn: Connection,
    pub(crate) engine: SmsEngine,
    /// Local UTC offset for billing-cycle windows (minutes east of UTC).
    pub tz_offset_min: i32,
}

impl Store {
    pub fn open(path: impl AsRef<Path>, tz_offset_min: i32) -> Result<Store> {
        Self::init(Connection::open(path)?, tz_offset_min)
    }

    pub fn open_in_memory(tz_offset_min: i32) -> Result<Store> {
        Self::init(Connection::open_in_memory()?, tz_offset_min)
    }

    fn init(mut conn: Connection, tz_offset_min: i32) -> Result<Store> {
        conn.busy_timeout(Duration::from_millis(3000))?;
        let _mode: String =
            conn.pragma_update_and_check(None, "journal_mode", "WAL", |r| r.get(0))?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        migrate::migrate(&mut conn)?;
        Ok(Store {
            conn,
            engine: SmsEngine::new(),
            tz_offset_min,
        })
    }

    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    pub fn schema_version(&self) -> Result<i64> {
        Ok(self
            .conn
            .pragma_query_value(None, "user_version", |r| r.get(0))?)
    }

    pub fn message_count(&self) -> Result<i64> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM messages", [], |r| r.get(0))?)
    }
}
