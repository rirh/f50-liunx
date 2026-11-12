//! Calls to other ubus objects through the [`Platform`]'s client, with ubus failures mapped to f50d's codes.
//!
//! Without a client (tests, or before ubusd is up) every call answers `unsupported`.

use std::time::Duration;

use f50_core::{Code, Error, Result};
use f50_ubus::UbusError;
use serde_json::Value;

use crate::Platform;

/// How long one call to another object may take (netifd and procd answer in milliseconds; a reload returns before
/// the work it starts is done).
pub const CALL_TIMEOUT: Duration = Duration::from_secs(10);

/// UBUS_STATUS_* codes f50d tells apart (libubus `ubusmsg.h`).
const STATUS_INVALID_ARGUMENT: u32 = 2;
const STATUS_METHOD_NOT_FOUND: u32 = 3;
const STATUS_NOT_FOUND: u32 = 4;
const STATUS_PERMISSION_DENIED: u32 = 6;
const STATUS_TIMEOUT: u32 = 7;
const STATUS_NOT_SUPPORTED: u32 = 8;

impl Platform {
    /// `ubus call <object> <method> <args>`: the reply's data. A missing object is `not-found`, a missing method
    /// `unsupported`, no client `unsupported`.
    pub async fn call(&self, object: &str, method: &str, args: &Value) -> Result<Value> {
        let client = self.ubus().ok_or_else(|| Error::unsupported("ubus"))?;
        client.call(object, method, args, CALL_TIMEOUT).await.map_err(|e| map_error(object, method, e))
    }

    /// Like [`call`](Self::call), but `None` when the object is not registered (an interface netifd does not
    /// have, a service procd does not know).
    pub async fn call_opt(&self, object: &str, method: &str, args: &Value) -> Result<Option<Value>> {
        match self.call(object, method, args).await {
            Ok(v) => Ok(Some(v)),
            Err(e) if e.code() == Code::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// The object is registered on the bus now.
    pub async fn has_object(&self, object: &str) -> Result<bool> {
        let client = self.ubus().ok_or_else(|| Error::unsupported("ubus"))?;
        client.lookup(object).await.map(|id| id.is_some()).map_err(|e| map_error(object, "lookup", e))
    }
}

/// A ubus failure as f50d's error.
pub(crate) fn map_error(object: &str, method: &str, e: UbusError) -> Error {
    match e {
        UbusError::Timeout | UbusError::Status(STATUS_TIMEOUT) => Error::timeout(format!("ubus {object} {method}")),
        UbusError::Status(STATUS_NOT_FOUND) => Error::not_found(format!("ubus object {object}")),
        UbusError::Status(STATUS_METHOD_NOT_FOUND | STATUS_NOT_SUPPORTED) => {
            Error::unsupported(format!("ubus {object} {method}"))
        }
        UbusError::Status(STATUS_PERMISSION_DENIED) => Error::forbidden(format!("ubus {object} {method}: denied")),
        UbusError::Status(STATUS_INVALID_ARGUMENT) => {
            Error::failed(format!("ubus {object} {method}: invalid argument"))
        }
        other => Error::failed(format!("ubus {object} {method}: {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statuses_map_to_codes() {
        assert_eq!(map_error("o", "m", UbusError::Status(4)).code(), Code::NotFound);
        assert_eq!(map_error("o", "m", UbusError::Status(3)).code(), Code::Unsupported);
        assert_eq!(map_error("o", "m", UbusError::Timeout).code(), Code::Timeout);
        assert_eq!(map_error("o", "m", UbusError::Status(9)).code(), Code::Failed);
    }

    #[tokio::test]
    async fn without_a_client_calls_are_unsupported() {
        let p = Platform::new(f50_core::paths::Paths::new("/nonexistent"), None);
        let e = p.call("network", "reload", &Value::Null).await.expect_err("no client");
        assert_eq!(e.code(), Code::Unsupported);
    }
}
