//! libubox `blob_attr`: the tagged, length-prefixed, 4-byte-aligned container every ubus message is made of.
//!
//! Header (one big-endian `u32`, `blob.h`): bit 31 `BLOB_ATTR_EXTENDED`, bits 24..30 the id (`BLOB_ATTR_ID_MASK
//! 0x7f000000`), bits 0..23 the raw length including the 4-byte header (`BLOB_ATTR_LEN_MASK 0x00ffffff`). The
//! payload is padded with zeros to `BLOB_ATTR_ALIGN` (4); a parent's length counts its children padded, a leaf's
//! own length does not count its trailing padding (`blob_new`, `blob_fill_pad` in `blob.c`).
//!
//! This module only frames bytes; what an id means belongs to [`crate::proto`] and [`crate::blobmsg`].

use crate::UbusError;

/// `BLOB_ATTR_EXTENDED`: the attribute carries a blobmsg name header.
pub(crate) const EXTENDED: u32 = 0x8000_0000;
/// `BLOB_ATTR_ID_MASK`.
const ID_MASK: u32 = 0x7f00_0000;
/// `BLOB_ATTR_ID_SHIFT`.
const ID_SHIFT: u32 = 24;
/// `BLOB_ATTR_LEN_MASK`: the largest raw length an attribute can declare.
pub(crate) const LEN_MASK: u32 = 0x00ff_ffff;
/// `BLOB_ATTR_ALIGN`.
pub(crate) const ALIGN: usize = 4;
/// `sizeof(struct blob_attr)`.
pub(crate) const HEADER_LEN: usize = 4;

/// Rounds `len` up to the attribute alignment (`blob_pad_len`).
pub(crate) const fn pad(len: usize) -> usize {
    (len + ALIGN - 1) & !(ALIGN - 1)
}

/// Splits a raw header word into `(id, extended, raw_len)`.
pub(crate) const fn split_header(word: u32) -> (u8, bool, usize) {
    // the mask keeps 7 bits, so the narrowing is exact
    let id = ((word & ID_MASK) >> ID_SHIFT) as u8;
    (id, word & EXTENDED != 0, (word & LEN_MASK) as usize)
}

/// One parsed attribute, borrowing its payload (the bytes after the 4-byte header, without trailing padding).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Attr<'a> {
    pub id: u8,
    pub extended: bool,
    pub payload: &'a [u8],
}

impl<'a> Attr<'a> {
    /// Parses the attribute at the start of `buf`; returns it and the padded length it occupies (clamped to `buf`).
    pub(crate) fn parse(buf: &'a [u8]) -> Result<(Attr<'a>, usize), UbusError> {
        let head: [u8; HEADER_LEN] =
            buf.get(..HEADER_LEN).and_then(|h| h.try_into().ok()).ok_or_else(|| proto_err("truncated blob header"))?;
        let (id, extended, raw_len) = split_header(u32::from_be_bytes(head));
        if raw_len < HEADER_LEN {
            return Err(proto_err("blob length shorter than its header"));
        }
        let payload =
            buf.get(HEADER_LEN..raw_len).ok_or_else(|| proto_err("blob length past the end of its parent"))?;
        let used = pad(raw_len).min(buf.len());
        Ok((Attr { id, extended, payload }, used))
    }

    /// The children of a nested attribute (its payload read as a sequence of attributes).
    pub(crate) fn children(&self) -> Children<'a> {
        Children { rest: self.payload }
    }

    /// The payload as a big-endian `u32` (`blob_get_u32`); `None` when shorter than 4 bytes.
    pub(crate) fn get_u32(&self) -> Option<u32> {
        let b: [u8; 4] = self.payload.get(..4)?.try_into().ok()?;
        Some(u32::from_be_bytes(b))
    }

    /// The payload as a NUL-terminated string (`blob_data` of a `BLOB_ATTR_STRING`).
    pub(crate) fn get_str(&self) -> Option<&'a str> {
        let end = self.payload.iter().position(|&b| b == 0)?;
        std::str::from_utf8(self.payload.get(..end)?).ok()
    }
}

/// Iterates the attributes packed in a payload. Stops at the first malformed attribute, yielding its error.
#[derive(Debug, Clone)]
pub(crate) struct Children<'a> {
    rest: &'a [u8],
}

impl<'a> Children<'a> {
    pub(crate) fn new(rest: &'a [u8]) -> Self {
        Children { rest }
    }
}

impl<'a> Iterator for Children<'a> {
    type Item = Result<Attr<'a>, UbusError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.rest.is_empty() {
            return None;
        }
        match Attr::parse(self.rest) {
            Ok((attr, used)) => {
                self.rest = self.rest.get(used..).unwrap_or_default();
                Some(Ok(attr))
            }
            Err(e) => {
                self.rest = &[];
                Some(Err(e))
            }
        }
    }
}

/// Builds attributes into one buffer (`struct blob_buf`). Every attribute starts 4-aligned because each `end`
/// pads the buffer after it.
#[derive(Debug, Default)]
pub(crate) struct BlobWriter {
    buf: Vec<u8>,
}

/// The position of an open attribute, returned by [`BlobWriter::start`] and closed by [`BlobWriter::end`].
#[derive(Debug, Clone, Copy)]
#[must_use = "an open attribute must be closed with BlobWriter::end"]
pub(crate) struct Open {
    offset: usize,
    word: u32,
}

