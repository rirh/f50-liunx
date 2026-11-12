//! Objects, methods and the dispatch path.

use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use serde::Serialize;
use serde_json::{Value, json};

use super::field::Field;
use super::params::{Params, validate};
use crate::error::{Code, Error, Result};

/// Which ACL grant a method belongs to: reads may be given to a read-only login, writes never.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    /// Changes nothing and returns nothing secret.
    Read,
    /// Changes something, or returns something private (SMS text, the AT history, a phone number).
    Write,
}

/// What a handler returns: any serialisable value, turned into the envelope's `data`.
pub type Reply = Result<Value>;

type HandlerFuture = Pin<Box<dyn Future<Output = Reply> + Send>>;
type Handler = Arc<dyn Fn(Params) -> HandlerFuture + Send + Sync>;

/// One method of an object.
#[derive(Clone)]
pub struct Method {
    pub name: &'static str,
    pub access: Access,
    pub fields: &'static [Field],
    /// One sentence for the generated API documentation.
    pub doc: &'static str,
    handler: Handler,
}

impl fmt::Debug for Method {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Method").field("name", &self.name).field("access", &self.access).finish_non_exhaustive()
    }
}

impl Method {
    /// The request checked against the fields, then the handler.
    pub async fn call(&self, req: &Value) -> Reply {
        let params = validate(self.fields, req)?;
        (self.handler)(params).await
    }
}

/// A named set of methods, published as one ubus object.
#[derive(Debug, Clone)]
pub struct Object {
    pub name: &'static str,
    pub methods: Vec<Method>,
}

impl Object {
    pub fn builder(name: &'static str) -> ObjectBuilder {
        ObjectBuilder { obj: Object { name, methods: Vec::new() } }
    }

    pub fn method(&self, name: &str) -> Option<&Method> {
        self.methods.iter().find(|m| m.name == name)
    }
}

/// Builds an [`Object`]; each handler is a closure over the capability's service.
#[derive(Debug)]
pub struct ObjectBuilder {
    obj: Object,
}

impl ObjectBuilder {
    /// Adds a method. `handler` receives the checked request; its value is serialised into `data`.
    #[must_use]
    pub fn method<F, Fut, T>(
        mut self,
        name: &'static str,
        access: Access,
        fields: &'static [Field],
        doc: &'static str,
        handler: F,
    ) -> Self
    where
        F: Fn(Params) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<T>> + Send + 'static,
        T: Serialize,
    {
        debug_assert!(self.obj.method(name).is_none(), "{}.{name} declared twice", self.obj.name);
        let handler: Handler = Arc::new(move |p| {
            let fut = handler(p);
            Box::pin(async move {
                let v = fut.await?;
                serde_json::to_value(v).map_err(|e| Error::failed(format!("reply not serialisable: {e}")))
            })
        });
        self.obj.methods.push(Method { name, access, fields, doc, handler });
        self
    }

    pub fn build(self) -> Object {
        self.obj
    }
}

/// The one path from a request to a reply: find the method, validate, run.
pub async fn dispatch(objects: &[Object], object: &str, method: &str, req: &Value) -> Reply {
    let m = objects
        .iter()
        .find(|o| o.name == object)
        .and_then(|o| o.method(method))
        .ok_or_else(|| Error::new(Code::UnknownMethod, format!("no method {object}.{method}")))?;
    m.call(req).await
}

/// The envelope every method answers with: `{"ok":true,"data":...}` or
/// `{"ok":false,"code":"...","error":"...","field":"..."}`.
pub fn envelope(r: &Reply) -> Value {
    match r {
        Ok(data) => json!({ "ok": true, "data": data }),
        Err(e) => {
            let mut v = json!({ "ok": false, "code": e.code().as_str(), "error": e.message() });
            if let Some(f) = e.field_name() {
                v["field"] = json!(f);
            }
            v
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rpc::Rule;

    const NAME: &[Field] = &[Field::req("name", Rule::Word { re: "[a-z]{1,8}", max: 8 })];

    fn object() -> Object {
        Object::builder("t.demo")
            .method("hello", Access::Read, NAME, "Says hello.", |p: Params| async move {
                Ok(json!({ "hi": p.str("name") }))
            })
            .build()
    }

    #[tokio::test]
    async fn dispatch_validates_then_runs() {
        let objs = [object()];
        let ok = dispatch(&objs, "t.demo", "hello", &json!({"name":"x"})).await;
        assert_eq!(envelope(&ok), json!({"ok":true,"data":{"hi":"x"}}));
        let bad = dispatch(&objs, "t.demo", "hello", &json!({})).await;
        assert_eq!(envelope(&bad), json!({"ok":false,"code":"missing","error":"name is required","field":"name"}));
        let none = dispatch(&objs, "t.demo", "nope", &json!({})).await;
        assert_eq!(none.map_err(|e| e.code()), Err(Code::UnknownMethod));
    }
}
