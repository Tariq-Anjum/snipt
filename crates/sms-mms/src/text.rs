//! Character-set decoding for MMS text (IANA MIBenum values, WSP Table 42)
//! and address normalisation (OMA-MMS-ENC §8).

/// IANA MIBenum values used by MMS.
pub mod charset {
    pub const US_ASCII: u32 = 3;
    pub const ISO_8859_1: u32 = 4;
    pub const UTF_8: u32 = 106;
    pub const UCS_2: u32 = 1000;
    pub const UTF_16BE: u32 = 1013;
    pub const UTF_16LE: u32 = 1014;
    pub const UTF_16: u32 = 1015;
}

fn utf16(bytes: &[u8], little: bool) -> String {
    // A trailing odd byte is dropped.
    let units = bytes.as_chunks::<2>().0.iter().map(|&c| {
        if little {
            u16::from_le_bytes(c)
        } else {
            u16::from_be_bytes(c)
        }
    });
    char::decode_utf16(units)
        .map(|r| r.unwrap_or(char::REPLACEMENT_CHARACTER))
        .collect()
}

/// Decode `bytes` in `charset` (None: MMS default for text/plain is
/// US-ASCII; decoded as UTF-8, a superset). Unknown charsets fall back to
/// lossy UTF-8. Never fails; invalid sequences become U+FFFD.
pub fn decode(bytes: &[u8], charset: Option<u32>) -> String {
    use charset::*;
    match charset.unwrap_or(UTF_8) {
        ISO_8859_1 => bytes.iter().map(|&b| char::from(b)).collect(),
        UCS_2 | UTF_16BE => utf16(bytes, false),
        UTF_16LE => utf16(bytes, true),
        UTF_16 => match bytes {
            [0xFF, 0xFE, rest @ ..] => utf16(rest, true),
            [0xFE, 0xFF, rest @ ..] => utf16(rest, false),
            _ => utf16(bytes, false),
        },
        // US-ASCII, UTF-8, "any" (0) and everything else.
        _ => {
            let b = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
            String::from_utf8_lossy(b).into_owned()
        }
    }
}

/// Remove C0/C1 control characters except TAB/LF/CR; used for header text.
pub fn strip_controls(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_control() || matches!(c, '\t' | '\n' | '\r'))
        .collect()
}

/// Truncate to at most `max` chars.
pub fn truncate_chars(s: &str, max: usize) -> String {
    match s.char_indices().nth(max) {
        Some((i, _)) => s[..i].to_string(),
        None => s.to_string(),
    }
}

/// OMA-MMS-ENC §8 device address: `+15551234567/TYPE=PLMN` → `+15551234567`.
/// E-mail and short codes are returned trimmed. Control characters removed.
pub fn normalize_address(raw: &str) -> String {
    let s = strip_controls(raw);
    let s = s.trim();
    let base = match s.rfind("/TYPE=") {
        Some(i) => &s[..i],
        None => match s.rfind("/type=") {
            Some(i) => &s[..i],
            None => s,
        },
    };
    truncate_chars(base.trim(), 128)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn charsets() {
        assert_eq!(decode(b"hi", Some(charset::US_ASCII)), "hi");
        assert_eq!(decode("₹ 500".as_bytes(), Some(charset::UTF_8)), "₹ 500");
        assert_eq!(decode(&[0xE9], Some(charset::ISO_8859_1)), "é");
        assert_eq!(
            decode(&[0x09, 0x39, 0x09, 0x3F], Some(charset::UCS_2)),
            "हि"
        );
        assert_eq!(
            decode(&[0xFF, 0xFE, 0x41, 0x00], Some(charset::UTF_16)),
            "A"
        );
        assert_eq!(decode(&[0x41, 0x00], Some(charset::UTF_16LE)), "A");
        // Odd trailing byte and lone surrogate do not panic.
        assert_eq!(
            decode(&[0xD8, 0x00, 0x41], Some(charset::UCS_2)),
            "\u{FFFD}"
        );
        assert_eq!(decode(&[0xC3], None), "\u{FFFD}");
        assert_eq!(decode(b"x", Some(9999)), "x");
    }

    #[test]
    fn addresses() {
        assert_eq!(
            normalize_address("+919876543210/TYPE=PLMN"),
            "+919876543210"
        );
        assert_eq!(normalize_address(" a@b.c "), "a@b.c");
        assert_eq!(normalize_address("12345\u{0}\u{7}"), "12345");
        assert_eq!(normalize_address("/TYPE=PLMN"), "");
    }
}
