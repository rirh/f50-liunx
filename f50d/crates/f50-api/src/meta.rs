//! `f50.meta`: the API describing itself.
//!
//! `describe` returns every public object with its methods: access, one-line doc, and each field with its type and
//! rule (pattern, limits, choices) as the validation applies it. A frontend can build forms from it; the generated
//! reference is the same data. Nothing secret is in it: field values never are.

use f50_core::rpc::{Access, Field, Object, Params, Rule};
use serde_json::{Value, json};

fn rule(r: Rule) -> Value {
    match r {
        Rule::Word { re, max } => json!({ "type": "string", "pattern": re, "max_bytes": max }),
        Rule::WordNum { re, max } => json!({ "type": "string|number", "pattern": re, "max_bytes": max }),
        Rule::Text { max_bytes, max_chars } => {
            json!({ "type": "text", "max_bytes": max_bytes, "max_chars": max_chars })
        }
        Rule::Body { max_chars } => json!({ "type": "body", "max_chars": max_chars }),
        Rule::Int { min, max } => json!({ "type": "integer", "min": min, "max": max }),
        Rule::Bool => json!({ "type": "boolean" }),
        Rule::Enum(words) => json!({ "type": "enum", "values": words }),
    }
}

fn field(f: &Field) -> Value {
    let mut v = rule(f.rule);
    v["name"] = json!(f.name);
    v["required"] = json!(f.required);
    if f.secret {
        v["secret"] = json!(true);
    }
    v
}

/// The description of `objects`.
pub fn describe(objects: &[Object]) -> Value {
    let objs: Vec<Value> = objects
        .iter()
        .map(|o| {
            let methods: Vec<Value> = o
                .methods
                .iter()
                .map(|m| {
                    json!({
                        "name": m.name,
                        "access": match m.access { Access::Read => "read", Access::Write => "write" },
                        "doc": m.doc,
                        "fields": m.fields.iter().map(field).collect::<Vec<_>>(),
                    })
                })
                .collect();
            json!({ "name": o.name, "methods": methods })
        })
        .collect();
    json!({ "api": 1, "objects": objs })
}

pub(crate) fn object(described: Vec<Object>) -> Object {
    let doc = describe(&described);
    Object::builder("f50.meta")
        .method(
            "describe",
            Access::Read,
            &[],
            "Every object and method, with access and field rules.",
            move |_p: Params| {
                let d = doc.clone();
                async move { Ok(d) }
            },
        )
        .build()
}
