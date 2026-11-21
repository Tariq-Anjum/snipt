//! Incrementally maintained thread summaries (rules of store.ts `buildThreads`)
//! and thread-level mutations. Thread consensus only needs the latest 12
//! messages, so a recompute is O(1) queries per touched thread.

use rusqlite::{Connection, OptionalExtension, Row, params};
use serde::{Deserialize, Serialize};
use sms_engine::{
    dlt::parse_sender,
    inbox::InboxFilter,
    thread::{classify_thread_classes, display_name},
};
use sms_types::{
    Category, Classification, ClassificationReason, ExtractedOtp, ExtractedReminder,
    ExtractedTransaction, SenderKind,
};

use crate::{Result, Store, StoreError};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ThreadRow {
    pub thread_id: i64,
    pub sender: String,
    pub display_name: String,
    pub sender_kind: SenderKind,
    pub latest_ts: i64,
    pub latest_message_id: i64,
    pub classification: Classification,
    pub unread_count: i64,
    pub has_starred: bool,
    pub is_archived: bool,
    pub is_blocked: bool,
    pub sim_slot: u8,
    pub is_rcs_capable: bool,
    pub message_count: i64,
    /// Latest message body for the list row: OTP codes masked, line breaks
    /// collapsed, at most [`PREVIEW_CHARS`] characters (store-only field; the
    /// TS `ThreadSummary` carries the whole `latestMessage` instead).
    pub preview: String,
    pub preview_is_outgoing: bool,
    /// The latest message carries an OTP (the code itself is not exposed here).
    pub latest_has_otp: bool,
    pub latest_transaction: Option<ExtractedTransaction>,
    pub latest_reminder: Option<ExtractedReminder>,
}

/// Maximum preview length in Unicode scalar values (including the ellipsis).
pub const PREVIEW_CHARS: usize = 140;

/// Thread-list preview: redact the OTP, collapse whitespace runs to one space,
/// cap at [`PREVIEW_CHARS`] characters.
pub fn make_preview(body: &str, otp_code: Option<&str>) -> String {
    let redacted = match otp_code {
        Some(code) => sms_engine::otp::redact_otp_code(body, code),
        None => body.to_string(),
    };
    let collapsed = redacted
        .split(sms_types::js::is_js_whitespace)
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    if collapsed.chars().count() <= PREVIEW_CHARS {
        return collapsed;
    }
    let mut out: String = collapsed.chars().take(PREVIEW_CHARS - 1).collect();
    out.push('\u{2026}');
    out
}

/// Keyset cursor for the thread list: `(latest_ts, thread_id)` of the last row.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ThreadCursor {
    pub latest_ts: i64,
    pub thread_id: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ThreadPage {
    pub threads: Vec<ThreadRow>,
    pub next: Option<ThreadCursor>,
}

pub(crate) fn parse_reason(s: &str) -> ClassificationReason {
    serde_json::from_value(serde_json::Value::String(s.to_string()))
        .unwrap_or(ClassificationReason::MlLinear)
}

pub(crate) fn classification_from(
    category: &str,
    confidence: f64,
    scores_json: &str,
    reason: &str,
) -> Result<Classification> {
    Ok(Classification {
        category: Category::parse(category).ok_or(StoreError::Invalid("category"))?,
        confidence,
        scores: serde_json::from_str(scores_json)?,
        reason: parse_reason(reason),
    })
}

pub(crate) const THREAD_COLS: &str = "thread_id, sender, display_name, sender_kind_json, latest_ts,
    latest_msg_id, category, confidence, scores_json, reason, unread, has_starred, is_archived,
    is_blocked, sim_slot, is_rcs, msg_count";

pub(crate) fn thread_from_row(r: &Row<'_>) -> rusqlite::Result<(ThreadRowRaw,)> {
    Ok((ThreadRowRaw {
        thread_id: r.get(0)?,
        sender: r.get(1)?,
        display_name: r.get(2)?,
        sender_kind_json: r.get(3)?,
        latest_ts: r.get(4)?,
        latest_msg_id: r.get(5)?,
        category: r.get(6)?,
        confidence: r.get(7)?,
        scores_json: r.get(8)?,
        reason: r.get(9)?,
        unread: r.get(10)?,
        has_starred: r.get(11)?,
        is_archived: r.get(12)?,
        is_blocked: r.get(13)?,
        sim_slot: r.get(14)?,
        is_rcs: r.get(15)?,
        msg_count: r.get(16)?,
    },))
}

pub(crate) struct ThreadRowRaw {
    thread_id: i64,
    sender: String,
    display_name: String,
    sender_kind_json: String,
    latest_ts: i64,
    latest_msg_id: i64,
    category: String,
    confidence: f64,
    scores_json: String,
    reason: String,
    unread: i64,
    has_starred: bool,
    is_archived: bool,
    is_blocked: bool,
    sim_slot: u8,
    is_rcs: bool,
    msg_count: i64,
}

