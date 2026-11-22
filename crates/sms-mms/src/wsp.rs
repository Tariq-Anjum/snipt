//! WSP Content-type-value (WAP-230-WSP 8.4.2.24) and parameters (8.4.2.4,
//! Table 38), multipart entries (8.5), and the well-known media table
//! (WINA content-type assignments, as listed in Wireshark `packet-wsp.c`).

use crate::Error;
use crate::reader::Reader;
use crate::text;

/// Well-known media (Integer-value) → name. Only the codes MMS uses or that
/// we must recognise; anything else becomes `application/octet-stream`.
fn well_known_media(code: u64) -> &'static str {
    match code {
        0x00 => "*/*",
        0x01 => "text/*",
        0x02 => "text/html",
        0x03 => "text/plain",
        0x06 => "text/x-vcalendar",
        0x07 => "text/x-vcard",
        0x0B => "multipart/*",
        0x0C => "multipart/mixed",
        0x0F => "multipart/alternative",
        0x10 => "application/*",
        0x1C => "image/*",
        0x1D => "image/gif",
        0x1E => "image/jpeg",
        0x1F => "image/tiff",
        0x20 => "image/png",
        0x21 => "image/vnd.wap.wbmp",
        0x22 => "application/vnd.wap.multipart.*",
        0x23 => "application/vnd.wap.multipart.mixed",
        0x26 => "application/vnd.wap.multipart.alternative",
        0x27 => "application/xml",
        0x28 => "text/xml",
        0x33 => "application/vnd.wap.multipart.related",
        0x3E => "application/vnd.wap.mms-message",
        _ => "application/octet-stream",
    }
}

/// A decoded Content-Type with the parameters MMS needs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ContentType {
    /// Lower-cased media type, e.g. `image/jpeg`.
    pub media: String,
    pub charset: Option<u32>,
    pub name: Option<String>,
    /// multipart/related `start` (Content-ID of the root part).
    pub start: Option<String>,
    /// multipart/related `type` (media type of the root part).
    pub related_type: Option<String>,
}

impl ContentType {
    pub fn is_multipart(&self) -> bool {
        self.media.starts_with("multipart/")
            || self.media.starts_with("application/vnd.wap.multipart.")
    }

    pub fn is_alternative(&self) -> bool {
        self.media.ends_with("multipart.alternative") || self.media == "multipart/alternative"
    }
}

fn lossy(b: &[u8]) -> String {
    text::truncate_chars(&text::strip_controls(&String::from_utf8_lossy(b)), 256)
}

fn media_name(b: &[u8]) -> String {
    let s = lossy(b).trim().to_ascii_lowercase();
    // Drop any textual parameters ("text/plain; charset=...").
    match s.split_once(';') {
        Some((m, _)) => m.trim().to_string(),
        None => s,
    }
}

/// Typed-value / Untyped-value as text (if it is text) or integer.
enum Val {
    Int(u64),
    Text(String),
    None,
}

fn read_val(r: &mut Reader<'_>) -> Result<Val, Error> {
    let b = r.peek()?;
    Ok(match b {
        0 => {
            r.u8()?;
            Val::None
        }
        1..=30 => Val::Int(r.long_integer()?),
        31 => {
            // Length-quote form: skip (not used by the parameters we read).
            let n = r.value_length()?;
            r.take(n)?;
            Val::None
        }
        32..=127 => Val::Text(lossy(r.text()?)),
        _ => Val::Int(u64::from(r.u8()? & 0x7F)),
    })
}

fn apply_param(ct: &mut ContentType, name: &str, v: Val) {
    match (name, v) {
        ("charset", Val::Int(c)) => ct.charset = u32::try_from(c).ok(),
        ("charset", Val::Text(t)) => {
            ct.charset = match t.to_ascii_lowercase().as_str() {
                "utf-8" => Some(text::charset::UTF_8),
                "us-ascii" => Some(text::charset::US_ASCII),
                "iso-8859-1" => Some(text::charset::ISO_8859_1),
                "iso-10646-ucs-2" => Some(text::charset::UCS_2),
                "utf-16" => Some(text::charset::UTF_16),
                _ => None,
            }
        }
        ("name", Val::Text(t)) | ("filename", Val::Text(t)) => {
            if ct.name.is_none() && !t.is_empty() {
                ct.name = Some(t)
            }
        }
        ("start", Val::Text(t)) => ct.start = Some(t),
        ("type", Val::Text(t)) => ct.related_type = Some(t.to_ascii_lowercase()),
        ("type", Val::Int(c)) => ct.related_type = Some(well_known_media(c).to_string()),
        _ => {}
    }
}

/// Parameters until the reader is exhausted (inside a Value-length).
fn read_params(r: &mut Reader<'_>, ct: &mut ContentType) -> Result<(), Error> {
    let mut guard = 0;
    while !r.is_empty() {
        guard += 1;
        if guard > 64 {
            return Err(Error::Malformed("too many parameters"));
        }
        let b = r.peek()?;
        if b >= 0x80 || (1..=30).contains(&b) {
            let code = r.integer()?;
            let name = match code {
                0x00 => {
                    // Q-value: uintvar-encoded, 1..=2 octets.
                    r.uintvar()?;
                    continue;
                }
                0x01 => "charset",
                0x03 | 0x09 => "type",
                0x05 | 0x17 => "name",
                0x06 | 0x18 => "filename",
                0x0A | 0x19 => "start",
                _ => "",
            };
            // Typed type param of multipart/related is Constrained-encoding:
            // a short-integer or extension media text, both handled by read_val.
            let v = read_val(r)?;
            apply_param(ct, name, v);
        } else if b == 0 || b == 31 {
            return Err(Error::Malformed("bad parameter"));
        } else {
            let name = lossy(r.cstr()?).to_ascii_lowercase();
            let v = read_val(r)?;
            apply_param(ct, &name, v);
        }
    }
    Ok(())
}

