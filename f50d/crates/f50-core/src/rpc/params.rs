//! Request validation: the request checked field by field into [`Params`] before a handler sees it.

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};

use regex::Regex;
use serde_json::{Map, Value};

use super::field::{Field, Rule, UBUS_SESSION};
use crate::error::{Code, Error, Result};

/// One checked value.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Param {
    Str(String),
    Int(i64),
    Bool(bool),
}

/// The checked request. Getters return `None` for a field that was not sent (or sent as `null`).
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

    /// A boolean field, false when not sent.
    pub fn flag(&self, name: &str) -> bool {
        self.bool(name).unwrap_or(false)
    }

    /// A string field the method needs (for a field whose requirement depends on another field).
    pub fn need_str(&self, name: &'static str) -> Result<&str> {
        self.str(name).ok_or_else(|| Error::missing(name))
    }

    pub fn need_int(&self, name: &'static str) -> Result<i64> {
        self.int(name).ok_or_else(|| Error::missing(name))
    }

    pub fn has(&self, name: &str) -> bool {
        self.values.contains_key(name)
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Builders for requests made inside the daemon and for tests.
    #[must_use]
    pub fn with_str(mut self, name: &'static str, v: impl Into<String>) -> Self {
        self.values.insert(name, Param::Str(v.into()));
        self
    }
    #[must_use]
    pub fn with_int(mut self, name: &'static str, v: i64) -> Self {
        self.values.insert(name, Param::Int(v));
        self
    }
    #[must_use]
    pub fn with_bool(mut self, name: &'static str, v: bool) -> Self {
        self.values.insert(name, Param::Bool(v));
        self
    }
}

type Cache = Mutex<HashMap<&'static str, Arc<Regex>>>;
static CACHE: LazyLock<Cache> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// `s` matches `^(?:re)$`. Patterns are constants of the calling crate; an invalid one never matches (and the
/// crate's tests catch it).
pub fn matches(re: &'static str, s: &str) -> bool {
    let rx = {
        let mut c = CACHE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(r) = c.get(re) {
            Some(r.clone())
        } else {
            let compiled = Regex::new(&format!("^(?:{re})$")).ok().map(Arc::new);
            if let Some(r) = &compiled {
                c.insert(re, r.clone());
            }
            compiled
        }
    };
    rx.is_some_and(|r| r.is_match(s))
}

/// One line of printable ASCII.
pub fn is_ascii_line(s: &str) -> bool {
    s.bytes().all(|b| (0x20..=0x7e).contains(&b))
}

/// One line of text: no control character (UTF-8 allowed).
pub fn is_text_line(s: &str) -> bool {
    !s.chars().any(char::is_control)
}

