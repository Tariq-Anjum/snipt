//! MMS receive (S15): download bookkeeping (`mms_pending`), ingest of a
//! retrieved message with its attachments (`mms_parts`), attachment reads
//! for the UI, and the MMS provider-id namespace (`messages.mms_id`).
//!
//! The store does not parse PDUs; the platform layer (src-tauri, via
//! `sms-mms`) hands it neutral, already-validated values. Caps are enforced
//! here again so a caller bug cannot bloat the database.

use std::collections::HashMap;

use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::ingest::{IncomingSms, ProviderLink, ingest_in_tx};
use crate::platform::ProviderRef;
use crate::{Result, Store, StoreError};
use sms_types::RawSms;

/// Largest image kept in the database (bytes). Larger images are stored as
/// `unsupported` (metadata only; the provider keeps the original).
pub const MAX_IMAGE_BYTES: usize = 5 * 1024 * 1024;
/// Largest thumbnail accepted from the platform.
pub const MAX_THUMB_BYTES: usize = 256 * 1024;
/// Most attachments stored per message.
pub const MAX_ATTACHMENTS: usize = 32;
/// Total attachment bytes stored per message.
pub const MAX_MESSAGE_BYTES: usize = 8 * 1024 * 1024;
/// Download attempts before giving up (placeholder message).
pub const MAX_ATTEMPTS: i64 = 3;
/// Image types the UI may display (decided by magic bytes, never by the PDU).
pub const IMAGE_TYPES: &[&str] = &[
    "image/jpeg",
    "image/png",
    "image/gif",
    "image/webp",
    "image/bmp",
];

const MAX_LOCATION_CHARS: usize = 2048;

/// (kind, content_type, data, thumb)
type PartRow = (String, String, Option<Vec<u8>>, Option<Vec<u8>>);

/// A new M-Notification.ind.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MmsNotice {
    pub transaction_id: String,
    pub location: String,
    pub sender: Option<String>,
    pub sub_id: Option<i64>,
    pub sim_slot: u8,
    pub received_ms: i64,
    pub expiry_ms: Option<i64>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum NoticeAction {
    /// Download now (new transaction, or a re-sent push for a pending one).
    Download,
    /// Already retrieved or given up: ignore the push.
    Duplicate,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PendingMms {
    pub pending_id: i64,
    pub transaction_id: String,
    pub location: String,
    pub sender: Option<String>,
    pub sub_id: Option<i64>,
    pub sim_slot: u8,
    pub received_ms: i64,
    pub expiry_ms: Option<i64>,
    pub attempts: i64,
    pub state: String,
}

/// Outcome of a failed download attempt.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum FailOutcome {
    /// Still pending; retried on the next trigger (app start, next push).
    Retry,
    /// Attempts exhausted or expired: state `failed`.
    GaveUp,
    /// Unknown or already finished transaction.
    Ignored,
}

/// One non-text part handed to the store.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttachmentIn {
    pub seq: u32,
    /// Declared (PDU) content type, lower case; display only.
    pub content_type: String,
    pub name: Option<String>,
    /// `Some(mime)` when the data is a displayable image by magic bytes.
    pub image_type: Option<&'static str>,
    pub data: Vec<u8>,
}

/// Attachment metadata for the thread view (no bytes).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentMeta {
    pub id: i64,
    /// `image` or `unsupported`.
    pub kind: String,
    pub content_type: String,
    pub name: Option<String>,
    pub size: i64,
    pub has_thumb: bool,
}

/// A stored image attachment (for the platform's thumbnailer).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StoredImage {
    pub id: i64,
    pub seq: u32,
}

fn row_pending(r: &rusqlite::Row<'_>) -> rusqlite::Result<PendingMms> {
    Ok(PendingMms {
        pending_id: r.get(0)?,
        transaction_id: r.get(1)?,
        location: r.get(2)?,
        sender: r.get(3)?,
        sub_id: r.get(4)?,
        sim_slot: r.get(5)?,
        received_ms: r.get(6)?,
        expiry_ms: r.get(7)?,
        attempts: r.get(8)?,
        state: r.get(9)?,
    })
}

