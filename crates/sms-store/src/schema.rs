//! Schema v1 (ARCHITECTURE §4). Superset columns: `ins_seq` (insertion order =
//! TS array order, used as the stable-sort tie-breaker), `sender_lc` (JS
//! `toLowerCase()` of sender, for blocked/ingest matching), `search_extra`
//! (OTP code, merchant, payment method, reference, reminder reference — the
//! non-body parts of the TS search haystack), `archived_threads`.

pub const SCHEMA_V1: &str = r#"
CREATE TABLE messages (
    id INTEGER PRIMARY KEY,
    ins_seq INTEGER NOT NULL UNIQUE,
    provider_id INTEGER UNIQUE,
    provider_kind TEXT,
    thread_id INTEGER NOT NULL,
    sender TEXT NOT NULL,
    sender_lc TEXT NOT NULL,
    contact_name TEXT,
    body TEXT NOT NULL,
    ts_ms INTEGER NOT NULL,
    is_known_contact INTEGER NOT NULL,
    is_outgoing INTEGER NOT NULL,
    sim_slot INTEGER NOT NULL,
    sub_id INTEGER,
    is_read INTEGER,
    is_starred INTEGER,
    protocol TEXT,
    rcs_status TEXT,
    rcs_card_json TEXT,
    category TEXT NOT NULL,
    confidence REAL NOT NULL,
    reason TEXT NOT NULL,
    scores_json TEXT NOT NULL,
    sender_kind_json TEXT NOT NULL,
    otp_json TEXT,
    txn_json TEXT,
    reminder_json TEXT,
    search_extra TEXT NOT NULL DEFAULT '',
    lang TEXT,
    send_status TEXT
);
CREATE INDEX idx_messages_thread ON messages(thread_id, ts_ms DESC, ins_seq DESC);
CREATE INDEX idx_messages_category ON messages(category, ts_ms DESC);
CREATE INDEX idx_messages_ts ON messages(ts_ms DESC, id DESC);
CREATE INDEX idx_messages_sender_lc ON messages(sender_lc);

CREATE TABLE threads (
    thread_id INTEGER PRIMARY KEY,
    latest_ts INTEGER NOT NULL,
    latest_msg_id INTEGER NOT NULL,
    sender TEXT NOT NULL,
    sender_lc TEXT NOT NULL,
    display_name TEXT NOT NULL,
    sender_kind_json TEXT NOT NULL,
    category TEXT NOT NULL,
    confidence REAL NOT NULL,
    scores_json TEXT NOT NULL,
    reason TEXT NOT NULL,
    unread INTEGER NOT NULL,
    has_starred INTEGER NOT NULL,
    is_archived INTEGER NOT NULL,
    is_blocked INTEGER NOT NULL,
    sim_slot INTEGER NOT NULL,
    is_rcs INTEGER NOT NULL,
    msg_count INTEGER NOT NULL
);
CREATE INDEX idx_threads_page ON threads(latest_ts DESC, thread_id DESC);

CREATE TABLE transactions (
    msg_id INTEGER PRIMARY KEY REFERENCES messages(id) ON DELETE CASCADE,
    ts_ms INTEGER NOT NULL,
    amount REAL NOT NULL,
    direction TEXT NOT NULL,
    method TEXT NOT NULL,
    last4 TEXT,
    merchant TEXT,
    ref_id TEXT,
    balance REAL,
    budget_category TEXT NOT NULL
);
CREATE INDEX idx_transactions_ts ON transactions(ts_ms DESC);

CREATE TABLE reminders_done (msg_id INTEGER PRIMARY KEY);
CREATE TABLE blocked_senders (sender TEXT PRIMARY KEY);
CREATE TABLE archived_threads (thread_id INTEGER PRIMARY KEY);
CREATE TABLE settings (key TEXT PRIMARY KEY, json TEXT NOT NULL);
CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);

CREATE VIRTUAL TABLE messages_fts USING fts5(
    body, search_extra,
    content='messages', content_rowid='id', tokenize='trigram'
);
CREATE TRIGGER messages_ai AFTER INSERT ON messages BEGIN
    INSERT INTO messages_fts(rowid, body, search_extra) VALUES (new.id, new.body, new.search_extra);
END;
CREATE TRIGGER messages_ad AFTER DELETE ON messages BEGIN
    INSERT INTO messages_fts(messages_fts, rowid, body, search_extra)
        VALUES ('delete', old.id, old.body, old.search_extra);
END;
CREATE TRIGGER messages_au AFTER UPDATE OF body, search_extra ON messages BEGIN
    INSERT INTO messages_fts(messages_fts, rowid, body, search_extra)
        VALUES ('delete', old.id, old.body, old.search_extra);
    INSERT INTO messages_fts(rowid, body, search_extra) VALUES (new.id, new.body, new.search_extra);
END;
"#;

/// Schema v2 (R2/R3 review):
/// - `id_alloc`: monotonic high-water marks for message and thread ids, so
///   a deleted id is never handed out again (a stale notification action or
///   send callback can never address a newer message/thread). Explicit-id
///   inserts (backup restore, seed) raise the mark too.
/// - `send_attempts` / `send_part_results`: one row per radio hand-off with
///   its provider binding (set once) and the persisted per-part sent /
///   delivery outcomes the message's `send_status` is derived from.
/// - `provider_tombstones`: provider ids of rows deleted locally, so the
///   provider replay in reconciliation never resurrects them.
pub const SCHEMA_V2: &str = r#"
CREATE TABLE id_alloc (name TEXT PRIMARY KEY, next INTEGER NOT NULL);
CREATE TABLE send_attempts (
    attempt_id INTEGER PRIMARY KEY AUTOINCREMENT,
    msg_id INTEGER NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    parts INTEGER NOT NULL,
    provider_id INTEGER,
    created_ms INTEGER NOT NULL
);
CREATE INDEX idx_send_attempts_msg ON send_attempts(msg_id);
CREATE TABLE send_part_results (
    attempt_id INTEGER NOT NULL REFERENCES send_attempts(attempt_id) ON DELETE CASCADE,
    part INTEGER NOT NULL,
    kind TEXT NOT NULL,
    outcome TEXT NOT NULL,
    PRIMARY KEY (attempt_id, part, kind)
);
CREATE TABLE provider_tombstones (provider_id INTEGER PRIMARY KEY);
CREATE TRIGGER messages_tombstone AFTER DELETE ON messages
    WHEN old.provider_id IS NOT NULL BEGIN
    INSERT OR IGNORE INTO provider_tombstones(provider_id) VALUES (old.provider_id);
END;
-- Provider rows at or below this id predate tombstones (v1 deletions were
-- not recorded), so the reconcile replay never considers them.
INSERT OR REPLACE INTO meta(key, value)
    SELECT 'provider_replay_floor', CAST(COALESCE(MAX(provider_id), 0) AS TEXT) FROM messages;
"#;
