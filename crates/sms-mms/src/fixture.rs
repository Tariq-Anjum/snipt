//! Test/debug support (feature `fixture`): PDU builders that follow the OMA
//! encoding byte for byte, and a minimal PNG encoder (stored deflate blocks)
//! so fixtures carry a real, decodable image. Never used by the app at
//! runtime.

/// WSP uintvar encoding.
pub fn uintvar(mut v: u32) -> Vec<u8> {
    let mut out = vec![(v & 0x7F) as u8];
    v >>= 7;
    while v > 0 {
        out.push(0x80 | (v & 0x7F) as u8);
        v >>= 7;
    }
    out.reverse();
    out
}

/// Value-length prefix for `n` bytes.
pub fn value_length(n: usize) -> Vec<u8> {
    if n <= 30 {
        vec![n as u8]
    } else {
        let mut v = vec![31];
        v.extend(uintvar(n as u32));
        v
    }
}

pub fn text(s: &str) -> Vec<u8> {
    let mut v = Vec::new();
    if s.as_bytes().first().is_some_and(|&b| b >= 0x80) {
        v.push(0x7F);
    }
    v.extend(s.as_bytes());
    v.push(0);
    v
}

pub fn long_integer(v: u64) -> Vec<u8> {
    let bytes: Vec<u8> = v
        .to_be_bytes()
        .into_iter()
        .skip_while(|&b| b == 0)
        .collect();
    let bytes = if bytes.is_empty() { vec![0] } else { bytes };
    let mut out = vec![bytes.len() as u8];
    out.extend(bytes);
    out
}

/// Encoded-string-value with an explicit UTF-8 charset.
pub fn encoded_utf8(s: &str) -> Vec<u8> {
    let mut inner = vec![0xEA]; // charset utf-8 (106) as short-integer
    inner.extend(text(s));
    let mut v = value_length(inner.len());
    v.extend(inner);
    v
}

/// From-value with Address-present-token.
pub fn from(addr: &str) -> Vec<u8> {
    let mut inner = vec![0x80];
    inner.extend(text(addr));
    let mut v = value_length(inner.len());
    v.extend(inner);
    v
}

/// An M-Notification.ind (Table 3 mandatory fields + From/Subject).
pub fn notification_ind(tid: &str, from_addr: &str, location: &str, expiry_rel_s: u64) -> Vec<u8> {
    let mut p = vec![0x8C, 0x82];
    p.push(0x98);
    p.extend(text(tid));
    p.extend([0x8D, 0x92]);
    p.push(0x89);
    p.extend(from(from_addr));
    p.push(0x96);
    p.extend(encoded_utf8("Photo"));
    p.extend([0x8A, 0x80]); // Message-Class: Personal
    p.push(0x8E);
    p.extend(long_integer(48_213));
    p.push(0x88);
    let mut exp = vec![0x81];
    exp.extend(long_integer(expiry_rel_s));
    p.extend(value_length(exp.len()));
    p.extend(exp);
    p.push(0x83);
    p.extend(text(location));
    p
}

/// One multipart entry: `ct` is an encoded Content-type-value.
pub fn entry(ct: &[u8], headers: &[u8], data: &[u8]) -> Vec<u8> {
    let mut v = uintvar((ct.len() + headers.len()) as u32);
    v.extend(uintvar(data.len() as u32));
    v.extend(ct);
    v.extend(headers);
    v.extend(data);
    v
}

/// Content-type-value general form: media short-int + typed params.
pub fn ct_general(media: u8, params: &[u8]) -> Vec<u8> {
    let mut inner = vec![0x80 | media];
    inner.extend(params);
    let mut v = value_length(inner.len());
    v.extend(inner);
    v
}

pub fn ct_text_media(media: &str, params: &[u8]) -> Vec<u8> {
    let mut inner = text(media);
    inner.extend(params);
    let mut v = value_length(inner.len());
    v.extend(inner);
    v
}

