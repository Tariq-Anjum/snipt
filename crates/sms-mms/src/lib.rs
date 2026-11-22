//! MMS PDU decoder for Snipt (S15, receive only).
//!
//! Sources: OMA-TS-MMS_ENC-V1_3-20110913-A (§6.2 M-Notification.ind /
//! M-NotifyResp.ind, §6.3 M-Retrieve.conf, §7 binary encoding, Table 25 field
//! assignments) and WAP-230-WSP-20010705-a (8.4.2 primitive encodings,
//! 8.4.2.24 Content-type-value, 8.5 multipart, Table 38 parameters).
//!
//! Safety rules: every read is bounds-checked ([`reader::Reader`]); no
//! `unsafe`, no indexing that can panic, no recursion beyond
//! [`MAX_DEPTH`]; inputs larger than [`MAX_PDU_BYTES`] are refused and at most
//! [`MAX_PARTS`] parts are returned. Parsed parts carry *ranges* into the
//! caller's buffer, never copies, so a hostile size field cannot allocate.

#![forbid(unsafe_code)]

pub mod reader;
pub mod text;
pub mod wsp;

#[cfg(any(test, feature = "fixture"))]
pub mod fixture;

use reader::Reader;
pub use wsp::ContentType;

/// Largest PDU accepted (the platform MmsService caps PDU files at 8 MiB,
/// `MAX_MMS_FILE_SIZE` in packages/services/Mms).
pub const MAX_PDU_BYTES: usize = 8 * 1024 * 1024;
/// Most parts returned from one message (extra parts are dropped and
/// [`RetrieveConf::truncated`] is set).
pub const MAX_PARTS: usize = 32;
/// Nesting depth of multipart bodies (related → alternative is depth 2).
pub const MAX_DEPTH: usize = 2;
/// Most header fields read from one PDU.
pub const MAX_HEADERS: usize = 128;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// Input ended inside a field.
    Truncated,
    /// Input violates the encoding rules.
    Malformed(&'static str),
    /// Input exceeds [`MAX_PDU_BYTES`].
    TooLarge,
    /// A mandatory header is missing.
    Missing(&'static str),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Truncated => write!(f, "truncated PDU"),
            Error::Malformed(m) => write!(f, "malformed PDU: {m}"),
            Error::TooLarge => write!(f, "PDU too large"),
            Error::Missing(m) => write!(f, "missing header: {m}"),
        }
    }
}

impl std::error::Error for Error {}

/// X-Mms-Message-Type values (OMA-MMS-ENC §7.3.30).
pub mod message_type {
    pub const SEND_REQ: u8 = 128;
    pub const NOTIFICATION_IND: u8 = 130;
    pub const NOTIFYRESP_IND: u8 = 131;
    pub const RETRIEVE_CONF: u8 = 132;
}

/// X-Mms-Status values (§7.3.54) used in M-NotifyResp.ind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotifyStatus {
    Expired = 128,
    Retrieved = 129,
    Rejected = 130,
    Deferred = 131,
    Unrecognised = 132,
}

/// X-Mms-Expiry (§7.3.20).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Expiry {
    /// Seconds since 1970-01-01 UTC.
    Absolute(u64),
    /// Seconds after the notification was received.
    Relative(u64),
}

impl Expiry {
    /// Expiry instant in ms, given when the notification arrived.
    pub fn at_ms(self, received_ms: i64) -> i64 {
        let secs = |s: u64| i64::try_from(s).unwrap_or(i64::MAX).saturating_mul(1000);
        match self {
            Expiry::Absolute(s) => secs(s),
            Expiry::Relative(s) => received_ms.saturating_add(secs(s)),
        }
    }
}

/// M-Notification.ind (§6.2, Table 3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationInd {
    pub transaction_id: String,
    pub version: u8,
    pub from: Option<String>,
    pub subject: Option<String>,
    pub message_size: Option<u64>,
    pub expiry: Option<Expiry>,
    pub content_location: String,
}