impl ThreadRowRaw {
    pub(crate) fn finish(self) -> Result<ThreadRow> {
        Ok(ThreadRow {
            thread_id: self.thread_id,
            sender: self.sender,
            display_name: self.display_name,
            sender_kind: serde_json::from_str(&self.sender_kind_json)?,
            latest_ts: self.latest_ts,
            latest_message_id: self.latest_msg_id,
            classification: classification_from(
                &self.category,
                self.confidence,
                &self.scores_json,
                &self.reason,
            )?,
            unread_count: self.unread,
            has_starred: self.has_starred,
            is_archived: self.is_archived,
            is_blocked: self.is_blocked,
            sim_slot: self.sim_slot,
            is_rcs_capable: self.is_rcs,
            message_count: self.msg_count,
            preview: String::new(),
            preview_is_outgoing: false,
            latest_has_otp: false,
            latest_transaction: None,
            latest_reminder: None,
        })
    }
}

/// body, is_outgoing, otp_json, txn_json, reminder_json of the latest message.
type LatestCols = (String, bool, Option<String>, Option<String>, Option<String>);

/// Fill the preview fields of each row from its latest message (one primary
/// key lookup per row; pages are at most 500 rows).
pub(crate) fn attach_previews(conn: &Connection, rows: &mut [ThreadRow]) -> Result<()> {
    let mut stmt = conn.prepare_cached(
        "SELECT body, is_outgoing, otp_json, txn_json, reminder_json FROM messages WHERE id = ?1",
    )?;
    for row in rows.iter_mut() {
        let got: Option<LatestCols> = stmt
            .query_row([row.latest_message_id], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
            })
            .optional()?;
        let Some((body, outgoing, otp, txn, reminder)) = got else {
            continue;
        };
        let otp: Option<ExtractedOtp> = otp.map(|j| serde_json::from_str(&j)).transpose()?;
        row.preview = make_preview(&body, otp.as_ref().map(|o| o.code.as_str()));
        row.preview_is_outgoing = outgoing;
        row.latest_has_otp = otp.is_some();
        row.latest_transaction = txn.map(|j| serde_json::from_str(&j)).transpose()?;
        row.latest_reminder = reminder.map(|j| serde_json::from_str(&j)).transpose()?;
    }
    Ok(())
}

/// Run a `SELECT {THREAD_COLS} ...` query and return finished rows with previews.
pub(crate) fn query_rows(
    conn: &Connection,
    sql: &str,
    args: impl rusqlite::Params,
) -> Result<Vec<ThreadRow>> {
    let mut stmt = conn.prepare_cached(sql)?;
    let mut rows: Vec<ThreadRow> = stmt
        .query_map(args, thread_from_row)?
        .map(|r| r.map_err(Into::into).and_then(|(t,)| t.finish()))
        .collect::<Result<_>>()?;
    attach_previews(conn, &mut rows)?;
    Ok(rows)
}

