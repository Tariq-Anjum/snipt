//! MMS receive (S15) on the core: PDU parsing (`sms-mms`), download
//! bookkeeping and ingest into the store. Used by the Android JNI path
//! (`jni_bridge`) and by tests; the UI only reads attachments
//! ([`Core::mms_image`]).
//!
//! Flow (Android): WAP push -> [`Core::mms_notification`] -> platform
//! download -> [`Core::mms_retrieved`] (parts for the Telephony provider
//! write) -> [`Core::mms_ingest`] (store + notification) -> thumbnails
//! ([`Core::mms_set_thumb`]) -> M-NotifyResp.ind ([`Core::mms_notify_resp`]).

use base64::Engine as _;
use serde::{Deserialize, Serialize};
use sms_mms::{NotifyStatus, Pdu, RetrieveConf};
use sms_store::{AttachmentIn, FailOutcome, IncomingSms, MmsNotice, NoticeAction, ProviderRef};

use super::{Core, IncomingResult, MAX_BODY_CHARS, MAX_SENDER_CHARS, now_ms};
use crate::error::{CmdError, CmdResult};

/// Body of the message stored when a download is given up.
pub const PLACEHOLDER_BODY: &str = "Multimedia message could not be downloaded.";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MmsNoticeOut {
    /// `download` or `duplicate`.
    pub action: NoticeAction,
    pub pending_id: i64,
    pub location: String,
    pub sub_id: Option<i64>,
}

/// One part as the Telephony provider needs it. `offset`/`len` index the
/// PDU the caller already holds (no copies cross JNI).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderPart {
    pub seq: u32,
    pub content_type: String,
    pub name: Option<String>,
    pub charset: Option<u32>,
    pub content_id: Option<String>,
    pub content_location: Option<String>,
    /// Decoded text of text/plain parts (provider `text` column).
    pub text: Option<String>,
    pub offset: usize,
    pub len: usize,
}

/// A retrieved message's headers and parts for the provider write.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MmsRetrieved {
    pub from: String,
    pub to: Vec<String>,
    pub subject: Option<String>,
    /// Seconds since epoch (sender's Date header), if present.
    pub date_sent: Option<u64>,
    pub received_ms: i64,
    pub message_id: Option<String>,
    pub transaction_id: Option<String>,
    pub content_type: String,
    pub mms_version: u8,
    pub sub_id: Option<i64>,
    pub parts: Vec<ProviderPart>,
}

/// An image part stored in the core, with its PDU range for thumbnailing.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ThumbJob {
    pub part_id: i64,
    pub offset: usize,
    pub len: usize,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MmsIngestOut {
    pub result: IncomingResult,
    pub thumbs: Vec<ThumbJob>,
}

/// Image bytes for the UI (base64; the UI turns it into a `blob:` URL).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MmsImage {
    pub content_type: String,
    pub data_b64: String,
}

fn parse_retrieve(pdu: &[u8]) -> CmdResult<RetrieveConf> {
    match sms_mms::parse(pdu) {
        Ok(Pdu::RetrieveConf(m)) if m.retrieve_ok() => Ok(m),
        Ok(Pdu::RetrieveConf(_)) => Err(CmdError::invalid("MMSC returned an error status")),
        Ok(_) => Err(CmdError::invalid("not an M-Retrieve.conf")),
        Err(_) => Err(CmdError::invalid("malformed MMS")),
    }
}

/// Display parts: everything except SMIL presentation parts, in order.
fn display_parts(m: &RetrieveConf) -> impl Iterator<Item = (u32, &sms_mms::Part)> {
    m.parts
        .iter()
        .enumerate()
        .filter(|(_, p)| !p.is_smil())
        .map(|(i, p)| (i as u32, p))
}

fn truncate(s: &str, max: usize) -> String {
    sms_mms::text::truncate_chars(s, max)
}

/// Message body: text parts joined by newlines (classified like an SMS);
/// else the subject; else a short label so previews and notifications are
/// never blank.
fn body_of(m: &RetrieveConf, pdu: &[u8]) -> String {
    let text: Vec<String> = display_parts(m)
        .filter(|(_, p)| p.is_text())
        .map(|(_, p)| p.text(pdu).trim().to_string())
        .filter(|t| !t.is_empty())
        .collect();
    let body = text.join("\n");
    let body = if !body.is_empty() {
        body
    } else if let Some(s) = m.subject.as_deref().filter(|s| !s.trim().is_empty()) {
        s.trim().to_string()
    } else if display_parts(m).any(|(_, p)| sms_mms::sniff_image(p.data(pdu)).is_some()) {
        "[Photo]".to_string()
    } else {
        "[Attachment]".to_string()
    };
    truncate(&body, MAX_BODY_CHARS)
}

