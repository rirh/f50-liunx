//! blobmsg (`libubox/blobmsg.h`): named, typed blob attributes, and their conversion to and from JSON.
//!
//! A blobmsg attribute is an *extended* blob attribute whose id is the type. After the 4-byte header comes a
//! `u16` big-endian name length, the name, a NUL, zero padding to 4 (`blobmsg_hdrlen`), then the value. Array
//! elements carry an empty name. Types (`enum blobmsg_type`): UNSPEC 0, ARRAY 1, TABLE 2, STRING 3, INT64 4,
//! INT32 5, INT16 6, INT8 7 (= BOOL), DOUBLE 8. Integers and doubles are big-endian; strings end in NUL.
//!
//! JSON mapping follows libubox's `blobmsg_json.c`, which is what `ubus call` and uhttpd use: `null` is UNSPEC
//! without data, booleans are BOOL, integers are INT32 when they fit and INT64 otherwise, other numbers DOUBLE.
//! Decoding is lenient where JSON has no counterpart: a non-finite DOUBLE becomes `null`, invalid UTF-8 is replaced.

use serde_json::{Map, Number, Value};

use crate::UbusError;
use crate::blob::{Attr, BlobWriter, Children, proto_err};

pub(crate) const TYPE_UNSPEC: u8 = 0;
pub(crate) const TYPE_ARRAY: u8 = 1;
pub(crate) const TYPE_TABLE: u8 = 2;
pub(crate) const TYPE_STRING: u8 = 3;
pub(crate) const TYPE_INT64: u8 = 4;
pub(crate) const TYPE_INT32: u8 = 5;
pub(crate) const TYPE_INT16: u8 = 6;
/// `BLOBMSG_TYPE_INT8`, which is also `BLOBMSG_TYPE_BOOL`.
pub(crate) const TYPE_INT8: u8 = 7;
pub(crate) const TYPE_BOOL: u8 = TYPE_INT8;
pub(crate) const TYPE_DOUBLE: u8 = 8;

/// Nesting deeper than this is refused: a 1 MiB message could otherwise nest ~260k tables and exhaust the stack.
pub(crate) const MAX_DEPTH: usize = 64;

/// Writes the blobmsg header of a new attribute: name length, name, NUL, padding (`blobmsg_new`).
pub(crate) fn start_named(w: &mut BlobWriter, ty: u8, name: &str) -> Result<crate::blob::Open, UbusError> {
    let len = u16::try_from(name.len()).map_err(|_| proto_err("blobmsg name longer than 65535 bytes"))?;
    if name.as_bytes().contains(&0) {
        return Err(proto_err("blobmsg name contains NUL"));
    }
    let open = w.start(ty, true);
    w.put_bytes(&len.to_be_bytes());
    w.put_bytes(name.as_bytes());
    w.put_bytes(&[0]);
    w.align();
    Ok(open)
}

/// Appends `value` as one blobmsg attribute named `name`.
pub(crate) fn put_value(w: &mut BlobWriter, name: &str, value: &Value) -> Result<(), UbusError> {
    put_value_depth(w, name, value, 0)
}

fn put_value_depth(w: &mut BlobWriter, name: &str, value: &Value, depth: usize) -> Result<(), UbusError> {
    if depth > MAX_DEPTH {
        return Err(proto_err("JSON nested too deeply for blobmsg"));
    }
    match value {
        Value::Null => {
            let open = start_named(w, TYPE_UNSPEC, name)?;
            w.end(open)
        }
        Value::Bool(b) => {
            let open = start_named(w, TYPE_BOOL, name)?;
            w.put_bytes(&[u8::from(*b)]);
            w.end(open)
        }
        Value::Number(n) => put_number(w, name, n),
        Value::String(s) => {
            if s.as_bytes().contains(&0) {
                return Err(proto_err("string value contains NUL"));
            }
            let open = start_named(w, TYPE_STRING, name)?;
            w.put_bytes(s.as_bytes());
            w.put_bytes(&[0]);
            w.end(open)
        }
        Value::Array(items) => {
            let open = start_named(w, TYPE_ARRAY, name)?;
            for item in items {
                put_value_depth(w, "", item, depth + 1)?;
            }
            w.end(open)
        }
        Value::Object(map) => {
            let open = start_named(w, TYPE_TABLE, name)?;
            put_table_fields(w, map, depth + 1)?;
            w.end(open)
        }
    }
}

fn put_number(w: &mut BlobWriter, name: &str, n: &Number) -> Result<(), UbusError> {
    if let Some(i) = n.as_i64() {
        if let Ok(small) = i32::try_from(i) {
            let open = start_named(w, TYPE_INT32, name)?;
            w.put_bytes(&small.to_be_bytes());
            return w.end(open);
        }
        let open = start_named(w, TYPE_INT64, name)?;
        w.put_bytes(&i.to_be_bytes());
        return w.end(open);
    }
    // u64 above i64::MAX and every float: libubox has no unsigned 64-bit type, so DOUBLE (as blobmsg_json does)
    let f = n.as_f64().unwrap_or(0.0);
    let open = start_named(w, TYPE_DOUBLE, name)?;
    w.put_bytes(&f.to_bits().to_be_bytes());
    w.end(open)
}