/// `value` checked as a word by the method itself (when the rule depends on another field).
pub fn check_word(field: &'static str, value: &str, re: &'static str, max: usize) -> Result<()> {
    if value.len() <= max && is_ascii_line(value) && !value.starts_with('-') && matches(re, value) {
        Ok(())
    } else {
        Err(Error::bad_value(field))
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
    let lead_ok = |s: &str| f.secret || !s.starts_with('-');
    match f.rule {
        Rule::Word { re, max } => {
            let s = v.as_str()?;
            (s.len() <= max && is_ascii_line(s) && lead_ok(s) && matches(re, s)).then(|| Param::Str(s.to_owned()))
        }
        Rule::WordNum { re, max } => {
            let s = scalar_text(v)?;
            (s.len() <= max && is_ascii_line(&s) && lead_ok(&s) && matches(re, &s)).then_some(Param::Str(s))
        }
        Rule::Text { max_bytes, max_chars } => {
            let s = v.as_str()?;
            (s.len() <= max_bytes && s.chars().count() <= max_chars && is_text_line(s) && lead_ok(s))
                .then(|| Param::Str(s.to_owned()))
        }
        Rule::Body { max_chars } => {
            let s = v.as_str()?;
            let chars_ok = s.chars().all(|c| c == '\n' || c == '\t' || !c.is_control());
            (!s.is_empty() && s.chars().count() <= max_chars && chars_ok && lead_ok(s))
                .then(|| Param::Str(s.to_owned()))
        }
        Rule::Int { min, max } => {
            let n = match v {
                Value::Number(n) => n.as_i64()?,
                Value::String(s) if matches("0|-?[1-9][0-9]{0,17}", s) => s.parse().ok()?,
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
    }
}

/// The request checked against `fields`: its shape, unknown fields, then each field's rule and requirement.
pub fn validate(fields: &[Field], req: &Value) -> Result<Params> {
    let obj: &Map<String, Value> =
        req.as_object().ok_or_else(|| Error::new(Code::BadRequest, "the request is not one JSON object"))?;
    if let Some(k) = obj.keys().find(|k| k.as_str() != UBUS_SESSION && !fields.iter().any(|f| f.name == k.as_str())) {
        return Err(Error::new(Code::UnknownField, format!("unknown field: {k}")));
    }
    let mut out = Params::default();
    for f in fields {
        match obj.get(f.name) {
            None | Some(Value::Null) if f.required => return Err(Error::missing(f.name)),
            None | Some(Value::Null) => {}
            Some(v) => {
                let p = check_one(f, v).ok_or_else(|| Error::bad_value(f.name))?;
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
        Field::opt("n", Rule::Int { min: -5, max: 5 }),
        Field::opt("b", Rule::Bool),
        Field::secret("pw", Rule::Text { max_bytes: 16, max_chars: 16 }, false),
        Field::opt("t", Rule::Text { max_bytes: 16, max_chars: 16 }),
        Field::opt("e", Rule::Enum(&["on", "off"])),
    ];

    fn refused(req: serde_json::Value) -> (Code, Option<&'static str>) {
        let e = validate(F, &req).expect_err("refused");
        (e.code(), e.field_name())
    }

    #[test]
    fn accepts_typed_fields() {
        let p = validate(F, &json!({"name":"abc","n":"-3","b":1,"pw":"-x","e":"on","ubus_rpc_session":"s"}))
            .expect("valid");
        assert_eq!(p.str("name"), Some("abc"));
        assert_eq!(p.int("n"), Some(-3));
        assert_eq!(p.bool("b"), Some(true));
        assert_eq!(p.str("pw"), Some("-x"));
        assert_eq!(p.str("e"), Some("on"));
        assert!(!p.has("t"));
    }

    #[test]
    fn refuses() {
        assert_eq!(refused(json!([])).0, Code::BadRequest);
        assert_eq!(refused(json!({"name":"a","x":1})).0, Code::UnknownField);
        assert_eq!(refused(json!({})), (Code::Missing, Some("name")));
        assert_eq!(refused(json!({"name":null})), (Code::Missing, Some("name")));
        assert_eq!(refused(json!({"name":"ABC"})), (Code::BadValue, Some("name")));
        assert_eq!(refused(json!({"name":"a","n":6})).0, Code::BadValue);
        assert_eq!(refused(json!({"name":"a","n":"03"})).0, Code::BadValue);
        assert_eq!(refused(json!({"name":"a","b":"true"})).0, Code::BadValue);
        assert_eq!(refused(json!({"name":"a","t":"-rf"})), (Code::BadValue, Some("t")));
        assert_eq!(refused(json!({"name":"a","t":"a\nb"})).0, Code::BadValue);
        assert_eq!(refused(json!({"name":"a","e":"maybe"})).0, Code::BadValue);
    }

    #[test]
    fn patterns_are_anchored() {
        assert!(!matches("[a-z]+", "abc1"));
        assert!(!matches("a|b", "ab"));
        assert!(!matches("(", "("));
        assert!(check_word("v", "-1", "-?[0-9]", 4).is_err());
    }
}