/// Recompute one thread row from its messages (or delete it when empty).
pub(crate) fn recompute_thread(conn: &Connection, thread_id: i64) -> Result<()> {
    let mut stmt = conn.prepare_cached(
        "SELECT category, confidence, scores_json, reason FROM messages WHERE thread_id = ?1
         ORDER BY ts_ms DESC, ins_seq DESC LIMIT 12",
    )?;
    let mut recent: Vec<Classification> = stmt
        .query_map([thread_id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, f64>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })?
        .map(|row| {
            let (c, conf, s, reason) = row?;
            classification_from(&c, conf, &s, &reason)
        })
        .collect::<Result<_>>()?;
    if recent.is_empty() {
        conn.execute("DELETE FROM threads WHERE thread_id = ?1", [thread_id])?;
        return Ok(());
    }
    recent.reverse();

    let (latest_id, latest_ts, sender, sender_lc, sim_slot, known): (
        i64,
        i64,
        String,
        String,
        i64,
        bool,
    ) = conn
        .prepare_cached(
            "SELECT id, ts_ms, sender, sender_lc, sim_slot, is_known_contact FROM messages
             WHERE thread_id = ?1 ORDER BY ts_ms DESC, ins_seq DESC LIMIT 1",
        )?
        .query_row([thread_id], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
            ))
        })?;
    let (count, unread, has_starred, is_rcs): (i64, i64, bool, bool) = conn
        .prepare_cached(
            "SELECT COUNT(*),
                    COALESCE(SUM(is_outgoing = 0 AND COALESCE(is_read, 0) = 0), 0),
                    COALESCE(MAX(COALESCE(is_starred, 0)), 0),
                    COALESCE(MAX(protocol = 'RCS'), 0)
             FROM messages WHERE thread_id = ?1",
        )?
        .query_row([thread_id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })?;
    let contact: Option<String> = conn
        .prepare_cached(
            "SELECT contact_name FROM messages WHERE thread_id = ?1 AND contact_name <> ''
             ORDER BY ts_ms, ins_seq LIMIT 1",
        )?
        .query_row([thread_id], |r| r.get(0))
        .optional()?;
    let is_blocked = conn
        .prepare_cached("SELECT 1 FROM blocked_senders WHERE sender = ?1")?
        .query_row([&sender_lc], |_| Ok(()))
        .optional()?
        .is_some();
    let is_archived = conn
        .prepare_cached("SELECT 1 FROM archived_threads WHERE thread_id = ?1")?
        .query_row([thread_id], |_| Ok(()))
        .optional()?
        .is_some();

    let kind = parse_sender(&sender, known);
    let refs: Vec<&Classification> = recent.iter().collect();
    let mut cls = classify_thread_classes(&refs);
    if is_blocked {
        cls.category = Category::Spam;
        cls.confidence = 1.0;
    }
    let display = display_name(contact.as_deref(), &kind, &sender);

    conn.prepare_cached(
        "INSERT INTO threads (thread_id, latest_ts, latest_msg_id, sender, sender_lc, display_name,
            sender_kind_json, category, confidence, scores_json, reason, unread, has_starred,
            is_archived, is_blocked, sim_slot, is_rcs, msg_count)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)
         ON CONFLICT(thread_id) DO UPDATE SET latest_ts = excluded.latest_ts,
            latest_msg_id = excluded.latest_msg_id, sender = excluded.sender,
            sender_lc = excluded.sender_lc, display_name = excluded.display_name,
            sender_kind_json = excluded.sender_kind_json, category = excluded.category,
            confidence = excluded.confidence, scores_json = excluded.scores_json,
            reason = excluded.reason, unread = excluded.unread,
            has_starred = excluded.has_starred, is_archived = excluded.is_archived,
            is_blocked = excluded.is_blocked, sim_slot = excluded.sim_slot,
            is_rcs = excluded.is_rcs, msg_count = excluded.msg_count",
    )?
    .execute(params![
        thread_id,
        latest_ts,
        latest_id,
        sender,
        sender_lc,
        display,
        serde_json::to_string(&kind)?,
        cls.category.as_str(),
        cls.confidence,
        serde_json::to_string(&cls.scores)?,
        crate::ingest::enum_str(&cls.reason),
        unread,
        has_starred,
        is_archived,
        is_blocked,
        sim_slot,
        is_rcs,
        count,
    ])?;
    Ok(())
}

/// SQL predicate + params for the folder/category/SIM part of filterThreads.
pub(crate) fn filter_sql(
    filter: InboxFilter,
    sim: Option<u8>,
) -> (String, Vec<rusqlite::types::Value>) {
    use rusqlite::types::Value as V;
    let mut clauses: Vec<String> = Vec::new();
    let mut args: Vec<V> = Vec::new();
    match filter {
        InboxFilter::Archived => clauses.push("is_archived = 1".into()),
        other => {
            clauses.push("is_archived = 0".into());
            match other {
                InboxFilter::Starred => clauses.push("has_starred = 1".into()),
                InboxFilter::Rcs => clauses.push("is_rcs = 1".into()),
                InboxFilter::All => clauses.push("category <> 'Spam'".into()),
                InboxFilter::Category(c) => {
                    clauses.push(format!("category = ?{}", args.len() + 1));
                    args.push(V::Text(c.as_str().into()));
                }
                InboxFilter::Archived => unreachable!(),
            }
        }
    }
    if let Some(s) = sim {
        clauses.push(format!("sim_slot = ?{}", args.len() + 1));
        args.push(V::Integer(i64::from(s)));
    }
    (clauses.join(" AND "), args)
}

impl Store {
    /// One keyset page of the thread list, newest first.
    pub fn threads_page(
        &self,
        filter: InboxFilter,
        sim: Option<u8>,
        after: Option<ThreadCursor>,
        limit: usize,
    ) -> Result<ThreadPage> {
        use rusqlite::types::Value as V;
        let limit = limit.clamp(1, 500);
        let (mut pred, mut args) = filter_sql(filter, sim);
        if let Some(c) = after {
            let (a, b) = (args.len() + 1, args.len() + 2);
            pred.push_str(&format!(
                " AND (latest_ts < ?{a} OR (latest_ts = ?{a} AND thread_id < ?{b}))"
            ));
            args.push(V::Integer(c.latest_ts));
            args.push(V::Integer(c.thread_id));
        }
        let sql = format!(
            "SELECT {THREAD_COLS} FROM threads WHERE {pred}
             ORDER BY latest_ts DESC, thread_id DESC LIMIT {}",
            limit + 1
        );
        let mut threads = query_rows(&self.conn, &sql, rusqlite::params_from_iter(args))?;
        let next = if threads.len() > limit {
            threads.truncate(limit);
            threads.last().map(|t| ThreadCursor {
                latest_ts: t.latest_ts,
                thread_id: t.thread_id,
            })
        } else {
            None
        };
        Ok(ThreadPage { threads, next })
    }

