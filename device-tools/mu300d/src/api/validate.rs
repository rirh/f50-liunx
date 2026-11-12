//! Typed request fields: every field of every method has a rule, checked before anything runs.
//!
//! The rules follow mu300-api's getters (`word`, `wnum`, `int`, `flag`, `text`, `only`). Two rules hold for every
//! method on top of them: an unknown field is refused (`ubus_rpc_session` excepted), and a string that is not a
//! secret may not start with "-" (it may reach a tool's command line, where it would read as an option).

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};

use regex::Regex;
use serde_json::{Map, Value};

use super::error::{ApiError, ApiResult, Code};

/// The field OpenWrt's ubus over HTTP adds to every call; never one of ours, never refused.
pub const UBUS_SESSION: &str = "ubus_rpc_session";

/// What a field may be.
#[derive(Debug, Clone, Copy)]
pub enum Rule {
    /// A JSON string of printable ASCII matching `^(?:re)$`, at most `max` bytes.
    Word { re: &'static str, max: usize },
    /// As `Word`, and a JSON number too (byte counts: `1073741824` as well as `"1073741824"`).
    WordNum { re: &'static str, max: usize },
    /// One line of text (UTF-8), no control character, at most `max_bytes` bytes and `max_chars` characters.
    Text { max_bytes: usize, max_chars: usize },
    /// Text that may hold newlines and tabs (an SMS), at most `max_chars` characters, not empty.
    Body { max_chars: usize },
    /// A whole number in a range: a JSON number, or a string of digits.
    Int { min: i64, max: i64 },
    /// true/false (or 1/0).
    Bool,
    /// One of these words.
    Enum(&'static [&'static str]),
    /// A string or a number, kept as text and checked by the method itself (its rule depends on another field).
    Scalar { max: usize },
}

/// One field of a method's request.
#[derive(Debug, Clone, Copy)]
pub struct Field {
    pub name: &'static str,
    pub rule: Rule,
    pub required: bool,
    /// The value is a secret: it only ever goes to a tool's stdin, and it may start with "-".
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

/// A checked value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Param {
    Str(String),
    Int(i64),
    Bool(bool),
}

/// The checked request.
#[derive(Debug, Default, Clone)]
pub struct Params {
    values: HashMap<&'static str, Param>,
}

impl Params {
    pub fn str(&self, name: &str) -> Option<&str> {
        match self.values.get(name) {
            Some(Param::Str(s)) => Some(s),
            _ => None,
        }
    }
    pub fn int(&self, name: &str) -> Option<i64> {
        match self.values.get(name) {
            Some(Param::Int(i)) => Some(*i),
            _ => None,
        }
    }
    pub fn bool(&self, name: &str) -> Option<bool> {
        match self.values.get(name) {
            Some(Param::Bool(b)) => Some(*b),
            _ => None,
        }
    }
    pub fn flag(&self, name: &str) -> bool {
        self.bool(name).unwrap_or(false)
    }
    pub fn has(&self, name: &str) -> bool {
        self.values.contains_key(name)
    }
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
    /// For tests and for building requests inside the daemon.
    pub fn insert(&mut self, name: &'static str, p: Param) {
        self.values.insert(name, p);
    }
}

static CACHE: LazyLock<Mutex<HashMap<&'static str, Arc<Regex>>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// `s` matches `^(?:re)$` (patterns are compile-time constants of this crate).
pub fn matches(re: &'static str, s: &str) -> bool {
    let rx = {
        let mut c = CACHE.lock().unwrap_or_else(|e| e.into_inner());
        c.entry(re)
            .or_insert_with(|| Arc::new(Regex::new(&format!("^(?:{re})$")).expect("static field pattern")))
            .clone()
    };
    rx.is_match(s)
}

/// One line of printable ASCII.
pub fn is_ascii_line(s: &str) -> bool {
    s.bytes().all(|b| (0x20..=0x7e).contains(&b))
}

/// One line of text: no control character (UTF-8 allowed).
pub fn is_text_line(s: &str) -> bool {
    !s.chars().any(char::is_control)
}

/// `value` checked as a word by the method itself (a rule chosen by another field).
pub fn check_word(field: &str, value: &str, re: &'static str, max: usize) -> ApiResult<()> {
    if value.len() <= max && is_ascii_line(value) && !value.starts_with('-') && matches(re, value) {
        Ok(())
    } else {
        Err(ApiError::bad_value(field))
    }
}

fn scalar_text(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) if n.is_u64() || n.is_i64() => Some(n.to_string()),
        _ => None,
    }
}