impl Core {
    /// A WAP push (`WAP_PUSH_DELIVER` `data` extra = M-Notification.ind).
    pub fn mms_notification(
        &mut self,
        pdu: &[u8],
        sub_id: Option<i64>,
        sim_slot: u8,
    ) -> CmdResult<MmsNoticeOut> {
        let n = match sms_mms::parse(pdu) {
            Ok(Pdu::Notification(n)) => n,
            Ok(_) => return Err(CmdError::invalid("not an M-Notification.ind")),
            Err(_) => return Err(CmdError::invalid("malformed MMS notification")),
        };
        let received = now_ms();
        let (p, action) = self.store.mms_notice(&MmsNotice {
            transaction_id: n.transaction_id,
            location: n.content_location,
            sender: n.from.map(|f| truncate(&f, MAX_SENDER_CHARS)),
            sub_id,
            sim_slot: sim_slot.min(1),
            received_ms: received,
            expiry_ms: n.expiry.map(|e| e.at_ms(received)),
        })?;
        Ok(MmsNoticeOut {
            action,
            pending_id: p.pending_id,
            location: p.location,
            sub_id: p.sub_id,
        })
    }

    /// Count a download attempt; fails for a transaction that is not pending.
    pub fn mms_attempt(&mut self, pending_id: i64) -> CmdResult<i64> {
        Ok(self.store.mms_attempt(pending_id, now_ms())?)
    }

    /// Pending downloads worth retrying now.
    pub fn mms_retryable(&self) -> CmdResult<Vec<MmsNoticeOut>> {
        Ok(self
            .store
            .mms_retryable(now_ms())?
            .into_iter()
            .map(|p| MmsNoticeOut {
                action: NoticeAction::Download,
                pending_id: p.pending_id,
                location: p.location,
                sub_id: p.sub_id,
            })
            .collect())
    }

    /// Parse a downloaded M-Retrieve.conf for the provider write. Refuses a
    /// transaction that is no longer pending (no second provider row).
    pub fn mms_retrieved(&self, pending_id: i64, pdu: &[u8]) -> CmdResult<MmsRetrieved> {
        let p = self
            .store
            .mms_pending(pending_id)?
            .ok_or_else(|| CmdError::invalid("unknown MMS"))?;
        if p.state != "pending" {
            return Err(CmdError::invalid("MMS already handled"));
        }
        let m = parse_retrieve(pdu)?;
        let from = m
            .from
            .clone()
            .or(p.sender)
            .unwrap_or_else(|| "Unknown".into());
        let parts = m
            .parts
            .iter()
            .enumerate()
            .map(|(i, part)| ProviderPart {
                seq: i as u32,
                content_type: part.content_type.media.clone(),
                name: part.name.clone(),
                charset: part.content_type.charset,
                content_id: part.content_id.clone(),
                content_location: part.content_location.clone(),
                text: part.is_text().then(|| part.text(pdu)),
                offset: part.offset,
                len: part.len,
            })
            .collect();
        Ok(MmsRetrieved {
            from: truncate(&from, MAX_SENDER_CHARS),
            to: m.to.clone(),
            subject: m.subject.clone(),
            date_sent: m.date,
            received_ms: p.received_ms,
            message_id: m.message_id.clone(),
            transaction_id: m.transaction_id.clone(),
            content_type: m.content_type.media.clone(),
            mms_version: m.version,
            sub_id: p.sub_id,
            parts,
        })
    }