/// Content-Location (0x8E) + Content-ID (0xC0) part headers.
pub fn part_headers(location: &str, cid: &str) -> Vec<u8> {
    let mut h = vec![0x8E];
    h.extend(text(location));
    h.push(0xC0);
    h.push(b'"');
    h.extend(text(&format!("<{cid}>"))[..].iter().copied());
    h
}

/// An M-Retrieve.conf with multipart/related: SMIL, UTF-8 text, PNG image.
pub fn retrieve_conf(tid: &str, from_addr: &str, body_text: &str, png: &[u8]) -> Vec<u8> {
    let mut p = vec![0x8C, 0x84];
    p.push(0x98);
    p.extend(text(tid));
    p.extend([0x8D, 0x92]);
    p.push(0x8B);
    p.extend(text("msg-0001@mmsc.example"));
    p.push(0x85);
    p.extend(long_integer(1_795_000_000));
    p.push(0x89);
    p.extend(from(from_addr));
    p.push(0x97);
    p.extend(encoded_utf8("+919000000001/TYPE=PLMN"));
    p.push(0x96);
    p.extend(encoded_utf8("Photo"));
    p.extend([0x8A, 0x80]);
    p.extend([0x99, 0x80]); // Retrieve-Status: Ok
    // Content-Type: application/vnd.wap.multipart.related; type=application/smil; start=<smil>
    let mut params = vec![0x89];
    params.extend(text("application/smil"));
    params.push(0x8A);
    params.extend(text("<smil>"));
    p.push(0x84);
    p.extend(ct_general(0x33, &params));
    // Body
    let smil =
        b"<smil><body><par><img src=\"image.png\"/><text src=\"text.txt\"/></par></body></smil>";
    let parts = [
        entry(
            &ct_text_media("application/smil", &[]),
            &part_headers("smil.xml", "smil"),
            smil,
        ),
        entry(
            &ct_general(0x03, &[0x81, 0xEA]),
            &part_headers("text.txt", "text"),
            body_text.as_bytes(),
        ),
        entry(
            &{
                let mut n = vec![0x85];
                n.extend(text("image.png"));
                ct_general(0x20, &n)
            },
            &part_headers("image.png", "image"),
            png,
        ),
    ];
    p.extend(uintvar(parts.len() as u32));
    for e in parts {
        p.extend(e);
    }
    p
}

fn crc32(data: &[u8]) -> u32 {
    let mut c = 0xFFFF_FFFFu32;
    for &b in data {
        c ^= u32::from(b);
        for _ in 0..8 {
            c = if c & 1 != 0 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
        }
    }
    !c
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &x in data {
        a = (a + u32::from(x)) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend((data.len() as u32).to_be_bytes());
    let mut c = kind.to_vec();
    c.extend(data);
    out.extend(&c);
    out.extend(crc32(&c).to_be_bytes());
}

/// A `w`×`h` RGB gradient PNG (zlib stored blocks, valid per RFC 1950/1951).
pub fn png(w: u32, h: u32) -> Vec<u8> {
    let mut raw = Vec::new();
    for y in 0..h {
        raw.push(0); // filter: none
        for x in 0..w {
            raw.extend([(x * 255 / w.max(1)) as u8, (y * 255 / h.max(1)) as u8, 0x80]);
        }
    }
    let mut z = vec![0x78, 0x01];
    let blocks: Vec<&[u8]> = raw.chunks(65_535).collect();
    for (i, b) in blocks.iter().enumerate() {
        z.push(u8::from(i + 1 == blocks.len()));
        let len = b.len() as u16;
        z.extend(len.to_le_bytes());
        z.extend((!len).to_le_bytes());
        z.extend(*b);
    }
    z.extend(adler32(&raw).to_be_bytes());
    let mut out = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    let mut ihdr = Vec::new();
    ihdr.extend(w.to_be_bytes());
    ihdr.extend(h.to_be_bytes());
    ihdr.extend([8, 2, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"IDAT", &z);
    chunk(&mut out, b"IEND", &[]);
    out
}
