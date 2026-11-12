//! The ubus message: constants from `ubusmsg.h`, the 12-byte header, and the attribute table of a message body.
//!
//! On the wire a message is the 8-byte `struct ubus_msghdr` (u8 version = 0, u8 type, u16 seq BE, u32 peer BE),
//! then one root blob (id 0) whose children are `UBUS_ATTR_*` attributes. Only the root's raw length is sent, without
//! trailing padding (`ubus_send_msg` in `libubus-io.c`); since every child is padded, the two are equal anyway.
//! All constants below were checked against openwrt/ubus `ubusmsg.h` and `CMakeLists.txt` (master).

use serde_json::{Map, Value};

use crate::UbusError;
use crate::blob::{self, Attr, BlobWriter, proto_err};
use crate::blobmsg;

/// `UBUS_MAX_MSGLEN` (set in ubus's CMakeLists.txt): ubusd drops and disconnects a client exceeding it.
pub const MAX_MSG_LEN: usize = 1_048_576;
/// `sizeof(struct ubus_msghdr)` (packed: 1 + 1 + 2 + 4). With the root blob header that follows, the fixed part
/// of a message is 12 bytes.
pub(crate) const HEADER_LEN: usize = 8;
/// `UBUS_CLIENT_ID_CHANNEL`: a hello must give us an id above it (`ubus_reconnect`).
pub(crate) const CLIENT_ID_CHANNEL: u32 = 1;

/// `enum ubus_msg_type`, complete (not every type is sent or handled here).
#[allow(dead_code)]
pub(crate) mod msg {
    pub const HELLO: u8 = 0;
    pub const STATUS: u8 = 1;
    pub const DATA: u8 = 2;
    pub const PING: u8 = 3;
    pub const LOOKUP: u8 = 4;
    pub const INVOKE: u8 = 5;
    pub const ADD_OBJECT: u8 = 6;
    pub const REMOVE_OBJECT: u8 = 7;
    pub const SUBSCRIBE: u8 = 8;
    pub const UNSUBSCRIBE: u8 = 9;
    pub const NOTIFY: u8 = 10;
    pub const MONITOR: u8 = 11;
}

/// `enum ubus_msg_attr`, complete (not every attribute is used here).
#[allow(dead_code)]
pub(crate) mod attr {
    pub const UNSPEC: u8 = 0;
    pub const STATUS: u8 = 1;
    pub const OBJPATH: u8 = 2;
    pub const OBJID: u8 = 3;
    pub const METHOD: u8 = 4;
    pub const OBJTYPE: u8 = 5;
    pub const SIGNATURE: u8 = 6;
    pub const DATA: u8 = 7;
    pub const TARGET: u8 = 8;
    pub const ACTIVE: u8 = 9;
    pub const NO_REPLY: u8 = 10;
    pub const SUBSCRIBERS: u8 = 11;
    pub const USER: u8 = 12;
    pub const GROUP: u8 = 13;
    /// `UBUS_ATTR_MAX`.
    pub const MAX: usize = 14;
}

/// `enum ubus_msg_status`: the codes [`crate::UbusError::Status`] carries.
pub mod status {
    pub const OK: u32 = 0;
    pub const INVALID_COMMAND: u32 = 1;
    pub const INVALID_ARGUMENT: u32 = 2;
    pub const METHOD_NOT_FOUND: u32 = 3;
    pub const NOT_FOUND: u32 = 4;
    pub const NO_DATA: u32 = 5;
    pub const PERMISSION_DENIED: u32 = 6;
    pub const TIMEOUT: u32 = 7;
    pub const NOT_SUPPORTED: u32 = 8;
    pub const UNKNOWN_ERROR: u32 = 9;
    pub const CONNECTION_FAILED: u32 = 10;
    pub const NO_MEMORY: u32 = 11;
    pub const PARSE_ERROR: u32 = 12;
    pub const SYSTEM_ERROR: u32 = 13;
}

/// `struct ubus_msghdr`, in host order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Header {
    pub ty: u8,
    pub seq: u16,
    pub peer: u32,
}

