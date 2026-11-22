//! Bounds-checked cursor over an untrusted byte slice. Every read returns a
//! `Result`; nothing indexes past the end and nothing panics.

use crate::Error;

#[derive(Clone, Copy)]
pub(crate) struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

/// Longest NUL-terminated text accepted anywhere in a PDU header (a
/// Content-Location URI is the longest legitimate one).
pub(crate) const MAX_TEXT: usize = 4096;

impl<'a> Reader<'a> {
    pub(crate) fn new(buf: &'a [u8]) -> Self {
        Reader { buf, pos: 0 }
    }

    pub(crate) fn pos(&self) -> usize {
        self.pos
    }

    pub(crate) fn remaining(&self) -> usize {
        self.buf.len() - self.pos
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.remaining() == 0
    }

    pub(crate) fn peek(&self) -> Result<u8, Error> {
        self.buf.get(self.pos).copied().ok_or(Error::Truncated)
    }

    pub(crate) fn u8(&mut self) -> Result<u8, Error> {
        let b = self.peek()?;
        self.pos += 1;
        Ok(b)
    }

    pub(crate) fn take(&mut self, n: usize) -> Result<&'a [u8], Error> {
        if n > self.remaining() {
            return Err(Error::Truncated);
        }
        let s = &self.buf[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }

    /// A sub-reader over the next `n` bytes (consumed from `self`).
    pub(crate) fn sub(&mut self, n: usize) -> Result<Reader<'a>, Error> {
        Ok(Reader::new(self.take(n)?))
    }

    /// WSP uintvar (8.1.2): 1..=5 octets, 7 bits each, MSB = continuation.
    /// Values above u32::MAX are rejected.
    pub(crate) fn uintvar(&mut self) -> Result<u32, Error> {
        let mut v: u64 = 0;
        for i in 0..5 {
            let b = self.u8()?;
            v = (v << 7) | u64::from(b & 0x7F);
            if b & 0x80 == 0 {
                return u32::try_from(v).map_err(|_| Error::Malformed("uintvar overflow"));
            }
            if i == 4 {
                break;
            }
        }
        Err(Error::Malformed("uintvar too long"))
    }

    /// Bytes up to (excluding) the next NUL; the NUL is consumed.
    pub(crate) fn cstr(&mut self) -> Result<&'a [u8], Error> {
        let rest = &self.buf[self.pos..];
        let end = rest.iter().position(|&b| b == 0).ok_or(Error::Truncated)?;
        if end > MAX_TEXT {
            return Err(Error::Malformed("text too long"));
        }
        self.pos += end + 1;
        Ok(&rest[..end])
    }

    /// Text-string: optional Quote (127) then text, NUL-terminated. Also
    /// strips the leading `"` of a Quoted-string.
    pub(crate) fn text(&mut self) -> Result<&'a [u8], Error> {
        let s = self.cstr()?;
        Ok(match s.first() {
            Some(0x7F) | Some(b'"') => &s[1..],
            _ => s,
        })
    }

    /// Value-length (8.4.2.2): Short-length 0..=30, or 31 + uintvar.
    pub(crate) fn value_length(&mut self) -> Result<usize, Error> {
        let b = self.u8()?;
        match b {
            0..=30 => Ok(usize::from(b)),
            31 => Ok(self.uintvar()? as usize),
            _ => Err(Error::Malformed("expected value-length")),
        }
    }

    /// Long-integer: Short-length (1..=30) then big-endian octets. More than
    /// 8 significant octets cannot fit a u64 and are rejected.
    pub(crate) fn long_integer(&mut self) -> Result<u64, Error> {
        let n = self.u8()?;
        if n == 0 || n > 30 {
            return Err(Error::Malformed("bad long-integer length"));
        }
        let bytes = self.take(usize::from(n))?;
        let mut v: u64 = 0;
        for &b in bytes {
            if v >> 56 != 0 {
                return Err(Error::Malformed("long-integer overflow"));
            }
            v = (v << 8) | u64::from(b);
        }
        Ok(v)
    }

    /// Integer-value: Short-integer (one octet, MSB set) or Long-integer.
    pub(crate) fn integer(&mut self) -> Result<u64, Error> {
        let b = self.peek()?;
        if b & 0x80 != 0 {
            self.pos += 1;
            Ok(u64::from(b & 0x7F))
        } else {
            self.long_integer()
        }
    }

    /// Skip one generic WSP value (8.4.1.2 field value forms): Short-length +
    /// data, Length-quote + uintvar + data, NUL-terminated text, or a single
    /// short-integer octet.
    pub(crate) fn skip_value(&mut self) -> Result<(), Error> {
        let b = self.peek()?;
        match b {
            0..=31 => {
                let n = self.value_length()?;
                self.take(n)?;
            }
            32..=127 => {
                self.cstr()?;
            }
            _ => {
                self.pos += 1;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uintvar_bounds() {
        assert_eq!(Reader::new(&[0x00]).uintvar().unwrap(), 0);
        assert_eq!(Reader::new(&[0x81, 0x00]).uintvar().unwrap(), 128);
        assert_eq!(
            Reader::new(&[0x8F, 0xFF, 0xFF, 0xFF, 0x7F])
                .uintvar()
                .unwrap(),
            u32::MAX
        );
        assert!(
            Reader::new(&[0x9F, 0xFF, 0xFF, 0xFF, 0x7F])
                .uintvar()
                .is_err()
        );
        assert!(
            Reader::new(&[0x81, 0x81, 0x81, 0x81, 0x81, 0x01])
                .uintvar()
                .is_err()
        );
        assert!(Reader::new(&[0x81]).uintvar().is_err());
    }

    #[test]
    fn long_integer_bounds() {
        assert_eq!(Reader::new(&[1, 0xFF]).long_integer().unwrap(), 255);
        assert_eq!(
            Reader::new(&[4, 0x5F, 0x5E, 0x10, 0x00])
                .long_integer()
                .unwrap(),
            0x5F5E_1000
        );
        assert!(Reader::new(&[0]).long_integer().is_err());
        assert!(Reader::new(&[3, 1, 2]).long_integer().is_err());
        let mut nine = vec![9u8, 1];
        nine.extend([0u8; 8]);
        assert!(Reader::new(&nine).long_integer().is_err());
        // Leading zero octets are tolerated (still fits).
        let mut padded = vec![10u8, 0, 0];
        padded.extend([0xFFu8; 8]);
        assert_eq!(Reader::new(&padded).long_integer().unwrap(), u64::MAX);
    }

    #[test]
    fn text_forms() {
        assert_eq!(Reader::new(b"abc\0").text().unwrap(), b"abc");
        assert_eq!(Reader::new(b"\x7f\xe9t\0").text().unwrap(), b"\xe9t");
        assert_eq!(Reader::new(b"\"<cid>\0").text().unwrap(), b"<cid>");
        assert!(Reader::new(b"abc").text().is_err());
    }
}