/// Appends every field of `map` as named attributes (the inside of a table, or of a message's DATA).
pub(crate) fn put_table_fields(w: &mut BlobWriter, map: &Map<String, Value>, depth: usize) -> Result<(), UbusError> {
    for (k, v) in map {
        put_value_depth(w, k, v, depth)?;
    }
    Ok(())
}

/// Encodes the fields of a JSON object as a bare sequence of blobmsg attributes (no enclosing table header).
#[cfg(test)]
pub(crate) fn encode_fields(map: &Map<String, Value>) -> Result<Vec<u8>, UbusError> {
    let mut w = BlobWriter::new();
    put_table_fields(&mut w, map, 0)?;
    Ok(w.into_bytes())
}

/// One decoded blobmsg attribute: its name and value.
fn decode_attr(attr: &Attr<'_>, depth: usize) -> Result<(String, Value), UbusError> {
    if !attr.extended {
        return Err(proto_err("blobmsg attribute without a name header"));
    }
    let (name, data) = split_name(attr.payload)?;
    Ok((name, decode_data(attr.id, data, depth)?))
}

/// Splits an extended payload into its name and value bytes (`blobmsg_name`, `blobmsg_data`).
fn split_name(payload: &[u8]) -> Result<(String, &[u8]), UbusError> {
    let len_bytes: [u8; 2] =
        payload.get(..2).and_then(|b| b.try_into().ok()).ok_or_else(|| proto_err("blobmsg name header truncated"))?;
    let namelen = usize::from(u16::from_be_bytes(len_bytes));
    let name = payload.get(2..2 + namelen).ok_or_else(|| proto_err("blobmsg name past the attribute"))?;
    if payload.get(2 + namelen) != Some(&0) {
        return Err(proto_err("blobmsg name not NUL-terminated"));
    }
    let hdrlen = crate::blob::pad(2 + namelen + 1);
    let data = payload.get(hdrlen..).ok_or_else(|| proto_err("blobmsg header past the attribute"))?;
    Ok((String::from_utf8_lossy(name).into_owned(), data))
}

fn fixed<const N: usize>(data: &[u8]) -> Result<[u8; N], UbusError> {
    data.get(..N).and_then(|b| b.try_into().ok()).ok_or_else(|| proto_err("blobmsg value shorter than its type"))
}

fn decode_data(ty: u8, data: &[u8], depth: usize) -> Result<Value, UbusError> {
    if depth > MAX_DEPTH {
        return Err(proto_err("blobmsg nested too deeply"));
    }
    Ok(match ty {
        TYPE_UNSPEC => Value::Null,
        TYPE_ARRAY => {
            let mut items = Vec::new();
            for child in Children::new(data) {
                items.push(decode_attr(&child?, depth + 1)?.1);
            }
            Value::Array(items)
        }
        TYPE_TABLE => Value::Object(decode_fields_depth(data, depth + 1)?),
        TYPE_STRING => {
            let end = data.iter().position(|&b| b == 0).unwrap_or(data.len());
            Value::String(String::from_utf8_lossy(data.get(..end).unwrap_or_default()).into_owned())
        }
        TYPE_INT64 => Value::from(i64::from_be_bytes(fixed::<8>(data)?)),
        TYPE_INT32 => Value::from(i32::from_be_bytes(fixed::<4>(data)?)),
        TYPE_INT16 => Value::from(i16::from_be_bytes(fixed::<2>(data)?)),
        TYPE_BOOL => Value::Bool(fixed::<1>(data)?[0] != 0),
        TYPE_DOUBLE => {
            let f = f64::from_bits(u64::from_be_bytes(fixed::<8>(data)?));
            Number::from_f64(f).map_or(Value::Null, Value::Number)
        }
        other => return Err(UbusError::Protocol(format!("unknown blobmsg type {other}"))),
    })
}

fn decode_fields_depth(data: &[u8], depth: usize) -> Result<Map<String, Value>, UbusError> {
    let mut map = Map::new();
    for child in Children::new(data) {
        let (name, value) = decode_attr(&child?, depth)?;
        map.insert(name, value);
    }
    Ok(map)
}

/// Decodes a bare sequence of blobmsg attributes (a table's inside, or a message's DATA) into a JSON object.
pub(crate) fn decode_fields(data: &[u8]) -> Result<Map<String, Value>, UbusError> {
    decode_fields_depth(data, 0)
}

#[cfg(test)]
#[path = "blobmsg_tests.rs"]
mod tests;
