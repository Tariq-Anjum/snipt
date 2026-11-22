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

/// Schema v3 (S15, MMS receive):
/// - `messages.mms_id`: the `content://mms` `_id` of a received MMS. A
///   separate column from `provider_id` (SMS `_id`), so the two provider id
///   namespaces never collide; unique when set.
/// - `mms_parts`: non-text parts of an MMS (text parts are the message
///   body). `kind` = `image` (bytes kept, magic-byte checked, downscaled
///   `thumb` added by the platform) or `unsupported` (metadata only; the
///   provider keeps the original). Deleted with the message (FK cascade).
/// - `mms_pending`: one row per M-Notification.ind transaction: download
///   location, attempts, expiry and state, so retries survive process death
///   and a repeated WAP push never creates a second message.
pub const SCHEMA_V3: &str = r#"
ALTER TABLE messages ADD COLUMN mms_id INTEGER;
CREATE UNIQUE INDEX idx_messages_mms_id ON messages(mms_id) WHERE mms_id IS NOT NULL;
CREATE TABLE mms_parts (
    part_id INTEGER PRIMARY KEY AUTOINCREMENT,
    msg_id INTEGER NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    seq INTEGER NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('image', 'unsupported')),
    content_type TEXT NOT NULL,
    name TEXT,
    size INTEGER NOT NULL,
    data BLOB,
    thumb BLOB,
    UNIQUE (msg_id, seq)
);
CREATE TABLE mms_pending (
    pending_id INTEGER PRIMARY KEY AUTOINCREMENT,
    transaction_id TEXT NOT NULL UNIQUE,
    location TEXT NOT NULL,
    sender TEXT,
    sub_id INTEGER,
    sim_slot INTEGER NOT NULL,
    received_ms INTEGER NOT NULL,
    expiry_ms INTEGER,
    attempts INTEGER NOT NULL DEFAULT 0,
    state TEXT NOT NULL DEFAULT 'pending' CHECK (state IN ('pending', 'done', 'failed')),
    msg_id INTEGER
);
"#;

/// Schema v4 (R4 review):
/// - `send_attempts.handoff`: 0 while the platform is handing an attempt to
///   the radio (between `begin_send_attempt` and `end_send_handoff`). A
///   message with an open hand-off cannot be deleted, so a delete can no
///   longer cascade the attempt away mid hand-off. Existing attempts are
///   finished (1).
/// - [`repair_v4`]: legacy in-flight sends become `indeterminate`, id
///   high-water marks are seeded and saturated ids renumbered.
pub const SCHEMA_V4: &str = r#"
ALTER TABLE send_attempts ADD COLUMN handoff INTEGER NOT NULL DEFAULT 1;
"#;

/// Data part of the v4 migration (inside the migration transaction):
/// - a `sending` row with no send attempt was in flight under v1, whose
///   callbacks carried no attempt id and are now ignored: it moves to the
///   explicit `indeterminate` state (never resent);
/// - message and thread id marks are seeded from the stored maxima and
///   saturated ids renumbered ([`crate::ids::repair_for_v4`]).
pub(crate) fn repair_v4(tx: &rusqlite::Transaction<'_>) -> Result<(), crate::StoreError> {
    tx.execute_batch(
        "UPDATE messages SET send_status = 'indeterminate'
         WHERE is_outgoing = 1 AND send_status = 'sending'
           AND NOT EXISTS (SELECT 1 FROM send_attempts a WHERE a.msg_id = messages.id);
         PRAGMA defer_foreign_keys = ON;",
    )?;
    crate::ids::repair_for_v4(tx, crate::ids::IdKind::Message)?;
    crate::ids::repair_for_v4(tx, crate::ids::IdKind::Thread)?;
    Ok(())
}

/// Schema v7 (R5 #5-#7):
/// - `mms_pending` deduplicates per (transaction id, content location,
///   subscription): OMA scopes the transaction id to one notification
///   exchange, so two SIMs (or a reused id) are separate downloads.
/// - `mms_pending.inflight_ms`: when the running download attempt started;
///   maintenance never finalizes an attempt that is still in flight.
/// - `mms_parts.display_ok`: set when the platform thumbnailer validated the
///   image (dimension cap); only such images are ever served to the UI.
pub const SCHEMA_V7: &str = r#"
CREATE TABLE mms_pending_v7 (
    pending_id INTEGER PRIMARY KEY AUTOINCREMENT,
    transaction_id TEXT NOT NULL,
    location TEXT NOT NULL,
    sender TEXT,
    sub_id INTEGER,
    sim_slot INTEGER NOT NULL,
    received_ms INTEGER NOT NULL,
    expiry_ms INTEGER,
    attempts INTEGER NOT NULL DEFAULT 0,
    state TEXT NOT NULL DEFAULT 'pending' CHECK (state IN ('pending', 'done', 'failed')),
    msg_id INTEGER,
    inflight_ms INTEGER
);
INSERT INTO mms_pending_v7 (pending_id, transaction_id, location, sender, sub_id, sim_slot,
    received_ms, expiry_ms, attempts, state, msg_id)
    SELECT pending_id, transaction_id, location, sender, sub_id, sim_slot,
        received_ms, expiry_ms, attempts, state, msg_id FROM mms_pending;
DROP TABLE mms_pending;
ALTER TABLE mms_pending_v7 RENAME TO mms_pending;
CREATE UNIQUE INDEX idx_mms_pending_identity
    ON mms_pending(transaction_id, location, COALESCE(sub_id, -1));
ALTER TABLE mms_parts ADD COLUMN display_ok INTEGER NOT NULL DEFAULT 0;
UPDATE mms_parts SET display_ok = 1 WHERE thumb IS NOT NULL;
"#;
