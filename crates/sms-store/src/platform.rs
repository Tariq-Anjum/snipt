//! Platform-facing store paths: inbox counters, the outgoing send queue and
//! provider (Android `content://sms`) synchronisation. None of this exists in
//! the TS oracle (store.ts has no send state or provider ids); outgoing rows
//! follow store.ts `appendOutgoingReply` for the parts it defines.

use std::collections::BTreeSet;

use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use sms_types::{MessageProtocol, RawSms, js, time::is_valid_timestamp_ms};

use crate::{Result, Store, StoreError, ingest::insert_message, threads::recompute_thread};

/// Prototype `MIN_HISTORY_CAPACITY`: the history size the app is built for.
pub const HISTORY_CAPACITY: i64 = 10_000;

/// Thread counts per inbox filter, with the same predicates as
/// `threads_page` / store.ts `filterThreads` (empty query).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct FilterCounts {
    #[serde(rename = "All")]
    pub all: i64,
    #[serde(rename = "Personal")]
    pub personal: i64,
    #[serde(rename = "Transactions")]
    pub transactions: i64,
    #[serde(rename = "Promotions")]
    pub promotions: i64,
    #[serde(rename = "Priority")]
    pub priority: i64,
    #[serde(rename = "Spam")]
    pub spam: i64,
    #[serde(rename = "RCS")]
    pub rcs: i64,
    #[serde(rename = "Starred")]
    pub starred: i64,
    #[serde(rename = "Archived")]
    pub archived: i64,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct InboxCounts {
    /// Every stored message (all threads, archived and spam included).
    pub total_messages: i64,
    /// Unread incoming messages in non-archived, non-Spam threads (the
    /// prototype's inbox badge).
    pub unread: i64,
    pub filters: FilterCounts,
    pub capacity: i64,
}

/// Send state of an outgoing message (`messages.send_status`).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SendStatus {
    Queued,
    Sending,
    Sent,
    Delivered,
    Failed,
}

impl SendStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            SendStatus::Queued => "queued",
            SendStatus::Sending => "sending",
            SendStatus::Sent => "sent",
            SendStatus::Delivered => "delivered",
            SendStatus::Failed => "failed",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "queued" => SendStatus::Queued,
            "sending" => SendStatus::Sending,
            "sent" => SendStatus::Sent,
            "delivered" => SendStatus::Delivered,
            "failed" => SendStatus::Failed,
            _ => return None,
        })
    }
}

/// A message to queue for sending: either a reply in an existing thread or a
/// new message to an address.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OutgoingSms {
    #[serde(default)]
    pub thread_id: Option<i64>,
    #[serde(default)]
    pub address: Option<String>,
    pub body: String,
    pub sim_slot: u8,
}

/// Store id ↔ provider id pair.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderRef {
    pub id: i64,
    pub provider_id: i64,
}

impl Store {
    pub fn inbox_counts(&self, sim: Option<u8>) -> Result<InboxCounts> {
        let (pred, args) = match sim {
            Some(s) => ("WHERE sim_slot = ?1", vec![i64::from(s)]),
            None => ("", vec![]),
        };
        let sql = format!(
            "SELECT
                COALESCE(SUM(is_archived = 0 AND category <> 'Spam'), 0),
                COALESCE(SUM(is_archived = 0 AND category = 'Personal'), 0),
                COALESCE(SUM(is_archived = 0 AND category = 'Transactions'), 0),
                COALESCE(SUM(is_archived = 0 AND category = 'Promotions'), 0),
                COALESCE(SUM(is_archived = 0 AND category = 'Priority'), 0),
                COALESCE(SUM(is_archived = 0 AND category = 'Spam'), 0),
                COALESCE(SUM(is_archived = 0 AND is_rcs = 1), 0),
                COALESCE(SUM(is_archived = 0 AND has_starred = 1), 0),
                COALESCE(SUM(is_archived = 1), 0),
                COALESCE(SUM(CASE WHEN is_archived = 0 AND category <> 'Spam'
                                  THEN unread ELSE 0 END), 0)
             FROM threads {pred}"
        );
        let (filters, unread) =
            self.conn
                .prepare_cached(&sql)?
                .query_row(rusqlite::params_from_iter(args), |r| {
                    Ok((
                        FilterCounts {
                            all: r.get(0)?,
                            personal: r.get(1)?,
                            transactions: r.get(2)?,
                            promotions: r.get(3)?,
                            priority: r.get(4)?,
                            spam: r.get(5)?,
                            rcs: r.get(6)?,
                            starred: r.get(7)?,
                            archived: r.get(8)?,
                        },
                        r.get(9)?,
                    ))
                })?;
        Ok(InboxCounts {
            total_messages: self.message_count()?,
            unread,
            filters,
            capacity: HISTORY_CAPACITY,
        })
    }

