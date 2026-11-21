//! Message reads: keyset message paging, OTP items, reminders, OTP purge,
//! weekly scan over the stored history.

use rusqlite::{OptionalExtension, Row, params};
use serde::{Deserialize, Serialize};
use sms_engine::{
    cleanup::{WeeklyScanReport, run_weekly_useless_scan},
    inbox::{ActiveOtpItem, ReminderItem},
};
use sms_types::{ExtractedOtp, ExtractedReminder, ParsedSms, RawSms};

use crate::{
    Result, Store,
    threads::{classification_from, recompute_thread},
};

/// Keyset cursor for messages: `(ts_ms, id)` of the last row returned.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MessageCursor {
    pub ts_ms: i64,
    pub id: i64,
}

pub(crate) const RAW_COLS: &str =
    "id, thread_id, sender, contact_name, body, ts_ms, is_known_contact,
    is_outgoing, sim_slot, is_read, is_starred, protocol, rcs_status, rcs_card_json";
const PARSED_COLS: &str = "category, confidence, scores_json, reason, sender_kind_json, otp_json,
    txn_json, reminder_json";

fn de_enum<T: serde::de::DeserializeOwned>(s: Option<String>) -> Option<T> {
    s.and_then(|s| serde_json::from_value(serde_json::Value::String(s)).ok())
}

fn de_json<T: serde::de::DeserializeOwned>(s: Option<String>) -> rusqlite::Result<Option<T>> {
    match s {
        None => Ok(None),
        Some(s) => serde_json::from_str(&s).map(Some).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
        }),
    }
}

pub(crate) fn raw_from_row(r: &Row<'_>) -> rusqlite::Result<RawSms> {
    Ok(RawSms {
        id: r.get(0)?,
        thread_id: r.get(1)?,
        sender: r.get(2)?,
        contact_name: r.get(3)?,
        body: r.get(4)?,
        timestamp_ms: r.get(5)?,
        is_known_contact: r.get(6)?,
        is_outgoing: r.get(7)?,
        sim_slot: r.get(8)?,
        is_read: r.get(9)?,
        is_starred: r.get(10)?,
        protocol: de_enum(r.get(11)?),
        rcs_status: de_enum(r.get(12)?),
        rcs_rich_card: de_json(r.get(13)?)?,
    })
}

fn parsed_from_row(r: &Row<'_>) -> rusqlite::Result<ParsedSms> {
    let sms = raw_from_row(r)?;
    let classification = classification_from(
        &r.get::<_, String>(14)?,
        r.get(15)?,
        &r.get::<_, String>(16)?,
        &r.get::<_, String>(17)?,
    )
    .map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(14, rusqlite::types::Type::Text, Box::new(e))
    })?;
    Ok(ParsedSms {
        sms,
        sender_kind: de_json(Some(r.get(18)?))?.expect("sender kind"),
        classification,
        otp: de_json(r.get(19)?)?,
        transaction: de_json(r.get(20)?)?,
        reminder: de_json(r.get(21)?)?,
    })
}

impl Store {
    /// One keyset page of a thread's messages, newest first.
    pub fn messages_page(
        &self,
        thread_id: i64,
        before: Option<MessageCursor>,
        limit: usize,
    ) -> Result<(Vec<ParsedSms>, Option<MessageCursor>)> {
        let limit = limit.clamp(1, 500);
        let (ts, id) = before.map_or((i64::MAX, i64::MAX), |c| (c.ts_ms, c.id));
        let sql = format!(
            "SELECT {RAW_COLS}, {PARSED_COLS} FROM messages
             WHERE thread_id = ?1 AND (ts_ms < ?2 OR (ts_ms = ?2 AND id < ?3))
             ORDER BY ts_ms DESC, id DESC LIMIT ?4"
        );
        let mut stmt = self.conn.prepare_cached(&sql)?;
        let mut rows: Vec<ParsedSms> = stmt
            .query_map(
                params![thread_id, ts, id, (limit + 1) as i64],
                parsed_from_row,
            )?
            .collect::<rusqlite::Result<_>>()?;
        let next = if rows.len() > limit {
            rows.truncate(limit);
            rows.last().map(|m| MessageCursor {
                ts_ms: m.sms.timestamp_ms,
                id: m.sms.id,
            })
        } else {
            None
        };
        Ok((rows, next))
    }

    pub fn message(&self, id: i64) -> Result<Option<ParsedSms>> {
        let sql = format!("SELECT {RAW_COLS}, {PARSED_COLS} FROM messages WHERE id = ?1");
        Ok(self
            .conn
            .prepare_cached(&sql)?
            .query_row([id], parsed_from_row)
            .optional()?)
    }