const PENDING_COLS: &str = "pending_id, transaction_id, location, sender, sub_id, sim_slot,
    received_ms, expiry_ms, attempts, state";

fn clean(s: &str, max: usize) -> String {
    s.chars().filter(|c| !c.is_control()).take(max).collect()
}

impl Store {
    /// Record a WAP-push notification. A transaction id seen before is not
    /// recorded twice: still `pending` → download again (the network
    /// re-sent it); `done`/`failed` → duplicate.
    pub fn mms_notice(&mut self, n: &MmsNotice) -> Result<(PendingMms, NoticeAction)> {
        let tid = clean(n.transaction_id.trim(), 256);
        let loc = n.location.trim();
        if tid.is_empty() {
            return Err(StoreError::Invalid("empty transaction id"));
        }
        if loc.is_empty() || loc.chars().count() > MAX_LOCATION_CHARS || loc.contains('\0') {
            return Err(StoreError::Invalid("bad content location"));
        }
        if n.sim_slot > 1 {
            return Err(StoreError::Invalid("simSlot must be 0 or 1"));
        }
        let tx = crate::write_tx(&mut self.conn)?;
        tx.prepare_cached(
            "INSERT OR IGNORE INTO mms_pending(transaction_id, location, sender, sub_id, sim_slot,
                received_ms, expiry_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )?
        .execute(params![
            tid,
            loc,
            n.sender.as_deref().map(|s| clean(s, 128)),
            n.sub_id,
            n.sim_slot,
            n.received_ms,
            n.expiry_ms
        ])?;
        let p = tx
            .prepare_cached(&format!(
                "SELECT {PENDING_COLS} FROM mms_pending WHERE transaction_id = ?1"
            ))?
            .query_row([&tid], row_pending)?;
        tx.commit()?;
        let action = if p.state == "pending" {
            NoticeAction::Download
        } else {
            NoticeAction::Duplicate
        };
        Ok((p, action))
    }

    pub fn mms_pending(&self, pending_id: i64) -> Result<Option<PendingMms>> {
        Ok(self
            .conn
            .prepare_cached(&format!(
                "SELECT {PENDING_COLS} FROM mms_pending WHERE pending_id = ?1"
            ))?
            .query_row([pending_id], row_pending)
            .optional()?)
    }

    /// Count one download attempt (before handing it to the platform).
    pub fn mms_attempt(&mut self, pending_id: i64) -> Result<i64> {
        let tx = crate::write_tx(&mut self.conn)?;
        let n = tx.execute(
            "UPDATE mms_pending SET attempts = attempts + 1
             WHERE pending_id = ?1 AND state = 'pending'",
            [pending_id],
        )?;
        if n == 0 {
            return Err(StoreError::Invalid("not a pending MMS"));
        }
        let a: i64 = tx.query_row(
            "SELECT attempts FROM mms_pending WHERE pending_id = ?1",
            [pending_id],
            |r| r.get(0),
        )?;
        tx.commit()?;
        Ok(a)
    }

    /// Pending downloads that may be retried now (not expired, attempts
    /// left). Expired ones are returned by [`Store::mms_fail`] callers via
    /// `GaveUp`; here they are simply skipped.
    pub fn mms_retryable(&self, now_ms: i64) -> Result<Vec<PendingMms>> {
        let mut st = self.conn.prepare_cached(&format!(
            "SELECT {PENDING_COLS} FROM mms_pending
             WHERE state = 'pending' AND attempts < ?1
               AND (expiry_ms IS NULL OR expiry_ms > ?2)
             ORDER BY pending_id LIMIT 20"
        ))?;
        let v = st
            .query_map(params![MAX_ATTEMPTS, now_ms], row_pending)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(v)
    }