impl Header {
    pub(crate) fn encode(&self) -> [u8; HEADER_LEN] {
        let mut out = [0u8; HEADER_LEN];
        out[1] = self.ty;
        out[2..4].copy_from_slice(&self.seq.to_be_bytes());
        out[4..8].copy_from_slice(&self.peer.to_be_bytes());
        out
    }
}

/// One whole message: header and root blob bytes (root header included).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Message {
    pub header: Header,
    pub body: Vec<u8>,
}

impl Message {
    /// The wire bytes: the `ubus_msghdr` followed by the root blob.
    pub(crate) fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(HEADER_LEN + self.body.len());
        out.extend_from_slice(&self.header.encode());
        out.extend_from_slice(&self.body);
        out
    }

    /// The message's attributes, by `UBUS_ATTR_*` id (`ubus_parse_msg`). Later duplicates win, as in libubox.
    pub(crate) fn attrs(&self) -> Result<Attrs<'_>, UbusError> {
        let (root, _) = Attr::parse(&self.body)?;
        let mut slots: [Option<Attr<'_>>; attr::MAX] = [None; attr::MAX];
        for child in root.children() {
            let child = child?;
            if let Some(slot) = slots.get_mut(usize::from(child.id)) {
                *slot = Some(child);
            }
        }
        Ok(Attrs { slots })
    }
}

/// The parsed attribute table of one message.
#[derive(Debug)]
pub(crate) struct Attrs<'a> {
    slots: [Option<Attr<'a>>; attr::MAX],
}

impl<'a> Attrs<'a> {
    pub(crate) fn get(&self, id: u8) -> Option<Attr<'a>> {
        self.slots.get(usize::from(id)).copied().flatten()
    }

    pub(crate) fn u32(&self, id: u8) -> Option<u32> {
        self.get(id)?.get_u32()
    }

    pub(crate) fn str(&self, id: u8) -> Option<&'a str> {
        self.get(id)?.get_str()
    }

    /// `UBUS_ATTR_DATA` as a JSON object (`None` when absent).
    pub(crate) fn data(&self) -> Result<Option<Map<String, Value>>, UbusError> {
        self.get(attr::DATA).map(|a| blobmsg::decode_fields(a.payload)).transpose()
    }
}

/// Builds a message body: a root blob (id 0) of `UBUS_ATTR_*` attributes.
#[derive(Debug)]
pub(crate) struct Body {
    w: BlobWriter,
    root: blob::Open,
}

impl Body {
    pub(crate) fn new() -> Self {
        let mut w = BlobWriter::new();
        let root = w.start(0, false);
        Body { w, root }
    }

    pub(crate) fn u32(mut self, id: u8, value: u32) -> Result<Self, UbusError> {
        self.w.put_u32(id, value)?;
        Ok(self)
    }

    /// A one-byte attribute (`blob_put_int8`), as callers put `UBUS_ATTR_NO_REPLY`.
    #[cfg(test)]
    pub(crate) fn u8(mut self, id: u8, value: u8) -> Result<Self, UbusError> {
        self.w.put(id, &[value])?;
        Ok(self)
    }

    pub(crate) fn string(mut self, id: u8, value: &str) -> Result<Self, UbusError> {
        if value.as_bytes().contains(&0) {
            return Err(proto_err("string attribute contains NUL"));
        }
        self.w.put_string(id, value)?;
        Ok(self)
    }

    /// `UBUS_ATTR_DATA` holding the fields of `map` as blobmsg (`ubus_put_data`).
    pub(crate) fn data(mut self, map: &Map<String, Value>) -> Result<Self, UbusError> {
        let open = self.w.start(attr::DATA, false);
        blobmsg::put_table_fields(&mut self.w, map, 0)?;
        self.w.end(open)?;
        Ok(self)
    }

    /// A nested attribute `id` whose contents `fill` writes (used for `UBUS_ATTR_SIGNATURE`).
    pub(crate) fn nested(
        mut self,
        id: u8,
        fill: impl FnOnce(&mut BlobWriter) -> Result<(), UbusError>,
    ) -> Result<Self, UbusError> {
        let open = self.w.start(id, false);
        fill(&mut self.w)?;
        self.w.end(open)?;
        Ok(self)
    }