fn check_one(f: &Field, v: &Value) -> Option<Param> {
    let s_ok = |s: &str| f.secret || !s.starts_with('-');
    match f.rule {
        Rule::Word { re, max } => {
            let s = v.as_str()?;
            (s.len() <= max && is_ascii_line(s) && s_ok(s) && matches(re, s)).then(|| Param::Str(s.to_owned()))
        }
        Rule::WordNum { re, max } => {
            let s = scalar_text(v)?;
            (s.len() <= max && is_ascii_line(&s) && s_ok(&s) && matches(re, &s)).then_some(Param::Str(s))
        }
        Rule::Text { max_bytes, max_chars } => {
            let s = v.as_str()?;
            (s.len() <= max_bytes && s.chars().count() <= max_chars && is_text_line(s) && s_ok(s))
                .then(|| Param::Str(s.to_owned()))
        }
        Rule::Body { max_chars } => {
            let s = v.as_str()?;
            let ok_chars = s.chars().all(|c| c == '\n' || c == '\t' || !c.is_control());
            (!s.is_empty() && s.chars().count() <= max_chars && ok_chars && s_ok(s)).then(|| Param::Str(s.to_owned()))
        }
        Rule::Int { min, max } => {
            let n = match v {
                Value::Number(n) => n.as_i64()?,
                Value::String(s) if matches("0|[1-9][0-9]{0,15}", s) => s.parse().ok()?,
                _ => return None,
            };
            (min..=max).contains(&n).then_some(Param::Int(n))
        }
        Rule::Bool => match v {
            Value::Bool(b) => Some(Param::Bool(*b)),
            Value::Number(n) if n.as_u64() == Some(1) => Some(Param::Bool(true)),
            Value::Number(n) if n.as_u64() == Some(0) => Some(Param::Bool(false)),
            _ => None,
        },
        Rule::Enum(words) => {
            let s = v.as_str()?;
            words.contains(&s).then(|| Param::Str(s.to_owned()))
        }
        Rule::Scalar { max } => {
            let s = scalar_text(v)?;
            (s.len() <= max && is_text_line(&s) && s_ok(&s)).then_some(Param::Str(s))
        }
    }
}

/// The request checked against `fields`: unknown fields, types, rules, required fields, in that order.
pub fn validate(fields: &[Field], req: &Value) -> ApiResult<Params> {
    let obj: &Map<String, Value> =
        req.as_object().ok_or_else(|| ApiError::new(Code::BadRequest, "the request is not one JSON object"))?;
    for k in obj.keys() {
        if k != UBUS_SESSION && !fields.iter().any(|f| f.name == k) {
            return Err(ApiError::field(Code::UnknownField, k, format!("unknown field: {k}")));
        }
    }
    let mut out = Params::default();
    for f in fields {
        match obj.get(f.name) {
            None | Some(Value::Null) => {
                if f.required {
                    return Err(ApiError::missing(f.name));
                }
            }
            Some(v) => {
                let p = check_one(f, v).ok_or_else(|| ApiError::bad_value(f.name))?;
                out.values.insert(f.name, p);
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const F: &[Field] = &[
        Field::req("name", Rule::Word { re: "[a-z]{1,8}", max: 8 }),
        Field::opt("n", Rule::Int { min: 1, max: 5 }),
        Field::opt("b", Rule::Bool),
        Field::secret("pw", Rule::Text { max_bytes: 16, max_chars: 16 }, false),
        Field::opt("t", Rule::Text { max_bytes: 16, max_chars: 16 }),
    ];

    fn code(r: ApiResult<Params>) -> (Code, Option<String>) {
        let e = r.unwrap_err();
        (e.code, e.field)
    }

    #[test]
    fn accepts_typed_fields() {
        let p = validate(F, &json!({"name":"abc","n":"3","b":1,"pw":"-x","ubus_rpc_session":"s"})).unwrap();
        assert_eq!(p.str("name"), Some("abc"));
        assert_eq!(p.int("n"), Some(3));
        assert_eq!(p.bool("b"), Some(true));
        assert_eq!(p.str("pw"), Some("-x"));
    }

    #[test]
    fn refuses() {
        assert_eq!(code(validate(F, &json!([]))).0, Code::BadRequest);
        assert_eq!(code(validate(F, &json!({"name":"a","x":1}))), (Code::UnknownField, Some("x".into())));
        assert_eq!(code(validate(F, &json!({}))), (Code::Missing, Some("name".into())));
        assert_eq!(code(validate(F, &json!({"name":null}))).0, Code::Missing);
        assert_eq!(code(validate(F, &json!({"name":"ABC"}))), (Code::BadValue, Some("name".into())));
        assert_eq!(code(validate(F, &json!({"name":"a","n":6}))).0, Code::BadValue);
        assert_eq!(code(validate(F, &json!({"name":"a","n":"03"}))).0, Code::BadValue);
        assert_eq!(code(validate(F, &json!({"name":"a","b":"true"}))).0, Code::BadValue);
        assert_eq!(code(validate(F, &json!({"name":"a","t":"-rf"}))), (Code::BadValue, Some("t".into())));
        assert_eq!(code(validate(F, &json!({"name":"a","t":"a\nb"}))).0, Code::BadValue);
        assert_eq!(code(validate(F, &json!({"name":"a","t":{"x":1}}))).0, Code::BadValue);
    }

    #[test]
    fn anchored() {
        assert!(!matches("[a-z]+", "abc1"));
        assert!(!matches("a|b", "ab"));
        assert!(check_word("v", "-1", "-?[0-9]", 4).is_err());
    }
}