    /// Every stored message in insertion order (TS array order).
    pub fn all_messages(&self) -> Result<Vec<RawSms>> {
        let sql = format!("SELECT {RAW_COLS} FROM messages ORDER BY ins_seq");
        let mut stmt = self.conn.prepare_cached(&sql)?;
        let v = stmt
            .query_map([], raw_from_row)?
            .collect::<rusqlite::Result<_>>()?;
        Ok(v)
    }

    /// Port of store.ts `collectOtpItems` (skips blocked and archived threads).
    pub fn otp_items(&self, now_ms: i64) -> Result<Vec<ActiveOtpItem>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT m.id, m.thread_id, t.display_name, m.body, m.ts_ms, m.sim_slot, m.otp_json
             FROM messages m JOIN threads t ON t.thread_id = m.thread_id
             WHERE m.otp_json IS NOT NULL AND t.is_blocked = 0 AND t.is_archived = 0
             ORDER BY m.ts_ms DESC, t.latest_ts DESC, t.thread_id DESC, m.ins_seq",
        )?;
        let items = stmt
            .query_map([], |r| {
                let otp: ExtractedOtp = de_json(Some(r.get(6)?))?.expect("otp");
                let ts: i64 = r.get(4)?;
                let expires_at_ms = sms_types::time::expiry_ms(ts, otp.ttl_seconds);
                Ok(ActiveOtpItem {
                    sms_id: r.get(0)?,
                    thread_id: r.get(1)?,
                    sender: r.get(2)?,
                    body: r.get(3)?,
                    timestamp_ms: ts,
                    sim_slot: r.get(5)?,
                    otp,
                    expires_at_ms,
                    is_expired: now_ms >= expires_at_ms,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(items)
    }

    /// Port of store.ts `collectReminders` (skips blocked threads).
    pub fn reminders(&self) -> Result<Vec<ReminderItem>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT m.id, m.thread_id, t.display_name, m.body, m.ts_ms, m.reminder_json,
                    (d.msg_id IS NOT NULL) AS done
             FROM messages m JOIN threads t ON t.thread_id = m.thread_id
             LEFT JOIN reminders_done d ON d.msg_id = m.id
             WHERE m.reminder_json IS NOT NULL AND t.is_blocked = 0
             ORDER BY done, m.ts_ms DESC, t.latest_ts DESC, t.thread_id DESC, m.ins_seq",
        )?;
        let items = stmt
            .query_map([], |r| {
                let reminder: ExtractedReminder = de_json(Some(r.get(5)?))?.expect("reminder");
                Ok(ReminderItem {
                    sms_id: r.get(0)?,
                    thread_id: r.get(1)?,
                    sender: r.get(2)?,
                    body: r.get(3)?,
                    timestamp_ms: r.get(4)?,
                    reminder,
                    is_done: r.get(6)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(items)
    }

    pub fn set_reminder_done(&mut self, sms_id: i64, done: bool) -> Result<()> {
        if done {
            self.conn.execute(
                "INSERT OR IGNORE INTO reminders_done(msg_id) VALUES (?1)",
                [sms_id],
            )?;
        } else {
            self.conn
                .execute("DELETE FROM reminders_done WHERE msg_id = ?1", [sms_id])?;
        }
        Ok(())
    }

    /// Port of store.ts `purgeExpiredOtpMessages`; returns purged message ids.
    pub fn purge_expired_otps(&mut self, now_ms: i64) -> Result<Vec<i64>> {
        let tx = crate::write_tx(&mut self.conn)?;
        let candidates: Vec<(i64, i64, i64, String)> = tx
            .prepare(
                "SELECT id, thread_id, ts_ms, otp_json FROM messages
                 WHERE otp_json IS NOT NULL AND txn_json IS NULL AND reminder_json IS NULL
                   AND COALESCE(is_starred, 0) = 0
                 ORDER BY ins_seq",
            )?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
            .collect::<rusqlite::Result<_>>()?;
        let mut purged = Vec::new();
        let mut touched = std::collections::BTreeSet::new();
        for (id, thread, ts, otp_json) in candidates {
            let otp: ExtractedOtp = serde_json::from_str(&otp_json)?;
            if now_ms >= sms_types::time::expiry_ms(ts, otp.ttl_seconds) {
                tx.execute("DELETE FROM messages WHERE id = ?1", [id])?;
                purged.push(id);
                touched.insert(thread);
            }
        }
        for t in touched {
            recompute_thread(&tx, t)?;
        }
        tx.commit()?;
        Ok(purged)
    }

    /// Weekly useless-SMS scan over the whole stored history (cleanup.ts rules).
    pub fn weekly_scan(&self, now_ms: i64) -> Result<WeeklyScanReport> {
        let messages = self.all_messages()?;
        let blocked = self.blocked_senders()?;
        Ok(run_weekly_useless_scan(
            &messages,
            &blocked,
            now_ms,
            &self.engine,
        ))
    }
}
