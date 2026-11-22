//! mySMS SQLite store: bundled SQLite (same engine on Android and Linux),
//! FTS5 trigram search, incrementally maintained `threads` table whose rules
//! equal store.ts `buildThreads`, keyset paging, finance queries.
//!
//! All SQL uses bound parameters only.

#![forbid(unsafe_code)]

use std::path::Path;
use std::time::{Duration, Instant};

use rusqlite::{Connection, ErrorCode, Transaction, TransactionBehavior};
use sms_engine::SmsEngine;

pub mod finance;
pub mod fsperm;
pub mod ids;
pub mod ingest;
pub mod migrate;
pub mod platform;
pub mod query;
pub mod schema;
pub mod search;
pub mod threads;

pub use ingest::{IncomingSms, IngestReport};
pub use platform::{
    FilterCounts, InboxCounts, OutgoingSms, PartKind, PartOutcome, ProviderRef, Reconciled,
    SendStatus, SendUpdate,
};
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
    #[error("database file: {0}")]
    Io(std::io::Error),
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

const BUSY_TIMEOUT: Duration = Duration::from_millis(3000);

/// `PRAGMA journal_mode=WAL` with a bounded retry. On a fresh file, a
/// concurrent first open can make the mode switch fail with SQLITE_BUSY
/// without SQLite invoking the busy handler (observed in
/// `tests/concurrency.rs`), so retry up to the busy timeout. WAL mode is
/// persistent: once any connection has switched, the retry succeeds at once.
fn enable_wal(conn: &Connection) -> rusqlite::Result<()> {
    let deadline = Instant::now() + BUSY_TIMEOUT;
    loop {
        match conn.pragma_update_and_check(None, "journal_mode", "WAL", |r| r.get::<_, String>(0)) {
            Ok(_) => return Ok(()),
            Err(rusqlite::Error::SqliteFailure(e, _))
                if e.code == ErrorCode::DatabaseBusy && Instant::now() < deadline =>
            {
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(e) => return Err(e),
        }
    }
}

pub struct Store {
    pub(crate) conn: Connection,
    pub(crate) engine: SmsEngine,
    /// Local UTC offset for billing-cycle windows (minutes east of UTC).
    pub tz_offset_min: i32,
}

impl Store {
    /// Opens (creating if needed) the database file. On unix the file is
    /// created/tightened to 0600 first, so SQLite's `-wal`/`-shm` files
    /// inherit owner-only permissions ([`fsperm::prepare_private_db`]).
    pub fn open(path: impl AsRef<Path>, tz_offset_min: i32) -> Result<Store> {
        let path = path.as_ref();
        fsperm::prepare_private_db(path).map_err(StoreError::Io)?;
        Self::init(Connection::open(path)?, tz_offset_min)
    }

    pub fn open_in_memory(tz_offset_min: i32) -> Result<Store> {
        Self::init(Connection::open_in_memory()?, tz_offset_min)
    }

    fn init(mut conn: Connection, tz_offset_min: i32) -> Result<Store> {
        conn.busy_timeout(BUSY_TIMEOUT)?;
        enable_wal(&conn)?;
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