/// One displayable part of a retrieved message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Part {
    pub content_type: ContentType,
    pub content_id: Option<String>,
    pub content_location: Option<String>,
    /// Display name: Content-Type name, Content-Disposition filename or
    /// Content-Location (sanitised, ≤ 128 chars).
    pub name: Option<String>,
    /// Absolute byte range of the data within the PDU.
    pub offset: usize,
    pub len: usize,
}

impl Part {
    pub fn data<'a>(&self, pdu: &'a [u8]) -> &'a [u8] {
        pdu.get(self.offset..self.offset.saturating_add(self.len))
            .unwrap_or(&[])
    }

    pub fn is_text(&self) -> bool {
        self.content_type.media == "text/plain"
    }

    pub fn is_smil(&self) -> bool {
        self.content_type.media == "application/smil"
    }

    /// Decoded text of a text/plain part.
    pub fn text(&self, pdu: &[u8]) -> String {
        text::decode(self.data(pdu), self.content_type.charset)
    }
}

/// M-Retrieve.conf (§6.3, Table 5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetrieveConf {
    pub transaction_id: Option<String>,
    pub message_id: Option<String>,
    pub version: u8,
    /// Seconds since epoch (Date header).
    pub date: Option<u64>,
    pub from: Option<String>,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub subject: Option<String>,
    /// X-Mms-Retrieve-Status (§7.3.43); `None` or 128 = Ok.
    pub retrieve_status: Option<u8>,
    pub content_type: ContentType,
    /// Leaf parts in document order (multiparts flattened; SMIL kept so the
    /// provider copy is complete; callers skip it for display).
    pub parts: Vec<Part>,
    /// More than [`MAX_PARTS`] parts were present.
    pub truncated: bool,
}