/// Content-type-value: Constrained-media | Value-length Media-type.
pub(crate) fn content_type(r: &mut Reader<'_>) -> Result<ContentType, Error> {
    let b = r.peek()?;
    let mut ct = ContentType::default();
    match b {
        0x80..=0xFF => {
            ct.media = well_known_media(u64::from(r.u8()? & 0x7F)).to_string();
        }
        32..=127 => {
            ct.media = media_name(r.cstr()?);
        }
        _ => {
            let n = r.value_length()?;
            let mut inner = r.sub(n)?;
            let m = inner.peek()?;
            ct.media = if m >= 0x80 || (1..=30).contains(&m) {
                well_known_media(inner.integer()?).to_string()
            } else {
                media_name(inner.cstr()?)
            };
            read_params(&mut inner, &mut ct)?;
        }
    }
    if ct.media.is_empty() {
        ct.media = "application/octet-stream".into();
    }
    Ok(ct)
}

/// Part headers that matter for display.
#[derive(Default)]
pub(crate) struct PartHeaders {
    pub content_location: Option<String>,
    pub content_id: Option<String>,
    pub filename: Option<String>,
}

/// Headers of a multipart entry (after its content type). Lenient: an
/// unparseable header ends header parsing (the part is still kept).
pub(crate) fn part_headers(mut r: Reader<'_>) -> PartHeaders {
    let mut h = PartHeaders::default();
    let mut guard = 0;
    while !r.is_empty() && guard < 64 {
        guard += 1;
        let Ok(b) = r.peek() else { break };
        let res: Result<(), Error> = (|| {
            if b >= 0x80 {
                r.u8()?;
                match b & 0x7F {
                    0x0E => h.content_location = Some(lossy(r.text()?)),
                    0x40 => {
                        let v = lossy(r.text()?);
                        h.content_id = Some(v.trim_matches(|c| c == '<' || c == '>').to_string());
                    }
                    0x2E | 0x45 => {
                        // Content-disposition-value = Value-length Disposition *(Parameter)
                        if r.peek()? <= 31 {
                            let n = r.value_length()?;
                            let mut inner = r.sub(n)?;
                            inner.skip_value()?; // form-data / attachment / inline
                            let mut ct = ContentType::default();
                            read_params(&mut inner, &mut ct)?;
                            if let Some(name) = ct.name {
                                h.filename = Some(name);
                            }
                        } else {
                            r.skip_value()?;
                        }
                    }
                    _ => r.skip_value()?,
                }
                Ok(())
            } else if b >= 32 {
                // Application-header: Token-text Text-string.
                r.cstr()?;
                r.cstr()?;
                Ok(())
            } else {
                Err(Error::Malformed("header code page shift"))
            }
        })();
        if res.is_err() {
            break;
        }
    }
    h
}

/// One raw multipart entry: content type, headers and the data range
/// (absolute offsets into the PDU).
pub(crate) struct RawEntry {
    pub ct: ContentType,
    pub headers: PartHeaders,
    pub start: usize,
    pub len: usize,
}

/// WSP multipart (8.5): `nEntries` (ignored for termination, per 8.5.2),
/// then entries until the body ends or `max` entries were read.
/// `base` = absolute offset of `body` within the PDU.
pub(crate) fn multipart(
    body: &[u8],
    base: usize,
    max: usize,
) -> Result<(Vec<RawEntry>, bool), Error> {
    let mut r = Reader::new(body);
    let _n_entries = r.uintvar()?;
    let mut out = Vec::new();
    let mut truncated = false;
    while !r.is_empty() {
        if out.len() >= max {
            truncated = true;
            break;
        }
        let headers_len = r.uintvar()? as usize;
        let data_len = r.uintvar()? as usize;
        let mut hdr = r.sub(headers_len)?;
        let ct = content_type(&mut hdr)?;
        let headers = part_headers(hdr);
        let start = base + r.pos();
        r.take(data_len)?;
        out.push(RawEntry {
            ct,
            headers,
            start,
            len: data_len,
        });
    }
    Ok((out, truncated))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_type_forms() {
        let ct = content_type(&mut Reader::new(&[0x83])).unwrap();
        assert_eq!(ct.media, "text/plain");
        let ct = content_type(&mut Reader::new(b"application/smil\0")).unwrap();
        assert_eq!(ct.media, "application/smil");
        // Value-length text/plain; charset=utf-8
        let ct = content_type(&mut Reader::new(&[0x03, 0x83, 0x81, 0xEA])).unwrap();
        assert_eq!((ct.media.as_str(), ct.charset), ("text/plain", Some(106)));
        // UCS-2 as long integer charset (1000 = 0x03E8)
        let ct = content_type(&mut Reader::new(&[0x05, 0x83, 0x81, 0x02, 0x03, 0xE8])).unwrap();
        assert_eq!(ct.charset, Some(1000));
        // image/jpeg; name="a.jpg" (typed 0x85 Name)
        let mut b = vec![0x08, 0x9E, 0x85];
        b.extend(b"a.jpg\0");
        let ct = content_type(&mut Reader::new(&b)).unwrap();
        assert_eq!(
            (ct.media.as_str(), ct.name.as_deref()),
            ("image/jpeg", Some("a.jpg"))
        );
        // untyped parameter
        let mut b = vec![0x0F, 0x83];
        b.extend(b"charset\0utf-8\0");
        let ct = content_type(&mut Reader::new(&b)).unwrap();
        assert_eq!(ct.charset, Some(106));
        // length overruns the buffer
        assert!(content_type(&mut Reader::new(&[0x10, 0x83])).is_err());
    }
}
