//! The signature of a release's checksums (docs/research/FINDINGS.md 86): Ed25519 with the project's key, which is a
//! file of the installed image and nothing else.
//!
//! Two formats carry it. A minisign signature (`SHA256SUMS.minisig`: two comment lines, the signature and a global
//! signature over it and the trusted comment) is checked here with `minisign-verify`. The releases of today publish
//! usign's format (`SHA256SUMS.sig`: a comment and one base64 line, "Ed", the key's number and 64 bytes), which has
//! no global signature, and `minisign-verify` will not take a signature without one: that format is checked by the
//! image's own `usign -V`, as the shell did - its exit status directly, after the key number was compared here. The
//! key file is the same for both (usign and minisign public keys are the same 42 bytes).

use std::path::Path;
use std::time::Duration;

use f50_core::exec::Cmd;
use serde::{Deserialize, Serialize};

use crate::util::trusted_file;

/// How the signature of a release's checksums checks on this device. Only `Valid` says the files are the
/// project's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SigState {
    Valid,
    /// It does not match the key: refused, always.
    Bad,
    /// The release has no signature.
    Unsigned,
    /// This system has no release key.
    NoKey,
    /// What is at the key's place is not root's own regular file: refused, always.
    BadKey,
    /// Nothing here can check this format.
    NoTool,
}

/// The release key of this system.
#[derive(Debug, Clone)]
pub enum Key {
    None,
    Unsafe,
    Ok(String),
}

/// The key file at `path` (a device path under the root): none, a key we may trust, or something else there.
pub fn key(path: &Path) -> Key {
    if std::fs::symlink_metadata(path).is_err() {
        return Key::None;
    }
    if !trusted_file(path) {
        return Key::Unsafe;
    }
    match std::fs::read_to_string(path) {
        Ok(s) if s.len() < 4096 && raw_line(&s).is_some_and(|r| r.len() == 42 && r.starts_with(b"Ed")) => Key::Ok(s),
        _ => Key::Unsafe,
    }
}

/// Standard base64 (with or without padding); `None` on anything else.
pub fn base64(s: &str) -> Option<Vec<u8>> {
    let val = |c: u8| -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        } as u32)
    };
    let s = s.trim().trim_end_matches('=');
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in s.bytes() {
        acc = (acc << 6) | val(c)?;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((acc >> bits) & 0xff) as u8);
        }
    }
    Some(out)
}

/// The base64 line of a key or a signature (the first line that is not a comment), decoded.
fn raw_line(s: &str) -> Option<Vec<u8>> {
    let line = s.lines().find(|l| !l.starts_with("untrusted comment:") && !l.trim().is_empty())?;
    base64(line)
}

/// A minisign signature of `data`, checked in Rust.
pub fn check_minisign(key_text: &str, sig_text: &str, data: &[u8]) -> SigState {
    let Ok(pk) = minisign_verify::PublicKey::decode(key_text.trim()) else { return SigState::BadKey };
    let Ok(sig) = minisign_verify::Signature::decode(sig_text) else { return SigState::Bad };
    if pk.verify(data, &sig, true).is_ok() { SigState::Valid } else { SigState::Bad }
}

/// The key number of a usign signature is the key's (bytes 2..10 of both): a signature of another key is bad
/// before any tool is asked.
pub fn usign_key_matches(key_text: &str, sig_text: &str) -> bool {
    match (raw_line(key_text), raw_line(sig_text)) {
        (Some(k), Some(s)) => k.len() == 42 && s.len() == 74 && s.starts_with(b"Ed") && k[..10] == s[..10],
        _ => false,
    }
}

/// A usign signature (`sig`, a file) of `sums` (a file) against `key` (the key file), by the image's usign.
pub async fn check_usign(usign: &Path, key_file: &Path, key_text: &str, sums: &Path, sig: &Path) -> SigState {
    let Ok(sig_text) = std::fs::read_to_string(sig) else { return SigState::Unsigned };
    if !usign_key_matches(key_text, &sig_text) {
        return SigState::Bad;
    }
    let out = Cmd::new(usign)
        .arg("-V")
        .arg("-q")
        .arg("-m")
        .arg(sums)
        .arg("-p")
        .arg(key_file)
        .arg("-x")
        .arg(sig)
        .timeout(Duration::from_secs(30))
        .run()
        .await;
    match out {
        Ok(o) if o.success() => SigState::Valid,
        Ok(_) => SigState::Bad,
        Err(_) => SigState::NoTool,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // a public key in the format usign and minisign share ("Ed", an 8-byte key number, 32 bytes); made up for the
    // tests, no secret key exists for it
    const PUB: &str = "untrusted comment: minisign public key 7D6A0F57B3A6B3E9\nRWTps6azVw9qfeH8zNshVUX5SsvjCz9N8dMvXQqY8o0Ynp0WDAt04DCV\n";

    #[test]
    fn base64_and_key_numbers() {
        assert_eq!(base64("aGVsbG8=").as_deref(), Some(&b"hello"[..]));
        assert_eq!(base64("aGVsbG8").as_deref(), Some(&b"hello"[..]));
        assert!(base64("a$b").is_none());
        let k = raw_line(PUB).expect("key");
        assert_eq!((k.len(), &k[..2]), (42, &b"Ed"[..]));
        // a usign signature with the key's number, and one with another
        let mut sig = b"Ed".to_vec();
        sig.extend_from_slice(&k[2..10]);
        sig.extend_from_slice(&[0u8; 64]);
        let enc = |b: &[u8]| {
            const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
            let mut s = String::new();
            for c in b.chunks(3) {
                let n = (u32::from(c[0]) << 16)
                    | (u32::from(*c.get(1).unwrap_or(&0)) << 8)
                    | u32::from(*c.get(2).unwrap_or(&0));
                for i in 0..=c.len() {
                    s.push(A[((n >> (18 - 6 * i)) & 63) as usize] as char);
                }
            }
            s
        };
        let good = format!("untrusted comment: signed\n{}\n", enc(&sig));
        assert!(usign_key_matches(PUB, &good));
        sig[3] ^= 1;
        let other = format!("untrusted comment: signed\n{}\n", enc(&sig));
        assert!(!usign_key_matches(PUB, &other));
    }

    #[test]
    fn a_minisign_signature_that_does_not_match_is_bad() {
        let sig = "untrusted comment: x\nRWTps6azVw9qfQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA==\ntrusted comment: timestamp:0\nAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA==\n";
        assert_eq!(check_minisign(PUB, sig, b"test\n"), SigState::Bad);
        assert_eq!(check_minisign(PUB, "not a signature", b"test\n"), SigState::Bad);
        assert_eq!(check_minisign("garbage", sig, b"test\n"), SigState::BadKey);
    }
}