impl RetrieveConf {
    pub fn retrieve_ok(&self) -> bool {
        matches!(self.retrieve_status, None | Some(128))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pdu {
    Notification(NotificationInd),
    RetrieveConf(RetrieveConf),
    /// A well-formed header with another message type (ignored by Snipt).
    Other(u8),
}

/// Encoded-string-value: Text-string | Value-length Char-set Text-string.
fn encoded_string(r: &mut Reader<'_>) -> Result<String, Error> {
    let b = r.peek()?;
    let s = if b <= 31 {
        let n = r.value_length()?;
        let mut inner = r.sub(n)?;
        let cs = inner.integer()?;
        let raw = inner.text()?;
        text::decode(raw, u32::try_from(cs).ok())
    } else {
        text::decode(r.text()?, None)
    };
    Ok(text::truncate_chars(&text::strip_controls(&s), 1024))
}

fn plain_text(r: &mut Reader<'_>) -> Result<String, Error> {
    Ok(text::truncate_chars(
        &text::strip_controls(&String::from_utf8_lossy(r.text()?)),
        reader::MAX_TEXT,
    ))
}

/// From-value: Value-length (Address-present-token Encoded-string | Insert-address-token).
fn from_value(r: &mut Reader<'_>) -> Result<Option<String>, Error> {
    let n = r.value_length()?;
    let mut inner = r.sub(n)?;
    match inner.u8()? {
        0x80 => Ok(Some(text::normalize_address(&encoded_string(&mut inner)?))),
        _ => Ok(None),
    }
}

fn expiry_value(r: &mut Reader<'_>) -> Result<Expiry, Error> {
    let n = r.value_length()?;
    let mut inner = r.sub(n)?;
    match inner.u8()? {
        0x80 => Ok(Expiry::Absolute(inner.long_integer()?)),
        0x81 => Ok(Expiry::Relative(inner.integer()?)),
        _ => Err(Error::Malformed("expiry token")),
    }
}

#[derive(Default)]
struct Headers {
    message_type: Option<u8>,
    transaction_id: Option<String>,
    message_id: Option<String>,
    version: Option<u8>,
    date: Option<u64>,
    from: Option<String>,
    to: Vec<String>,
    cc: Vec<String>,
    subject: Option<String>,
    message_size: Option<u64>,
    expiry: Option<Expiry>,
    content_location: Option<String>,
    retrieve_status: Option<u8>,
    content_type: Option<ContentType>,
}

/// Reads header fields until the end of input or, after Content-Type (which
/// MUST be the last header when a body follows, §7 / OMA-MMS-ENC p.49),
/// returns with the reader positioned at the body.
fn headers(r: &mut Reader<'_>) -> Result<Headers, Error> {
    let mut h = Headers::default();
    let mut count = 0;
    while !r.is_empty() {
        count += 1;
        if count > MAX_HEADERS {
            return Err(Error::Malformed("too many headers"));
        }
        let b = r.u8()?;
        if b < 0x80 {
            if b < 32 {
                return Err(Error::Malformed("bad header field name"));
            }
            // Application-header: Token-text Application-specific-value.
            r.cstr()?;
            r.cstr()?;
            continue;
        }
        let field = b & 0x7F;
        // The first header MUST be X-Mms-Message-Type (§7.1/§6).
        if count == 1 && field != 0x0C {
            return Err(Error::Malformed("first header is not X-Mms-Message-Type"));
        }
        match field {
            0x0C => {
                let t = r.u8()?;
                if h.message_type.is_some() {
                    return Err(Error::Malformed("duplicate message type"));
                }
                h.message_type = Some(t);
            }
            0x18 => h.transaction_id = Some(plain_text(r)?),
            0x0B => h.message_id = Some(plain_text(r)?),
            0x0D => {
                let b = r.peek()?;
                if b >= 0x80 {
                    h.version = Some(r.u8()? & 0x7F);
                } else {
                    r.skip_value()?;
                }
            }
            0x05 => h.date = Some(r.long_integer()?),
            0x09 => h.from = from_value(r)?,
            0x17 => {
                let v = text::normalize_address(&encoded_string(r)?);
                if h.to.len() < 64 {
                    h.to.push(v);
                }
            }
            0x02 => {
                let v = text::normalize_address(&encoded_string(r)?);
                if h.cc.len() < 64 {
                    h.cc.push(v);
                }
            }
            0x16 => h.subject = Some(encoded_string(r)?),
            0x0E => h.message_size = Some(r.long_integer()?),
            0x08 => h.expiry = Some(expiry_value(r)?),
            0x03 => h.content_location = Some(plain_text(r)?),
            0x19 => h.retrieve_status = Some(r.u8()?),
            0x04 => {
                h.content_type = Some(wsp::content_type(r)?);
                return Ok(h);
            }
            _ => r.skip_value()?,
        }
    }
    Ok(h)
}

/// Leaf parts of a body, flattening nested multiparts up to [`MAX_DEPTH`].
/// multipart/alternative keeps only its preferred child (text/plain, else
/// an image, else the first).
fn collect_parts(
    pdu: &[u8],
    ct: &ContentType,
    start: usize,
    len: usize,
    depth: usize,
    out: &mut Vec<Part>,
    truncated: &mut bool,
) -> Result<(), Error> {
    let body = pdu
        .get(start..start.saturating_add(len))
        .ok_or(Error::Truncated)?;
    if !ct.is_multipart() {
        if out.len() >= MAX_PARTS {
            *truncated = true;
            return Ok(());
        }
        out.push(Part {
            content_type: ct.clone(),
            content_id: None,
            content_location: None,
            name: ct.name.clone(),
            offset: start,
            len,
        });
        return Ok(());
    }
    if depth >= MAX_DEPTH {
        return Err(Error::Malformed("multipart nested too deep"));
    }
    let (mut entries, cut) = wsp::multipart(body, start, MAX_PARTS)?;
    if cut {
        *truncated = true;
    }
    if ct.is_alternative() && entries.len() > 1 {
        let pick = entries
            .iter()
            .position(|e| e.ct.media == "text/plain")
            .or_else(|| {
                entries
                    .iter()
                    .position(|e| e.ct.media.starts_with("image/"))
            })
            .unwrap_or(0);
        entries = vec![entries.swap_remove(pick)];
    }
    for e in entries {
        if e.ct.is_multipart() {
            collect_parts(pdu, &e.ct, e.start, e.len, depth + 1, out, truncated)?;
            continue;
        }
        if out.len() >= MAX_PARTS {
            *truncated = true;
            break;
        }
        let name =
            e.ct.name
                .clone()
                .or(e.headers.filename)
                .or_else(|| e.headers.content_location.clone())
                .map(|n| text::truncate_chars(&n, 128));
        out.push(Part {
            content_type: e.ct,
            content_id: e.headers.content_id,
            content_location: e.headers.content_location,
            name,
            offset: e.start,
            len: e.len,
        });
    }
    Ok(())
}

/// Parse an MMS PDU (the WAP push `data` extra, or a downloaded file).
pub fn parse(pdu: &[u8]) -> Result<Pdu, Error> {
    if pdu.len() > MAX_PDU_BYTES {
        return Err(Error::TooLarge);
    }
    let mut r = Reader::new(pdu);
    let h = headers(&mut r)?;
    let t = h.message_type.ok_or(Error::Missing("X-Mms-Message-Type"))?;
    match t {
        message_type::NOTIFICATION_IND => {
            let location = h
                .content_location
                .filter(|s| !s.is_empty())
                .ok_or(Error::Missing("X-Mms-Content-Location"))?;
            Ok(Pdu::Notification(NotificationInd {
                transaction_id: h
                    .transaction_id
                    .ok_or(Error::Missing("X-Mms-Transaction-Id"))?,
                version: h.version.unwrap_or(0x10),
                from: h.from.filter(|s| !s.is_empty()),
                subject: h.subject.filter(|s| !s.is_empty()),
                message_size: h.message_size,
                expiry: h.expiry,
                content_location: location,
            }))
        }
        message_type::RETRIEVE_CONF => {
            let mut parts = Vec::new();
            let mut truncated = false;
            let ct = h.content_type.unwrap_or_default();
            // A Retrieve.conf carrying an error status may have no body.
            if !ct.media.is_empty() {
                let start = r.pos();
                let len = r.remaining();
                collect_parts(pdu, &ct, start, len, 0, &mut parts, &mut truncated)?;
            }
            Ok(Pdu::RetrieveConf(RetrieveConf {
                transaction_id: h.transaction_id,
                message_id: h.message_id,
                version: h.version.unwrap_or(0x10),
                date: h.date,
                from: h.from.filter(|s| !s.is_empty()),
                to: h.to,
                cc: h.cc,
                subject: h.subject.filter(|s| !s.is_empty()),
                retrieve_status: h.retrieve_status,
                content_type: ct,
                parts,
                truncated,
            }))
        }
        other => Ok(Pdu::Other(other)),
    }
}

/// Encode an M-NotifyResp.ind (§6.2, Table 4): message type, transaction id
/// and version first (in that order), then X-Mms-Status and
/// X-Mms-Report-Allowed = No (Snipt never sends delivery reports).
pub fn encode_notify_resp(transaction_id: &str, status: NotifyStatus) -> Result<Vec<u8>, Error> {
    let tid = transaction_id.as_bytes();
    if tid.is_empty() || tid.len() > reader::MAX_TEXT || tid.iter().any(|&b| b == 0 || b >= 0x80) {
        return Err(Error::Malformed("transaction id"));
    }
    let mut out = Vec::with_capacity(tid.len() + 10);
    out.extend([0x8C, message_type::NOTIFYRESP_IND]);
    out.push(0x98);
    out.extend(tid);
    out.push(0);
    out.extend([0x8D, 0x92]); // MMS 1.2: major 1 (bits 4-6), minor 2
    out.extend([0x95, status as u8]);
    out.extend([0x91, 0x81]); // Report-Allowed: No
    Ok(out)
}

/// Image formats Snipt displays, detected from the data's magic bytes (the
/// declared content type is not trusted). SVG is never accepted.
pub fn sniff_image(data: &[u8]) -> Option<&'static str> {
    match data {
        [0xFF, 0xD8, 0xFF, ..] => Some("image/jpeg"),
        [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, ..] => Some("image/png"),
        [b'G', b'I', b'F', b'8', b'7' | b'9', b'a', ..] => Some("image/gif"),
        [
            b'R',
            b'I',
            b'F',
            b'F',
            _,
            _,
            _,
            _,
            b'W',
            b'E',
            b'B',
            b'P',
            ..,
        ] => Some("image/webp"),
        [b'B', b'M', ..] if data.len() > 26 => Some("image/bmp"),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
