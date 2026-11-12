//! Byte-exact blobmsg vectors, built by hand from `blobmsg_new`/`blobmsg_hdrlen` in libubox.

use serde_json::{Map, Value, json};

use super::*;

fn one(name: &str, value: Value) -> Map<String, Value> {
    let mut m = Map::new();
    m.insert(name.to_owned(), value);
    m
}

fn round_trip(name: &str, value: Value, bytes: &[u8]) -> Result<(), UbusError> {
    let map = one(name, value);
    assert_eq!(encode_fields(&map)?, bytes, "encode {map:?}");
    assert_eq!(decode_fields(bytes)?, map, "decode {bytes:02x?}");
    Ok(())
}

#[test]
fn string() -> Result<(), UbusError> {
    // raw length 4 + hdrlen 4 ("\0\x01a\0") + "b\0" = 10, padded to 12
    round_trip("a", json!("b"), &[0x83, 0, 0, 0x0a, 0, 1, b'a', 0, b'b', 0, 0, 0])
}

#[test]
fn long_name_header_pads_to_eight() -> Result<(), UbusError> {
    // hdrlen = pad(2 + 3 + 1) = 8
    round_trip("abc", json!(""), &[0x83, 0, 0, 0x0d, 0, 3, b'a', b'b', b'c', 0, 0, 0, 0, 0, 0, 0])
}

#[test]
fn int32() -> Result<(), UbusError> {
    round_trip("n", json!(1), &[0x85, 0, 0, 0x0c, 0, 1, b'n', 0, 0, 0, 0, 1])
}

#[test]
fn negative_int32() -> Result<(), UbusError> {
    round_trip("n", json!(-2), &[0x85, 0, 0, 0x0c, 0, 1, b'n', 0, 0xff, 0xff, 0xff, 0xfe])
}

#[test]
fn int64_when_out_of_i32_range() -> Result<(), UbusError> {
    round_trip("x", json!(4_294_967_296_i64), &[0x84, 0, 0, 0x10, 0, 1, b'x', 0, 0, 0, 0, 1, 0, 0, 0, 0])
}

#[test]
fn bool_is_int8() -> Result<(), UbusError> {
    round_trip("t", json!(true), &[0x87, 0, 0, 0x09, 0, 1, b't', 0, 1, 0, 0, 0])?;
    round_trip("f", json!(false), &[0x87, 0, 0, 0x09, 0, 1, b'f', 0, 0, 0, 0, 0])
}

#[test]
fn double() -> Result<(), UbusError> {
    round_trip("d", json!(1.5), &[0x88, 0, 0, 0x10, 0, 1, b'd', 0, 0x3f, 0xf8, 0, 0, 0, 0, 0, 0])
}

#[test]
fn null_is_unspec_without_data() -> Result<(), UbusError> {
    round_trip("z", Value::Null, &[0x80, 0, 0, 0x08, 0, 1, b'z', 0])
}

#[test]
fn empty_table() -> Result<(), UbusError> {
    round_trip("o", json!({}), &[0x82, 0, 0, 0x08, 0, 1, b'o', 0])
}

#[test]
fn array_elements_have_empty_names() -> Result<(), UbusError> {
    round_trip(
        "l",
        json!([1]),
        &[
            0x81, 0, 0, 0x14, 0, 1, b'l', 0, // array header, raw length 20
            0x85, 0, 0, 0x0c, 0, 0, 0, 0, 0, 0, 0, 1, // INT32 element, name ""
        ],
    )
}

#[test]
fn nested_table() -> Result<(), UbusError> {
    round_trip(
        "o",
        json!({"s": "hi"}),
        &[
            0x82, 0, 0, 0x14, 0, 1, b'o', 0, // table, raw 4 + 4 + 12
            0x83, 0, 0, 0x0b, 0, 1, b's', 0, b'h', b'i', 0, 0, // string, raw 11, padded 12
        ],
    )
}

#[test]
fn int16_decodes_as_number() -> Result<(), UbusError> {
    let bytes = [0x86, 0, 0, 0x0a, 0, 1, b'w', 0, 0x01, 0x00, 0, 0];
    assert_eq!(decode_fields(&bytes)?, one("w", json!(256)));
    Ok(())
}

#[test]
fn several_fields_keep_order_of_map() -> Result<(), UbusError> {
    let value = json!({"a": 1, "b": true, "c": [null, "x"], "d": {"e": -1.25}});
    let Value::Object(map) = value else { unreachable!("literal is an object") };
    assert_eq!(decode_fields(&encode_fields(&map)?)?, map);
    Ok(())
}

#[test]
fn rejects_unnamed_table_member() {
    // non-extended INT32 inside a table: libubox rejects it as well (blobmsg_check_name)
    assert!(decode_fields(&[0x05, 0, 0, 0x08, 0, 0, 0, 1]).is_err());
}

#[test]
fn rejects_truncated_value() {
    // INT32 with only two data bytes
    assert!(decode_fields(&[0x85, 0, 0, 0x0a, 0, 1, b'n', 0, 0, 1, 0, 0]).is_err());
}

#[test]
fn rejects_name_without_nul() {
    assert!(decode_fields(&[0x83, 0, 0, 0x08, 0, 2, b'a', b'b']).is_err());
}

#[test]
fn rejects_unknown_type() {
    assert!(decode_fields(&[0x89, 0, 0, 0x08, 0, 1, b'q', 0]).is_err());
}

#[test]
fn rejects_deep_nesting() -> Result<(), UbusError> {
    let mut v = json!(1);
    for _ in 0..(MAX_DEPTH + 2) {
        v = json!([v]);
    }
    assert!(encode_fields(&one("deep", v)).is_err());
    Ok(())
}

#[test]
fn rejects_nul_in_string() {
    assert!(encode_fields(&one("s", json!("a\u{0}b"))).is_err());
}
