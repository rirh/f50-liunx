//! The fixed error codes and the error every method returns.

use std::fmt;

use crate::sanitize::sanitize_message;

/// The machine-readable failure codes of the API. The set is closed: a client may match on every one of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Code {
    /// The request is not one JSON object of plain values, or is too large.
    BadRequest,
    /// No such method.
    UnknownMethod,
    /// The request has a field the method does not take (`field` names it).
    UnknownField,
    /// A field has the wrong type or a value outside its rule (`field` names it).
    BadValue,
    /// A required field is missing (`field` names it).
    Missing,
    /// The caller has no session (HTTP) or is not root (`call`).
    Unauthorized,
    /// The request is understood and refused on purpose (an AT command that wedges the modem, login throttling).
    Forbidden,
    /// This device or system has no such feature, or the tool is not installed.
    Unsupported,
    /// Something else holds the resource (a job of the same kind, the AT channel, a modem restart).
    Busy,
    /// The tool did not finish in its time.
    Timeout,
    /// The tool ran and failed; `error` is its last message, sanitised.
    Failed,
    /// No such object (a job id, an SMS).
    NotFound,
}

impl Code {
    pub fn as_str(self) -> &'static str {
        match self {
            Code::BadRequest => "bad-request",
            Code::UnknownMethod => "unknown-method",
            Code::UnknownField => "unknown-field",
            Code::BadValue => "bad-value",
            Code::Missing => "missing",
            Code::Unauthorized => "unauthorized",
            Code::Forbidden => "forbidden",
            Code::Unsupported => "unsupported",
            Code::Busy => "busy",
            Code::Timeout => "timeout",
            Code::Failed => "failed",
            Code::NotFound => "not-found",
        }
    }

    /// The HTTP status `serve` answers with.
    pub fn http_status(self) -> u16 {
        match self {
            Code::BadRequest | Code::UnknownField | Code::BadValue | Code::Missing => 400,
            Code::Unauthorized => 401,
            Code::Forbidden => 403,
            Code::UnknownMethod | Code::NotFound => 404,
            Code::Busy => 409,
            Code::Failed => 500,
            Code::Unsupported => 501,
            Code::Timeout => 504,
        }
    }
}

/// A refusal or a failure: a code, a human sentence (already sanitised: no control characters, no URL paths, at
/// most 200 bytes), and maybe the field it is about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiError {
    pub code: Code,
    pub error: String,
    pub field: Option<String>,
}

impl ApiError {
    pub fn new(code: Code, error: impl AsRef<str>) -> Self {
        ApiError { code, error: sanitize_message(error.as_ref()), field: None }
    }

    pub fn field(code: Code, field: &str, error: impl AsRef<str>) -> Self {
        ApiError { code, error: sanitize_message(error.as_ref()), field: Some(field.to_owned()) }
    }

    pub fn bad_value(field: &str) -> Self {
        Self::field(Code::BadValue, field, format!("{field}: not an accepted value"))
    }

    pub fn missing(field: &str) -> Self {
        Self::field(Code::Missing, field, format!("{field} is required"))
    }

    pub fn unsupported(what: &str) -> Self {
        Self::new(Code::Unsupported, format!("not available on this device: {what}"))
    }

    pub fn failed(msg: impl AsRef<str>) -> Self {
        let m = msg.as_ref();
        Self::new(Code::Failed, if m.trim().is_empty() { "the tool failed" } else { m })
    }

    pub fn busy(msg: impl AsRef<str>) -> Self {
        Self::new(Code::Busy, msg)
    }
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code.as_str(), self.error)
    }
}

impl std::error::Error for ApiError {}

pub type ApiResult<T> = Result<T, ApiError>;