impl BlobWriter {
    pub(crate) fn new() -> Self {
        BlobWriter::default()
    }

    /// Opens an attribute with `id`; the length is filled in by [`BlobWriter::end`].
    pub(crate) fn start(&mut self, id: u8, extended: bool) -> Open {
        let offset = self.buf.len();
        let mut word = (u32::from(id) << ID_SHIFT) & ID_MASK;
        if extended {
            word |= EXTENDED;
        }
        self.buf.extend_from_slice(&word.to_be_bytes());
        Open { offset, word }
    }

    /// Appends raw payload bytes to the open attribute.
    pub(crate) fn put_bytes(&mut self, bytes: &[u8]) {
        self.buf.extend_from_slice(bytes);
    }

    /// Pads the buffer with zeros to the attribute alignment.
    pub(crate) fn align(&mut self) {
        self.buf.resize(pad(self.buf.len()), 0);
    }

    /// Closes the attribute opened at `open`: writes its raw length (without trailing padding), then pads.
    pub(crate) fn end(&mut self, open: Open) -> Result<(), UbusError> {
        let raw = self.buf.len() - open.offset;
        let raw = u32::try_from(raw)
            .ok()
            .filter(|&r| r <= LEN_MASK)
            .ok_or_else(|| proto_err("blob attribute longer than 16 MiB"))?;
        let word = open.word | raw;
        if let Some(slot) = self.buf.get_mut(open.offset..open.offset + HEADER_LEN) {
            slot.copy_from_slice(&word.to_be_bytes());
        }
        self.align();
        Ok(())
    }

    /// A complete leaf attribute: `id` with `payload`.
    pub(crate) fn put(&mut self, id: u8, payload: &[u8]) -> Result<(), UbusError> {
        let open = self.start(id, false);
        self.put_bytes(payload);
        self.end(open)
    }

    /// `blob_put_u32` / `blob_put_int32`.
    pub(crate) fn put_u32(&mut self, id: u8, value: u32) -> Result<(), UbusError> {
        self.put(id, &value.to_be_bytes())
    }

    /// `blob_put_string`: the bytes and a terminating NUL.
    pub(crate) fn put_string(&mut self, id: u8, value: &str) -> Result<(), UbusError> {
        let open = self.start(id, false);
        self.put_bytes(value.as_bytes());
        self.put_bytes(&[0]);
        self.end(open)
    }

    pub(crate) fn len(&self) -> usize {
        self.buf.len()
    }

    pub(crate) fn into_bytes(self) -> Vec<u8> {
        self.buf
    }
}

pub(crate) fn proto_err(msg: &str) -> UbusError {
    UbusError::Protocol(msg.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_fields_split() {
        assert_eq!(split_header(0x8300_000a), (3, true, 10));
        assert_eq!(split_header(0x0200_0008), (2, false, 8));
        assert_eq!(pad(0), 0);
        assert_eq!(pad(9), 12);
        assert_eq!(pad(12), 12);
    }

    #[test]
    fn put_u32_and_string_bytes() -> Result<(), UbusError> {
        let mut w = BlobWriter::new();
        w.put_u32(3, 0x0102_0304)?;
        w.put_string(2, "f50")?;
        w.put_string(4, "ab")?;
        assert_eq!(
            w.into_bytes(),
            vec![
                0x03, 0, 0, 0x08, 1, 2, 3, 4, // OBJID-like u32
                0x02, 0, 0, 0x08, b'f', b'5', b'0', 0, // string, exactly aligned
                0x04, 0, 0, 0x07, b'a', b'b', 0, 0, // raw length 7, one byte of padding
            ]
        );
        Ok(())
    }

    #[test]
    fn nested_length_counts_padded_children() -> Result<(), UbusError> {
        let mut w = BlobWriter::new();
        let root = w.start(0, false);
        w.put_string(4, "ab")?;
        w.end(root)?;
        assert_eq!(w.into_bytes(), vec![0, 0, 0, 0x0c, 0x04, 0, 0, 0x07, b'a', b'b', 0, 0]);
        Ok(())
    }

    #[test]
    fn parse_round_trip() -> Result<(), UbusError> {
        let bytes = [0, 0, 0, 0x0c, 0x04, 0, 0, 0x07, b'a', b'b', 0, 0];
        let (root, used) = Attr::parse(&bytes)?;
        assert_eq!(used, 12);
        assert_eq!((root.id, root.extended), (0, false));
        let kids: Vec<_> = root.children().collect::<Result<_, _>>()?;
        assert_eq!(kids.len(), 1);
        assert_eq!(kids[0].id, 4);
        assert_eq!(kids[0].get_str(), Some("ab"));
        Ok(())
    }

    #[test]
    fn parse_rejects_bad_lengths() {
        assert!(Attr::parse(&[0, 0, 0]).is_err());
        assert!(Attr::parse(&[0, 0, 0, 3]).is_err());
        assert!(Attr::parse(&[0, 0, 0, 9, 1, 2, 3, 4]).is_err());
        let mut kids = Children::new(&[0x01, 0, 0, 0x08, 0, 0, 0, 0, 0xff]);
        assert!(matches!(kids.next(), Some(Ok(_))));
        assert!(matches!(kids.next(), Some(Err(_))));
        assert!(kids.next().is_none());
    }
}
