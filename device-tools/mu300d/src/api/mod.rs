//! The API contract: the envelope, the error codes, field validation, and the method registry.

pub mod error;
pub mod registry;
pub mod validate;

use serde_json::{Value, json};

use error::ApiResult;

/// The envelope every method answers with:
/// `{"ok":true,"data":{...}}` or `{"ok":false,"code":"...","error":"...","field":"..."}`.
pub fn envelope(r: &ApiResult<Value>) -> Value {
    match r {
        Ok(data) => json!({ "ok": true, "data": data }),
        Err(e) => {
            let mut v = json!({ "ok": false, "code": e.code.as_str(), "error": e.error });
            if let Some(f) = &e.field {
                v["field"] = json!(f);
            }
            v
        }
    }
}

/// The largest request accepted (bytes), as mu300-api's INPUT_MAX.
pub const REQUEST_MAX: usize = 64 * 1024;

/// Parse a request body: empty means `{}`; anything but one JSON object is `bad-request`.
pub fn parse_request(body: &[u8]) -> ApiResult<Value> {
    use error::{ApiError, Code};
    if body.len() > REQUEST_MAX {
        return Err(ApiError::new(Code::BadRequest, "the request is too large"));
    }
    if body.iter().all(u8::is_ascii_whitespace) {
        return Ok(json!({}));
    }
    match serde_json::from_slice::<Value>(body) {
        Ok(v @ Value::Object(_)) => Ok(v),
        _ => Err(ApiError::new(Code::BadRequest, "the request is not one JSON object")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use error::{ApiError, Code};

    #[test]
    fn envelope_shapes() {
        assert_eq!(envelope(&Ok(json!({"a":1}))), json!({"ok":true,"data":{"a":1}}));
        let e = ApiError::field(Code::BadValue, "ssid", "bad");
        assert_eq!(envelope(&Err(e)), json!({"ok":false,"code":"bad-value","error":"bad","field":"ssid"}));
        assert_eq!(envelope(&Err(ApiError::busy("x"))), json!({"ok":false,"code":"busy","error":"x"}));
    }

    #[test]
    fn requests() {
        assert_eq!(parse_request(b"").unwrap(), json!({}));
        assert_eq!(parse_request(b" {\"a\":1} ").unwrap(), json!({"a":1}));
        assert!(parse_request(b"[1]").is_err());
        assert!(parse_request(b"{} {}").is_err());
        assert!(parse_request(&vec![b' '; REQUEST_MAX + 1]).is_err());
    }
}