    /// Pending downloads that can never succeed any more (expired or out
    /// of attempts) but have not been turned into a placeholder yet.
    pub fn mms_exhausted(&self, now_ms: i64) -> Result<Vec<PendingMms>> {
        let mut st = self.conn.prepare_cached(&format!(
            "SELECT {PENDING_COLS} FROM mms_pending
             WHERE state = 'pending' AND (attempts >= ?1
               OR (expiry_ms IS NOT NULL AND expiry_ms <= ?2))
             ORDER BY pending_id LIMIT 20"
        ))?;
        let v = st
            .query_map(params![MAX_ATTEMPTS, now_ms], row_pending)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(v)
    }

    /// A download attempt failed.
    pub fn mms_fail(&mut self, pending_id: i64, now_ms: i64) -> Result<FailOutcome> {
        let Some(p) = self.mms_pending(pending_id)? else {
            return Ok(FailOutcome::Ignored);
        };
        if p.state != "pending" {
            return Ok(FailOutcome::Ignored);
        }
        let expired = p.expiry_ms.is_some_and(|e| e <= now_ms);
        Ok(if expired || p.attempts >= MAX_ATTEMPTS {
            FailOutcome::GaveUp
        } else {
            FailOutcome::Retry
        })
    }

    /// Store a retrieved MMS: the message (text body classified like an SMS,
    /// `mms_id` = provider row) plus its attachments, and close the pending
    /// transaction. Idempotent per `mms_id`: a second call returns the stored
    /// message with `false` and adds nothing.
    pub fn ingest_mms(
        &mut self,
        input: &IncomingSms,
        mms_id: Option<i64>,
        attachments: &[AttachmentIn],
        pending_id: Option<i64>,
        now_ms: i64,
    ) -> Result<(RawSms, bool, Vec<StoredImage>)> {
        let link = mms_id.map_or(ProviderLink::None, ProviderLink::Mms);
        let tx = crate::write_tx(&mut self.conn)?;
        if let Some(pid) = pending_id {
            let state: Option<(String, Option<i64>)> = tx
                .prepare_cached("SELECT state, msg_id FROM mms_pending WHERE pending_id = ?1")?
                .query_row([pid], |r| Ok((r.get(0)?, r.get(1)?)))
                .optional()?;
            match state {
                None => return Err(StoreError::Invalid("unknown pending MMS")),
                Some((s, Some(msg))) if s != "pending" => {
                    let sql = format!(
                        "SELECT {} FROM messages WHERE id = ?1",
                        crate::query::RAW_COLS
                    );
                    if let Some(raw) = tx
                        .prepare_cached(&sql)?
                        .query_row([msg], crate::query::raw_from_row)
                        .optional()?
                    {
                        return Ok((raw, false, Vec::new()));
                    }
                    return Err(StoreError::Invalid("MMS already handled"));
                }
                Some((s, None)) if s != "pending" => {
                    return Err(StoreError::Invalid("MMS already handled"));
                }
                _ => {}
            }
        }
        let (raw, inserted) = ingest_in_tx(&tx, &self.engine, input, link, now_ms)?;
        let mut images = Vec::new();
        if inserted {
            let mut total = 0usize;
            for a in attachments.iter().take(MAX_ATTACHMENTS) {
                let size = a.data.len();
                let keep = a.image_type.is_some()
                    && size <= MAX_IMAGE_BYTES
                    && total.saturating_add(size) <= MAX_MESSAGE_BYTES;
                let (kind, ct, data) = if keep {
                    total += size;
                    (
                        "image",
                        a.image_type.unwrap_or("image/jpeg"),
                        Some(&a.data[..]),
                    )
                } else {
                    ("unsupported", a.content_type.as_str(), None)
                };
                tx.prepare_cached(
                    "INSERT INTO mms_parts(msg_id, seq, kind, content_type, name, size, data)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                )?
                .execute(params![
                    raw.id,
                    a.seq,
                    kind,
                    clean(ct, 128),
                    a.name.as_deref().map(|n| clean(n, 128)),
                    size as i64,
                    data
                ])?;
                if keep {
                    images.push(StoredImage {
                        id: tx.last_insert_rowid(),
                        seq: a.seq,
                    });
                }
            }
        }
        if let Some(pid) = pending_id {
            tx.execute(
                "UPDATE mms_pending SET state = ?2, msg_id = ?3 WHERE pending_id = ?1",
                params![
                    pid,
                    if mms_id.is_some() { "done" } else { "failed" },
                    raw.id
                ],
            )?;
        }
        tx.commit()?;
        Ok((raw, inserted, images))
    }

    /// Attach a downscaled thumbnail (platform-made) to an image part.
    pub fn mms_set_thumb(&mut self, part_id: i64, thumb: &[u8]) -> Result<bool> {
        if thumb.is_empty() || thumb.len() > MAX_THUMB_BYTES {
            return Err(StoreError::Invalid("thumbnail size"));
        }
        let n = self.conn.execute(
            "UPDATE mms_parts SET thumb = ?2 WHERE part_id = ?1 AND kind = 'image'",
            params![part_id, thumb],
        )?;
        Ok(n > 0)
    }

    /// Attachment metadata for the given messages (no bytes).
    pub fn mms_attachments(&self, msg_ids: &[i64]) -> Result<HashMap<i64, Vec<AttachmentMeta>>> {
        let mut st = self.conn.prepare_cached(
            "SELECT part_id, kind, content_type, name, size, thumb IS NOT NULL
             FROM mms_parts WHERE msg_id = ?1 ORDER BY seq",
        )?;
        let mut out = HashMap::new();
        for id in msg_ids {
            let v = st
                .query_map([id], |r| {
                    Ok(AttachmentMeta {
                        id: r.get(0)?,
                        kind: r.get(1)?,
                        content_type: r.get(2)?,
                        name: r.get(3)?,
                        size: r.get(4)?,
                        has_thumb: r.get(5)?,
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            if !v.is_empty() {
                out.insert(*id, v);
            }
        }
        Ok(out)
    }

    /// Bytes of an image attachment (`thumb`: the downscaled copy, falling
    /// back to the original when none exists). Only `image` parts with an
    /// allow-listed type are ever returned.
    pub fn mms_image(&self, part_id: i64, thumb: bool) -> Result<Option<(String, Vec<u8>)>> {
        let row: Option<PartRow> = self
            .conn
            .prepare_cached(
                "SELECT kind, content_type, data, thumb FROM mms_parts WHERE part_id = ?1",
            )?
            .query_row([part_id], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
            })
            .optional()?;
        let Some((kind, ct, data, th)) = row else {
            return Ok(None);
        };
        if kind != "image" || !IMAGE_TYPES.contains(&ct.as_str()) {
            return Ok(None);
        }
        Ok(match (thumb, th, data) {
            // Thumbnails are JPEG (made by the platform).
            (true, Some(t), _) => Some(("image/jpeg".to_string(), t)),
            (_, _, Some(d)) => Some((ct, d)),
            _ => None,
        })
    }

    /// `(id, mms_id)` for the given store ids (rows without one skipped).
    pub fn mms_refs(&self, ids: &[i64]) -> Result<Vec<ProviderRef>> {
        let mut st = self
            .conn
            .prepare_cached("SELECT mms_id FROM messages WHERE id = ?1")?;
        let mut out = Vec::new();
        for id in ids {
            let p: Option<Option<i64>> = st.query_row([id], |r| r.get(0)).optional()?;
            if let Some(pid) = p.flatten() {
                out.push(ProviderRef {
                    id: *id,
                    provider_id: pid,
                });
            }
        }
        Ok(out)
    }
}