    /// Store an outgoing SMS with `send_status = 'queued'`. A reply copies
    /// sender / contact / known-contact from the thread's first message
    /// (store.ts `appendOutgoingReply`); a new address joins the thread of an
    /// earlier message from the same sender (case-insensitive) or starts one.
    /// The radio send is the platform's job; it reports back via
    /// [`Store::set_send_status`].
    pub fn queue_outgoing(&mut self, out: &OutgoingSms, now_ms: i64) -> Result<RawSms> {
        if out.sim_slot > 1 {
            return Err(StoreError::Invalid("simSlot must be 0 or 1"));
        }
        if !is_valid_timestamp_ms(now_ms) {
            return Err(StoreError::Invalid("timestampMs out of range"));
        }
        let body = js::trim(&out.body).to_string();
        if body.is_empty() {
            return Err(StoreError::Invalid("empty body"));
        }
        let tx = crate::write_tx(&mut self.conn)?;
        type Origin = (i64, String, Option<String>, bool);
        let origin: Option<Origin> = match (&out.thread_id, &out.address) {
            (Some(t), _) => {
                let o = tx
                    .prepare_cached(
                        "SELECT thread_id, sender, contact_name, is_known_contact FROM messages
                         WHERE thread_id = ?1 ORDER BY ins_seq LIMIT 1",
                    )?
                    .query_row([t], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
                    .optional()?;
                Some(o.ok_or(StoreError::Invalid("unknown thread"))?)
            }
            (None, Some(addr)) => {
                let addr = js::trim(addr).to_string();
                if addr.is_empty() {
                    return Err(StoreError::Invalid("empty address"));
                }
                let o = tx
                    .prepare_cached(
                        "SELECT thread_id, sender, contact_name, is_known_contact FROM messages
                         WHERE sender_lc = ?1 ORDER BY ins_seq LIMIT 1",
                    )?
                    .query_row([addr.to_lowercase()], |r| {
                        Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
                    })
                    .optional()?;
                match o {
                    Some(o) => Some(o),
                    None => {
                        let t: i64 = tx.query_row(
                            "SELECT MAX(COALESCE(MAX(thread_id), 100), 100) + 1 FROM messages",
                            [],
                            |r| r.get(0),
                        )?;
                        Some((t, addr, None, false))
                    }
                }
            }
            (None, None) => None,
        };
        let (thread_id, sender, contact_name, known) =
            origin.ok_or(StoreError::Invalid("threadId or address required"))?;
        let next_id: i64 = tx.query_row(
            "SELECT MAX(COALESCE(MAX(id), 1000), 1000) + 1 FROM messages",
            [],
            |r| r.get(0),
        )?;
        let sms = RawSms {
            id: next_id,
            thread_id,
            sender,
            contact_name,
            body,
            timestamp_ms: now_ms,
            is_known_contact: known,
            is_outgoing: true,
            sim_slot: out.sim_slot,
            is_read: Some(true),
            is_starred: Some(false),
            protocol: Some(MessageProtocol::Sms),
            rcs_status: None,
            rcs_rich_card: None,
        };
        let a = self.engine.analyze(&sms);
        insert_message(&tx, &sms, &a)?;
        tx.execute(
            "UPDATE messages SET send_status = ?2 WHERE id = ?1",
            params![sms.id, SendStatus::Queued.as_str()],
        )?;
        recompute_thread(&tx, thread_id)?;
        tx.commit()?;
        Ok(sms)
    }

    /// Update the send state of an outgoing message, optionally recording the
    /// provider id the platform wrote it under. Returns false when `sms_id`
    /// is not an outgoing message.
    pub fn set_send_status(
        &mut self,
        sms_id: i64,
        status: SendStatus,
        provider_id: Option<i64>,
    ) -> Result<bool> {
        let tx = crate::write_tx(&mut self.conn)?;
        let n = tx.execute(
            "UPDATE messages SET send_status = ?2 WHERE id = ?1 AND is_outgoing = 1",
            params![sms_id, status.as_str()],
        )?;
        if n > 0
            && let Some(pid) = provider_id
        {
            tx.execute(
                "UPDATE messages SET provider_id = ?2, provider_kind = 'sms' WHERE id = ?1",
                params![sms_id, pid],
            )?;
        }
        tx.commit()?;
        Ok(n > 0)
    }

    /// Send states of the given messages (only rows that have one).
    pub fn send_statuses(&self, ids: &[i64]) -> Result<Vec<(i64, SendStatus)>> {
        let mut stmt = self
            .conn
            .prepare_cached("SELECT send_status FROM messages WHERE id = ?1")?;
        let mut out = Vec::new();
        for id in ids {
            let s: Option<Option<String>> = stmt.query_row([id], |r| r.get(0)).optional()?;
            if let Some(st) = s.flatten().as_deref().and_then(SendStatus::parse) {
                out.push((*id, st));
            }
        }
        Ok(out)
    }

    /// Provider ids recorded for the given store ids (ids without one are skipped).
    pub fn provider_refs(&self, ids: &[i64]) -> Result<Vec<ProviderRef>> {
        let mut stmt = self
            .conn
            .prepare_cached("SELECT provider_id FROM messages WHERE id = ?1")?;
        let mut out = Vec::new();
        for id in ids {
            let p: Option<Option<i64>> = stmt.query_row([id], |r| r.get(0)).optional()?;
            if let Some(pid) = p.flatten() {
                out.push(ProviderRef {
                    id: *id,
                    provider_id: pid,
                });
            }
        }
        Ok(out)
    }

    fn ids_for_providers(&self, provider_ids: &[i64]) -> Result<Vec<i64>> {
        let mut stmt = self
            .conn
            .prepare_cached("SELECT id FROM messages WHERE provider_id = ?1")?;
        let mut out = Vec::new();
        for p in provider_ids {
            if let Some(id) = stmt.query_row([p], |r| r.get(0)).optional()? {
                out.push(id);
            }
        }
        Ok(out)
    }

    /// The platform reports messages read elsewhere; returns rows changed.
    pub fn provider_mark_read(&mut self, provider_ids: &[i64]) -> Result<usize> {
        let tx = crate::write_tx(&mut self.conn)?;
        let mut touched = BTreeSet::new();
        let mut changed = 0;
        for p in provider_ids {
            let row: Option<(i64, i64)> = tx
                .prepare_cached(
                    "SELECT id, thread_id FROM messages
                     WHERE provider_id = ?1 AND COALESCE(is_read, 0) = 0",
                )?
                .query_row([p], |r| Ok((r.get(0)?, r.get(1)?)))
                .optional()?;
            if let Some((id, t)) = row {
                changed += tx
                    .prepare_cached("UPDATE messages SET is_read = 1 WHERE id = ?1")?
                    .execute([id])?;
                touched.insert(t);
            }
        }
        for t in touched {
            recompute_thread(&tx, t)?;
        }
        tx.commit()?;
        Ok(changed)
    }

    /// The platform reports messages deleted elsewhere; returns rows deleted.
    pub fn provider_delete(&mut self, provider_ids: &[i64]) -> Result<usize> {
        let ids = self.ids_for_providers(provider_ids)?;
        self.delete_messages(&ids)
    }
}
