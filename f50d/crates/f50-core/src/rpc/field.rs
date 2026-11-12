//! What a request field may be.

/// The field uhttpd's ubus bridge adds to every call; never one of ours, never refused.
pub const UBUS_SESSION: &str = "ubus_rpc_session";

/// The rule for one field's value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rule {
    /// A JSON string of printable ASCII matching `^(?:re)$`, at most `max` bytes.
    Word { re: &'static str, max: usize },
    /// As `Word`, and a JSON number too (`1073741824` as well as `"1073741824"`).
    WordNum { re: &'static str, max: usize },
    /// One line of UTF-8 text, no control character, at most `max_bytes` bytes and `max_chars` characters.
    Text { max_bytes: usize, max_chars: usize },
    /// Text that may hold newlines and tabs (an SMS), not empty, at most `max_chars` characters.
    Body { max_chars: usize },
    /// A whole number in a range: a JSON number, or a string of digits.
    Int { min: i64, max: i64 },
    /// `true`/`false` (or `1`/`0`).
    Bool,
    /// One of these words.
    Enum(&'static [&'static str]),
}

/// The type a field is declared with in the ubus method signature (what `ubus -v list` shows).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WireType {
    String,
    Int,
    Bool,
}

impl Rule {
    pub const fn wire_type(self) -> WireType {
        match self {
            Rule::Int { .. } => WireType::Int,
            Rule::Bool => WireType::Bool,
            _ => WireType::String,
        }
    }
}

/// One field of a method's request.
#[derive(Debug, Clone, Copy)]
pub struct Field {
    pub name: &'static str,
    pub rule: Rule,
    pub required: bool,
    /// A secret (a password, SMS text, an AT command): it only ever goes to a program's stdin or to the modem, is
    /// never logged or echoed back, and may start with "-".
    pub secret: bool,
}

impl Field {
    pub const fn opt(name: &'static str, rule: Rule) -> Self {
        Field { name, rule, required: false, secret: false }
    }
    pub const fn req(name: &'static str, rule: Rule) -> Self {
        Field { name, rule, required: true, secret: false }
    }
    pub const fn secret(name: &'static str, rule: Rule, required: bool) -> Self {
        Field { name, rule, required, secret: true }
    }
}