    /// All thread rows in list order (latest first).
    pub fn all_threads(&self) -> Result<Vec<ThreadRow>> {
        let sql =
            format!("SELECT {THREAD_COLS} FROM threads ORDER BY latest_ts DESC, thread_id DESC");
        query_rows(&self.conn, &sql, [])
    }

    pub fn thread(&self, thread_id: i64) -> Result<Option<ThreadRow>> {
        let sql = format!("SELECT {THREAD_COLS} FROM threads WHERE thread_id = ?1");
        Ok(query_rows(&self.conn, &sql, [thread_id])?.pop())
    }

    pub fn set_archived(&mut self, thread_id: i64, archived: bool) -> Result<()> {
        let tx = crate::write_tx(&mut self.conn)?;
        if archived {
            tx.execute(
                "INSERT OR IGNORE INTO archived_threads(thread_id) VALUES (?1)",
                [thread_id],
            )?;
        } else {
            tx.execute(
                "DELETE FROM archived_threads WHERE thread_id = ?1",
                [thread_id],
            )?;
        }
        tx.execute(
            "UPDATE threads SET is_archived = ?2 WHERE thread_id = ?1",
            params![thread_id, archived],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Block/unblock a sender (case-insensitive, JS `toLowerCase`).
    pub fn set_blocked(&mut self, sender: &str, blocked: bool) -> Result<()> {
        let lc = sender.to_lowercase();
        let tx = crate::write_tx(&mut self.conn)?;
        if blocked {
            tx.execute(
                "INSERT OR IGNORE INTO blocked_senders(sender) VALUES (?1)",
                [&lc],
            )?;
        } else {
            tx.execute("DELETE FROM blocked_senders WHERE sender = ?1", [&lc])?;
        }
        let ids: Vec<i64> = tx
            .prepare("SELECT thread_id FROM threads WHERE sender_lc = ?1")?
            .query_map([&lc], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        for t in ids {
            recompute_thread(&tx, t)?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn blocked_senders(&self) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare_cached("SELECT sender FROM blocked_senders ORDER BY sender")?;
        let v = stmt
            .query_map([], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        Ok(v)
    }

    pub fn mark_read(&mut self, sms_id: i64) -> Result<bool> {
        self.update_flag("UPDATE messages SET is_read = 1 WHERE id = ?1", sms_id)
    }

    pub fn set_starred(&mut self, sms_id: i64, starred: bool) -> Result<bool> {
        let sql = if starred {
            "UPDATE messages SET is_starred = 1 WHERE id = ?1"
        } else {
            "UPDATE messages SET is_starred = 0 WHERE id = ?1"
        };
        self.update_flag(sql, sms_id)
    }

    fn update_flag(&mut self, sql: &str, sms_id: i64) -> Result<bool> {
        let tx = crate::write_tx(&mut self.conn)?;
        let n = tx.execute(sql, [sms_id])?;
        if n > 0 {
            let t: i64 = tx.query_row(
                "SELECT thread_id FROM messages WHERE id = ?1",
                [sms_id],
                |r| r.get(0),
            )?;
            recompute_thread(&tx, t)?;
        }
        tx.commit()?;
        Ok(n > 0)
    }

    /// Port of store.ts `deleteMessagesByIds`; returns the number deleted.
    pub fn delete_messages(&mut self, ids: &[i64]) -> Result<usize> {
        let tx = crate::write_tx(&mut self.conn)?;
        let mut touched = std::collections::BTreeSet::new();
        let mut deleted = 0;
        for id in ids {
            let t: Option<i64> = tx
                .prepare_cached("SELECT thread_id FROM messages WHERE id = ?1")?
                .query_row([id], |r| r.get(0))
                .optional()?;
            if let Some(t) = t {
                deleted += tx
                    .prepare_cached("DELETE FROM messages WHERE id = ?1")?
                    .execute([id])?;
                tx.prepare_cached("DELETE FROM reminders_done WHERE msg_id = ?1")?
                    .execute([id])?;
                touched.insert(t);
            }
        }
        for t in touched {
            recompute_thread(&tx, t)?;
        }
        tx.commit()?;
        Ok(deleted)
    }
}