    /// Store a downloaded MMS (after the provider row `mms_id` was written;
    /// `None` if the provider write failed: the message is still kept).
    pub fn mms_ingest(
        &mut self,
        pending_id: i64,
        pdu: &[u8],
        mms_id: Option<i64>,
    ) -> CmdResult<MmsIngestOut> {
        let p = self
            .store
            .mms_pending(pending_id)?
            .ok_or_else(|| CmdError::invalid("unknown MMS"))?;
        let m = parse_retrieve(pdu)?;
        let sender = m
            .from
            .clone()
            .or(p.sender.clone())
            .unwrap_or_else(|| "Unknown".into());
        let input = IncomingSms {
            sender: truncate(&sender, MAX_SENDER_CHARS),
            contact_name: None,
            body: body_of(&m, pdu),
            is_known_contact: false,
            sim_slot: p.sim_slot.min(1),
            protocol: None,
            timestamp_ms: Some(p.received_ms),
        };
        let mut ranges = Vec::new();
        let attachments: Vec<AttachmentIn> = display_parts(&m)
            .filter(|(_, part)| !part.is_text())
            .map(|(seq, part)| {
                let data = part.data(pdu);
                ranges.push((seq, part.offset, part.len));
                let image_type = sms_mms::sniff_image(data);
                AttachmentIn {
                    seq,
                    content_type: part.content_type.media.clone(),
                    name: part.name.clone(),
                    image_type,
                    // The store keeps bytes only for displayable images; the
                    // rest is recorded with its size (the provider keeps it).
                    data: data.to_vec(),
                }
            })
            .collect();
        let (raw, inserted, images) =
            self.store
                .ingest_mms(&input, mms_id, &attachments, Some(pending_id), now_ms())?;
        let thumbs = images
            .iter()
            .filter_map(|img| {
                ranges
                    .iter()
                    .find(|(s, _, _)| *s == img.seq)
                    .map(|&(_, offset, len)| ThumbJob {
                        part_id: img.id,
                        offset,
                        len,
                    })
            })
            .collect();
        Ok(MmsIngestOut {
            result: self.incoming_result(raw.id, raw.thread_id, inserted)?,
            thumbs,
        })
    }

    /// A download failed. `Some(result)` when it was given up: a placeholder
    /// message is stored (so the MMS is never silently lost) and should be
    /// notified.
    pub fn mms_failed(
        &mut self,
        pending_id: i64,
    ) -> CmdResult<(FailOutcome, Option<IncomingResult>)> {
        let outcome = self.store.mms_fail(pending_id, now_ms())?;
        if outcome != FailOutcome::GaveUp {
            return Ok((outcome, None));
        }
        Ok((outcome, Some(self.mms_placeholder(pending_id)?)))
    }

    fn mms_placeholder(&mut self, pending_id: i64) -> CmdResult<IncomingResult> {
        let p = self
            .store
            .mms_pending(pending_id)?
            .ok_or_else(|| CmdError::invalid("unknown MMS"))?;
        let input = IncomingSms {
            sender: p.sender.unwrap_or_else(|| "Unknown".into()),
            contact_name: None,
            body: PLACEHOLDER_BODY.into(),
            is_known_contact: false,
            sim_slot: p.sim_slot.min(1),
            protocol: None,
            timestamp_ms: Some(p.received_ms),
        };
        let (raw, inserted, _) =
            self.store
                .ingest_mms(&input, None, &[], Some(pending_id), now_ms())?;
        self.incoming_result(raw.id, raw.thread_id, inserted)
    }

    /// Placeholders for downloads that expired or ran out of attempts while
    /// no callback arrived (checked on app start).
    pub fn mms_give_up_exhausted(&mut self) -> CmdResult<Vec<IncomingResult>> {
        let ids: Vec<i64> = self
            .store
            .mms_exhausted(now_ms())?
            .into_iter()
            .map(|p| p.pending_id)
            .collect();
        ids.into_iter().map(|id| self.mms_placeholder(id)).collect()
    }

    /// Thumbnail (JPEG, made by the platform) for an image part.
    pub fn mms_set_thumb(&mut self, part_id: i64, jpeg: &[u8]) -> CmdResult<bool> {
        if sms_mms::sniff_image(jpeg) != Some("image/jpeg") {
            return Err(CmdError::invalid("thumbnail must be JPEG"));
        }
        Ok(self.store.mms_set_thumb(part_id, jpeg)?)
    }

    /// M-NotifyResp.ind (status Retrieved) for a completed download.
    pub fn mms_notify_resp(&self, pending_id: i64) -> CmdResult<Vec<u8>> {
        let p = self
            .store
            .mms_pending(pending_id)?
            .ok_or_else(|| CmdError::invalid("unknown MMS"))?;
        sms_mms::encode_notify_resp(&p.transaction_id, NotifyStatus::Retrieved)
            .map_err(|_| CmdError::invalid("transaction id"))
    }

    /// Image bytes of an attachment for the UI (only allow-listed image
    /// types; never a path or URI).
    pub fn mms_image(&self, part_id: i64, thumb: bool) -> CmdResult<MmsImage> {
        let (content_type, data) = self
            .store
            .mms_image(part_id, thumb)?
            .ok_or_else(|| CmdError::invalid("attachment not available"))?;
        Ok(MmsImage {
            content_type,
            data_b64: base64::engine::general_purpose::STANDARD.encode(data),
        })
    }

    pub fn mms_refs(&self, ids: &[i64]) -> CmdResult<Vec<ProviderRef>> {
        if ids.len() > super::MAX_IDS {
            return Err(CmdError::invalid("too many ids"));
        }
        Ok(self.store.mms_refs(ids)?)
    }
}
