//! The closed set of failure codes and the error every operation returns.
//!
//! A caller (the frontend, a script) may match on every [`Code`]; the set only grows with a version bump of the API.
//! The message is for people: already sanitised, one line, at most [`MESSAGE_MAX`](crate::sanitize::MESSAGE_MAX)
//! bytes.

use std::fmt;

use crate::sanitize::sanitize_message;

/// The machine-readable failure codes of the API.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Code {
    /// The request is not one JSON object of plain values, or is too large.
    BadRequest,
    /// No such object or method.
    UnknownMethod,
    /// The request has a field the method does not take (`field` names it).
    UnknownField,
    /// A field has the wrong type or a value outside its rule (`field` names it).
    BadValue,
    /// A required field is missing (`field` names it); also a dangerous action sent without `confirm`.
    Missing,
    /// Refused on purpose: an AT command that wedges the modem, an uncancellable job.
    Forbidden,
    /// This device or image has no such feature.
    Unsupported,
    /// Something else holds the resource: a job of the same kind, the radio, the AT channel.
    Busy,
    /// The operation did not finish in its time.
    Timeout,
    /// The operation ran and failed; the message says why.
    Failed,
    /// No such thing: a job id, an SMS, a Wi-Fi network.
    NotFound,
}

impl Code {
    pub const fn as_str(self) -> &'static str {
        match self {
            Code::BadRequest => "bad-request",
            Code::UnknownMethod => "unknown-method",
            Code::UnknownField => "unknown-field",
            Code::BadValue => "bad-value",
            Code::Missing => "missing",
            Code::Forbidden => "forbidden",
            Code::Unsupported => "unsupported",
            Code::Busy => "busy",
            Code::Timeout => "timeout",
            Code::Failed => "failed",
            Code::NotFound => "not-found",
        }
    }
}

impl fmt::Display for Code {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A refusal or a failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    code: Code,
    message: String,
    field: Option<&'static str>,
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

impl Error {
    pub fn new(code: Code, message: impl AsRef<str>) -> Self {
        Error { code, message: sanitize_message(message.as_ref()), field: None }
    }

    /// An error about one request field.
    pub fn field(code: Code, field: &'static str, message: impl AsRef<str>) -> Self {
        Error { code, message: sanitize_message(message.as_ref()), field: Some(field) }
    }

    pub fn bad_value(field: &'static str) -> Self {
        Self::field(Code::BadValue, field, format!("{field}: not an accepted value"))
    }

    pub fn missing(field: &'static str) -> Self {
        Self::field(Code::Missing, field, format!("{field} is required"))
    }

    pub fn unsupported(what: impl fmt::Display) -> Self {
        Self::new(Code::Unsupported, format!("not available on this device: {what}"))
    }

    pub fn failed(message: impl AsRef<str>) -> Self {
        let m = message.as_ref();
        Self::new(Code::Failed, if m.trim().is_empty() { "the operation failed" } else { m })
    }

    pub fn busy(message: impl AsRef<str>) -> Self {
        Self::new(Code::Busy, message)
    }

    pub fn timeout(what: impl fmt::Display) -> Self {
        Self::new(Code::Timeout, format!("{what} did not finish in time"))
    }

    pub fn not_found(what: impl fmt::Display) -> Self {
        Self::new(Code::NotFound, format!("no such {what}"))
    }

    pub fn forbidden(message: impl AsRef<str>) -> Self {
        Self::new(Code::Forbidden, message)
    }

    /// An I/O failure, with what was being done ("write /etc/config/wireless").
    pub fn io(context: impl fmt::Display, e: &std::io::Error) -> Self {
        Self::failed(format!("{context}: {e}"))
    }

    pub const fn code(&self) -> Code {
        self.code
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub const fn field_name(&self) -> Option<&'static str> {
        self.field
    }

    /// The same error with "context: " before its message (the code stays).
    #[must_use]
    pub fn context(mut self, context: impl fmt::Display) -> Self {
        self.message = sanitize_message(&format!("{context}: {}", self.message));
        self
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for Error {}

/// Adds context to the error of a `Result` without touching its code.
pub trait ResultExt<T> {
    fn context(self, context: impl fmt::Display) -> Result<T>;
}

impl<T> ResultExt<T> for Result<T> {
    fn context(self, context: impl fmt::Display) -> Result<T> {
        self.map_err(|e| e.context(context))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_are_sanitised_and_keep_their_code() {
        let e = Error::failed("bad\nthing\x1b").context("wifi.set");
        assert_eq!(e.code(), Code::Failed);
        assert_eq!(e.message(), "wifi.set: bad thing");
        assert_eq!(Error::bad_value("ssid").field_name(), Some("ssid"));
        assert_eq!(Error::failed("  ").message(), "the operation failed");
    }
}