    /// Finishes the root and frames the message; refuses bodies over [`MAX_MSG_LEN`] as libubus does.
    pub(crate) fn finish(mut self, ty: u8, seq: u16, peer: u32) -> Result<Message, UbusError> {
        self.w.end(self.root)?;
        if self.w.len() > MAX_MSG_LEN {
            return Err(proto_err("message larger than UBUS_MAX_MSGLEN"));
        }
        Ok(Message { header: Header { ty, seq, peer }, body: self.w.into_bytes() })
    }
}

/// A `STATUS` message (`ubus_complete_deferred_request`): STATUS and, from an object, its OBJID.
pub(crate) fn status_msg(seq: u16, peer: u32, code: u32, objid: Option<u32>) -> Result<Message, UbusError> {
    let mut body = Body::new().u32(attr::STATUS, code)?;
    if let Some(id) = objid {
        body = body.u32(attr::OBJID, id)?;
    }
    body.finish(msg::STATUS, seq, peer)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn hello_from_ubusd() -> Result<(), UbusError> {
        // ubusd_send_hello: empty root, seq 0, peer = the client's id
        let m = Body::new().finish(msg::HELLO, 0, 0x1234_abcd)?;
        assert_eq!(m.to_bytes(), vec![0, 0, 0, 0, 0x12, 0x34, 0xab, 0xcd, 0, 0, 0, 4]);
        Ok(())
    }

    #[test]
    fn lookup_request() -> Result<(), UbusError> {
        let m = Body::new().string(attr::OBJPATH, "f50")?.finish(msg::LOOKUP, 1, 0)?;
        assert_eq!(m.to_bytes(), vec![0, 4, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0x0c, 0x02, 0, 0, 0x08, b'f', b'5', b'0', 0]);
        Ok(())
    }

    #[test]
    fn status_from_object() -> Result<(), UbusError> {
        let m = status_msg(7, 0x55, status::OK, Some(0x0a0b_0c0d))?;
        assert_eq!(
            m.to_bytes(),
            vec![
                0, 1, 0, 7, 0, 0, 0, 0x55, // header
                0, 0, 0, 0x14, // root, 4 + 8 + 8
                0x01, 0, 0, 0x08, 0, 0, 0, 0, // STATUS 0
                0x03, 0, 0, 0x08, 0x0a, 0x0b, 0x0c, 0x0d, // OBJID
            ]
        );
        let attrs = m.attrs()?;
        assert_eq!(attrs.u32(attr::STATUS), Some(0));
        assert_eq!(attrs.u32(attr::OBJID), Some(0x0a0b_0c0d));
        Ok(())
    }

    #[test]
    fn invoke_with_data_round_trip() -> Result<(), UbusError> {
        let Value::Object(args) = json!({"n": 1}) else { unreachable!("literal is an object") };
        let m = Body::new().u32(attr::OBJID, 2)?.string(attr::METHOD, "s")?.data(&args)?.finish(msg::INVOKE, 3, 2)?;
        assert_eq!(
            m.to_bytes(),
            vec![
                0, 5, 0, 3, 0, 0, 0, 2, // header
                0, 0, 0, 0x24, // root: 4 + 8 + 8 + 16
                0x03, 0, 0, 0x08, 0, 0, 0, 2, // OBJID 2
                0x04, 0, 0, 0x06, b's', 0, 0, 0, // METHOD "s"
                0x07, 0, 0, 0x10, // DATA, plain (non-extended) blob
                0x85, 0, 0, 0x0c, 0, 1, b'n', 0, 0, 0, 0, 1, // "n": INT32 1
            ]
        );
        let attrs = m.attrs()?;
        assert_eq!(attrs.str(attr::METHOD), Some("s"));
        assert_eq!(attrs.data()?, Some(args));
        Ok(())
    }

    #[test]
    fn rejects_oversized() {
        let big = "x".repeat(MAX_MSG_LEN);
        let r = Body::new().string(attr::OBJPATH, &big).and_then(|b| b.finish(msg::LOOKUP, 1, 0));
        assert!(r.is_err());
    }
}
