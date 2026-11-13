//! MD5, only for identities the shell pool already wrote.
//!
//! The pool this crate replaces (`f50-sms`) named messages by `md5sum` of "sender|time|text", their parts by the
//! md5 of each PDU, and kept deleted ones as a list of those sums. Computing the same sums keeps a migrated pool's
//! fingerprints, `part_ids` and tombstones valid. It is an identity, never a security property.

use md5::{Digest, Md5};

/// The lowercase hex digest, as `md5sum` prints it.
pub(crate) fn hex(data: &[u8]) -> String {
    Md5::digest(data).iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::hex;

    #[test]
    fn rfc1321_vectors() {
        assert_eq!(hex(b""), "d41d8cd98f00b204e9800998ecf8427e");
        assert_eq!(hex(b"abc"), "900150983cd24fb0d6963f7d28e17f72");
        assert_eq!(hex(b"message digest"), "f96b697d7cb7938d525a2f31aaf161d0");
        assert_eq!(
            hex(b"12345678901234567890123456789012345678901234567890123456789012345678901234567890"),
            "57edf4a22be3c955ac49da2e2107b67a"
        );
    }
}
