//! Message framing over the ubusd stream socket, mirroring `get_next_msg` and `ubus_validate_hdr` in
//! `libubus-io.c`: read the 8-byte header and the 4-byte root blob header, validate version and length, then read
//! the rest of the root. A message over [`MAX_MSG_LEN`] cannot be skipped (the stream cannot resynchronise), so it
//! is an error that ends the connection.
//!
//! Neither function is cancel-safe: a reader task owns each read half and forwards whole messages over a channel.

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::UbusError;
use crate::blob::{self, proto_err};
use crate::proto::{HEADER_LEN, Header, MAX_MSG_LEN, Message};

/// Reads one message. `Ok(None)` is a clean end of stream before a new message began.
pub(crate) async fn read_message<R: AsyncRead + Unpin>(r: &mut R) -> Result<Option<Message>, UbusError> {
    let mut fixed = [0u8; HEADER_LEN + blob::HEADER_LEN];
    // first byte separately, so EOF between messages is distinguishable from a truncated one
    let mut first = [0u8; 1];
    if r.read(&mut first).await? == 0 {
        return Ok(None);
    }
    fixed[0] = first[0];
    r.read_exact(&mut fixed[1..]).await?;
    let (header, raw_len) = parse_fixed(&fixed)?;
    let mut body = vec![0u8; raw_len];
    body[..blob::HEADER_LEN].copy_from_slice(&fixed[HEADER_LEN..]);
    r.read_exact(&mut body[blob::HEADER_LEN..]).await?;
    Ok(Some(Message { header, body }))
}

/// Validates the fixed 12 bytes; returns the header and the root blob's raw length.
fn parse_fixed(fixed: &[u8; HEADER_LEN + blob::HEADER_LEN]) -> Result<(Header, usize), UbusError> {
    let [version, ty, s0, s1, p0, p1, p2, p3, b0, b1, b2, b3] = *fixed;
    if version != 0 {
        return Err(UbusError::Protocol(format!("unsupported ubus message version {version}")));
    }
    let header = Header { ty, seq: u16::from_be_bytes([s0, s1]), peer: u32::from_be_bytes([p0, p1, p2, p3]) };
    let (_, _, raw_len) = blob::split_header(u32::from_be_bytes([b0, b1, b2, b3]));
    if raw_len < blob::HEADER_LEN {
        return Err(proto_err("message root blob shorter than its header"));
    }
    if blob::pad(raw_len) > MAX_MSG_LEN {
        return Err(proto_err("message larger than UBUS_MAX_MSGLEN"));
    }
    Ok((header, raw_len))
}

/// Writes one message in a single buffer, then flushes.
pub(crate) async fn write_message<W: AsyncWrite + Unpin>(w: &mut W, msg: &Message) -> Result<(), UbusError> {
    if msg.body.len() > MAX_MSG_LEN {
        return Err(proto_err("message larger than UBUS_MAX_MSGLEN"));
    }
    w.write_all(&msg.to_bytes()).await?;
    w.flush().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::{Body, attr, msg};

    #[tokio::test]
    async fn round_trip_over_a_pipe() -> Result<(), UbusError> {
        let (mut a, mut b) = tokio::io::duplex(256);
        let sent = Body::new().string(attr::OBJPATH, "network")?.finish(msg::LOOKUP, 9, 0)?;
        write_message(&mut a, &sent).await?;
        drop(a);
        assert_eq!(read_message(&mut b).await?, Some(sent));
        assert_eq!(read_message(&mut b).await?, None);
        Ok(())
    }

    #[tokio::test]
    async fn reads_hello_bytes() -> Result<(), UbusError> {
        let bytes: &[u8] = &[0, 0, 0, 0, 0x12, 0x34, 0xab, 0xcd, 0, 0, 0, 4];
        let mut r = bytes;
        let m = read_message(&mut r).await?.ok_or_else(|| proto_err("eof"))?;
        assert_eq!(m.header, Header { ty: msg::HELLO, seq: 0, peer: 0x1234_abcd });
        assert_eq!(m.body, vec![0, 0, 0, 4]);
        Ok(())
    }

    #[tokio::test]
    async fn rejects_bad_version_and_size() {
        let mut bad_version: &[u8] = &[1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 4];
        assert!(read_message(&mut bad_version).await.is_err());
        // root length 0x00ffffff: above UBUS_MAX_MSGLEN
        let mut too_big: &[u8] = &[0, 2, 0, 1, 0, 0, 0, 0, 0, 0xff, 0xff, 0xff];
        assert!(read_message(&mut too_big).await.is_err());
        let mut truncated: &[u8] = &[0, 2, 0, 1, 0, 0, 0, 0, 0, 0, 0, 8, 1];
        assert!(read_message(&mut truncated).await.is_err());
    }
}
