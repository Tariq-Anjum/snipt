//! PBKDF2-HMAC-SHA256 with a 32-byte output (one PBKDF2 block), written so
//! that passphrase-derived state is wiped. Output is byte-identical to
//! RFC 8018 PBKDF2 / WebCrypto `deriveBits` (cross-checked against the
//! `pbkdf2` crate, RFC 7914 §11 vectors and the TS-written v1 fixture).
//!
//! Why not `pbkdf2::pbkdf2_hmac`: in the pinned `hmac 0.13.0` /
//! `pbkdf2 0.13.0`, `HmacCore::new_from_slice` leaves the padded key block
//! (passphrase ⊕ ipad/opad) on the stack unwiped, and `pbkdf2_body` leaves the
//! last `U_i` block unwiped, whatever features are enabled.
//!
//! What this implementation wipes:
//! - the padded HMAC key block and the ipad/opad blocks (`Zeroizing`);
//! - the pre-hash of a passphrase longer than 64 bytes;
//! - every SHA-256 hasher (`sha2` built with `zeroize`: state and block
//!   buffer are zeroized on drop, including the per-round clones);
//! - each `U_i` and the inner digest after use; the result is returned in
//!   `Zeroizing`.
//!
//! What remains outside our control (documented, not faked): copies the
//! compiler makes when it moves values or spills registers, which no
//! safe-Rust zeroize can reach, and the caller's own passphrase buffer
//! (see `backup` module docs for ownership).

use sha2::{Digest, Sha256, digest::Output};
use zeroize::{Zeroize, Zeroizing};

// Compile-time guard: the hasher (state + block buffer) must zeroize on drop,
// i.e. `sha2` must be built with its `zeroize` feature.
const _: fn() = || {
    fn zeroize_on_drop<T: zeroize::ZeroizeOnDrop>() {}
    zeroize_on_drop::<Sha256>();
};

const BLOCK: usize = 64;
const IPAD: u8 = 0x36;
const OPAD: u8 = 0x5c;

/// `h.finalize()` into `dst`, wiping the intermediate output array.
fn finalize_wiping(h: Sha256, dst: &mut [u8; 32]) {
    let mut out = Output::<Sha256>::default();
    h.finalize_into(&mut out);
    dst.copy_from_slice(&out);
    out.as_mut_slice().zeroize();
}

/// PBKDF2-HMAC-SHA256(password, salt, rounds, dkLen = 32).
pub(crate) fn pbkdf2_hmac_sha256_32(
    password: &[u8],
    salt: &[u8],
    rounds: u32,
) -> Zeroizing<[u8; 32]> {
    assert!(rounds >= 1, "PBKDF2 needs at least one round");

    // HMAC key block K0 (RFC 2104): pre-hash keys longer than the block size.
    let mut k0 = Zeroizing::new([0u8; BLOCK]);
    if password.len() > BLOCK {
        let mut hk = Zeroizing::new([0u8; 32]);
        let mut h = Sha256::new();
        h.update(password);
        finalize_wiping(h, &mut hk);
        k0[..32].copy_from_slice(&hk[..]);
    } else {
        k0[..password.len()].copy_from_slice(password);
    }
    let mut pad = Zeroizing::new([0u8; BLOCK]);
    for (p, k) in pad.iter_mut().zip(k0.iter()) {
        *p = k ^ IPAD;
    }
    let mut inner_base = Sha256::new();
    inner_base.update(&pad[..]);
    for (p, k) in pad.iter_mut().zip(k0.iter()) {
        *p = k ^ OPAD;
    }
    let mut outer_base = Sha256::new();
    outer_base.update(&pad[..]);
    drop(pad);
    drop(k0);

    let mut u = Zeroizing::new([0u8; 32]);
    let mut prev = Zeroizing::new([0u8; 32]);
    let mut inner = Zeroizing::new([0u8; 32]);
    let mut t = Zeroizing::new([0u8; 32]);
    let mut hmac = |msg: &[&[u8]], out: &mut [u8; 32]| {
        let mut h = inner_base.clone();
        for m in msg {
            h.update(m);
        }
        finalize_wiping(h, &mut inner);
        let mut h = outer_base.clone();
        h.update(&inner[..]);
        finalize_wiping(h, out);
    };

    // U_1 = PRF(P, S || INT(1)); T = U_1 ^ U_2 ^ ... ^ U_c
    hmac(&[salt, &1u32.to_be_bytes()], &mut u);
    t.copy_from_slice(&u[..]);
    for _ in 1..rounds {
        prev.copy_from_slice(&u[..]);
        hmac(&[&prev[..]], &mut u);
        for (a, b) in t.iter_mut().zip(u.iter()) {
            *a ^= b;
        }
    }
    t
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(b: &[u8]) -> String {
        b.iter().map(|x| format!("{x:02x}")).collect()
    }

    /// RFC 7914 §11 PBKDF2-HMAC-SHA256 vectors (first 32 bytes of dkLen 64).
    #[test]
    fn rfc7914_vectors() {
        let k = pbkdf2_hmac_sha256_32(b"passwd", b"salt", 1);
        assert_eq!(
            hex(&k[..]),
            "55ac046e56e3089fec1691c22544b605f94185216dde0465e68b9d57c20dacbc"
        );
        let k = pbkdf2_hmac_sha256_32(b"Password", b"NaCl", 80_000);
        assert_eq!(
            hex(&k[..]),
            "4ddcd8f60b98be21830cee5ef22701f9641a4418d04c0414aeff08876b34ab56"
        );
    }

    /// Byte-identical to the previously used `pbkdf2` crate, including keys
    /// shorter than, equal to and longer than the 64-byte HMAC block.
    #[test]
    fn equals_pbkdf2_crate() {
        let salts: [&[u8]; 3] = [b"", b"0123456789abcdef", &[0xa5; 80]];
        for len in [0usize, 1, 6, 19, 32, 63, 64, 65, 100, 200] {
            let pass: Vec<u8> = (0..len).map(|i| (i * 31 + 7) as u8).collect();
            for salt in salts {
                for rounds in [1u32, 2, 3, 1000] {
                    let mut want = [0u8; 32];
                    pbkdf2::pbkdf2_hmac::<sha2::Sha256>(&pass, salt, rounds, &mut want);
                    let got = pbkdf2_hmac_sha256_32(&pass, salt, rounds);
                    assert_eq!(*got, want, "len={len} salt={} rounds={rounds}", salt.len());
                }
            }
        }
    }
}
