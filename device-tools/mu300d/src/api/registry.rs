//! The method table and the one dispatch path every surface (`call`, HTTP) goes through.

use std::future::Future;
use std::pin::Pin;
use std::sync::LazyLock;

use serde_json::{Value, json};

use super::error::{ApiError, ApiResult, Code};
use super::validate::{Field, Params, validate};
use crate::sys::Ctx;

pub type BoxFut = Pin<Box<dyn Future<Output = ApiResult<Value>> + Send>>;
pub type Handler = fn(Ctx, Params) -> BoxFut;

/// Who may call a method.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    /// Anyone who can reach the API (hello, methods, auth.login).
    Public,
    /// Reads state; over HTTP it needs a session unless `open_reads` is set.
    Read,
    /// Changes something; needs a session over HTTP and root for `call`.
    Write,
}

impl Access {
    pub fn as_str(self) -> &'static str {
        match self {
            Access::Public => "public",
            Access::Read => "read",
            Access::Write => "write",
        }
    }
}

pub struct Method {
    pub name: &'static str,
    pub access: Access,
    pub fields: &'static [Field],
    pub handler: Handler,
}

/// A table entry: `m!("name", Read, FIELDS, path::to::async_fn)`.
#[macro_export]
macro_rules! m {
    ($name:literal, $access:ident, $fields:expr, $f:path) => {
        $crate::api::registry::Method {
            name: $name,
            access: $crate::api::registry::Access::$access,
            fields: $fields,
            handler: |c, p| Box::pin($f(c, p)),
        }
    };
}

static ALL: LazyLock<Vec<&'static Method>> = LazyLock::new(|| {
    use crate::methods::*;
    [
        system::METHODS,
        status::METHODS,
        data::METHODS,
        sim::METHODS,
        sms::METHODS,
        wifi::METHODS,
        lan::METHODS,
        device::METHODS,
        debug::METHODS,
        job::METHODS,
    ]
    .into_iter()
    .flatten()
    .collect()
});

/// The auth methods exist only over HTTP (http.rs answers them); they are listed so a client sees them.
pub const HTTP_ONLY: &[(&str, Access)] = &[("auth.login", Access::Public), ("auth.logout", Access::Public)];

pub fn all() -> &'static [&'static Method] {
    &ALL
}

pub fn find(name: &str) -> Option<&'static Method> {
    ALL.iter().copied().find(|m| m.name == name)
}

/// Every method name, in table order.
pub fn names() -> Vec<&'static str> {
    ALL.iter().map(|m| m.name).chain(HTTP_ONLY.iter().map(|(n, _)| *n)).collect()
}

/// The table as data (the `methods` method).
pub fn describe() -> Value {
    let mut v: Vec<Value> = ALL
        .iter()
        .map(|m| {
            let fields: Vec<Value> =
                m.fields.iter().map(|f| json!({"name": f.name, "required": f.required, "secret": f.secret})).collect();
            json!({"name": m.name, "access": m.access.as_str(), "fields": fields})
        })
        .collect();
    v.extend(HTTP_ONLY.iter().map(|(n, a)| json!({"name": n, "access": a.as_str(), "fields": []})));
    json!(v)
}

/// What the caller is allowed: the surface decides (root over adb, a session over HTTP).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Grant {
    pub read: bool,
    pub write: bool,
}

/// Look the method up, authorise, validate, run.
pub async fn dispatch(ctx: &Ctx, name: &str, req: &Value, grant: Grant) -> ApiResult<Value> {
    let m = find(name).ok_or_else(|| ApiError::new(Code::UnknownMethod, format!("no such method: {name}")))?;
    let allowed = match m.access {
        Access::Public => true,
        Access::Read => grant.read,
        Access::Write => grant.write,
    };
    if !allowed {
        return Err(ApiError::new(Code::Unauthorized, "log in first"));
    }
    let params = validate(m.fields, req)?;
    (m.handler)(ctx.clone(), params).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_unique_and_shaped() {
        let names = names();
        let mut seen = std::collections::HashSet::new();
        for n in &names {
            assert!(seen.insert(*n), "duplicate {n}");
            assert!(super::super::validate::matches("[a-z]+(\\.[a-z_]+)?", n), "{n}");
        }
        assert!(names.len() >= 50, "{}", names.len());
    }

    #[test]
    fn fields_are_unique() {
        for m in all() {
            let mut seen = std::collections::HashSet::new();
            for f in m.fields {
                assert!(seen.insert(f.name), "{} {}", m.name, f.name);
            }
        }
    }
}
