//! The API as an OpenAPI 3.1 document (Swagger), generated from the same method table as `f50.meta describe`.
//!
//! The transport is uhttpd's `/ubus` JSON-RPC, a single endpoint, so each method is described as a *virtual* path
//! `POST /{object}/{method}` whose body is the method's arguments and whose response is f50d's envelope. The real call
//! is `{"jsonrpc":"2.0","id":1,"method":"call","params":["<session>","<object>","<method>",{args}]}` to `/ubus`; the
//! document says so, and every operation carries `x-ubus` (object, method, access, the ACL group that grants it).
//! The Swagger UI page shipped with the web console turns a virtual call into the real one.
//!
//! - Request bodies come from the field rules exactly as the validation applies them.
//! - Response `data` schemas are inferred ([`infer`]) from replies sampled on a device: shapes and types only, never
//!   values. A method that was not sampled (it writes, or needs arguments) has `data` left open.
//! - The live event stream (`GET /ubus/subscribe/f50.live`) is a real path and is described as one.

use std::collections::BTreeMap;

use serde_json::{Map, Value, json};

/// The codes of an `ok: false` reply (`f50_core::Code::as_str`).
const CODES: &[&str] = &[
    "bad-request",
    "unknown-method",
    "unknown-field",
    "bad-value",
    "missing",
    "forbidden",
    "unsupported",
    "busy",
    "timeout",
    "failed",
    "not-found",
];

/// A field rule (as `f50.meta describe` writes it) as a JSON Schema.
fn field_schema(f: &Value) -> Value {
    let max_bytes = f.get("max_bytes").and_then(Value::as_u64);
    let mut s = match f.get("type").and_then(Value::as_str).unwrap_or("") {
        "string" => {
            let mut s = json!({ "type": "string" });
            if let Some(re) = f.get("pattern").and_then(Value::as_str) {
                s["pattern"] = json!(format!("^(?:{re})$"));
            }
            if let Some(m) = max_bytes {
                s["maxLength"] = json!(m);
            }
            s
        }
        "string|number" => {
            let mut text = json!({ "type": "string" });
            if let Some(re) = f.get("pattern").and_then(Value::as_str) {
                text["pattern"] = json!(format!("^(?:{re})$"));
            }
            if let Some(m) = max_bytes {
                text["maxLength"] = json!(m);
            }
            json!({ "oneOf": [text, { "type": "number" }] })
        }
        "text" => {
            let mut s = json!({ "type": "string", "description": "One line of UTF-8 text, no control characters." });
            if let Some(m) = f.get("max_chars").and_then(Value::as_u64) {
                s["maxLength"] = json!(m);
            }
            if let Some(m) = max_bytes {
                s["x-max-bytes"] = json!(m);
            }
            s
        }
        "body" => {
            let mut s = json!({ "type": "string", "minLength": 1, "description": "Text; newlines and tabs allowed." });
            if let Some(m) = f.get("max_chars").and_then(Value::as_u64) {
                s["maxLength"] = json!(m);
            }
            s
        }
        "integer" => {
            // f50d also takes a string of digits
            let mut n = json!({ "type": "integer" });
            if let Some(v) = f.get("min") {
                n["minimum"] = v.clone();
            }
            if let Some(v) = f.get("max") {
                n["maximum"] = v.clone();
            }
            json!({ "oneOf": [n, { "type": "string", "pattern": "^-?[0-9]+$" }] })
        }
        "boolean" => json!({ "oneOf": [{ "type": "boolean" }, { "enum": [0, 1] }] }),
        "enum" => json!({ "type": "string", "enum": f.get("values").cloned().unwrap_or(json!([])) }),
        _ => json!({}),
    };
    if f.get("secret").and_then(Value::as_bool) == Some(true) {
        s["writeOnly"] = json!(true);
        s["x-secret"] = json!(true);
    }
    s
}

fn request_body(fields: &[Value]) -> Value {
    let mut props = Map::new();
    let mut required = Vec::new();
    for f in fields {
        let Some(name) = f.get("name").and_then(Value::as_str) else { continue };
        props.insert(name.to_owned(), field_schema(f));
        if f.get("required").and_then(Value::as_bool) == Some(true) {
            required.push(name);
        }
    }
    let mut schema = json!({ "type": "object", "additionalProperties": false, "properties": props });
    if !required.is_empty() {
        schema["required"] = json!(required);
    }
    json!({ "required": !required.is_empty(), "content": { "application/json": { "schema": schema } } })
}

/// Long work is a job: the method answers `{"job": "<id>"}` and the caller polls `f50.job status`.
fn starts_job(doc: &str) -> bool {
    let d = doc.to_ascii_lowercase();
    d.contains("a job") || d.contains("(job") || d.contains("as a job") || d.contains("run as a job")
}

fn response(data: Option<&Value>, job: bool) -> Value {
    let data_schema = match (data, job) {
        (Some(s), false) => s.clone(),
        (Some(s), true) => json!({ "oneOf": [s, { "$ref": "#/components/schemas/JobStarted" }] }),
        (None, true) => json!({ "description": "The result, or `{job}` when the work runs as a job.",
                                 "anyOf": [{ "$ref": "#/components/schemas/JobStarted" }, {}] }),
        (None, false) => json!({ "description": "Not sampled: see the method's description." }),
    };
    json!({
        "200": {
            "description": "f50d's envelope: `ok` with `data`, or `ok: false` with a code (always HTTP 200; ubus \
                            itself only fails for a session or a missing object, as JSON-RPC errors).",
            "content": { "application/json": { "schema": { "oneOf": [
                { "type": "object", "required": ["ok", "data"], "additionalProperties": false,
                  "properties": { "ok": { "const": true }, "data": data_schema } },
                { "$ref": "#/components/schemas/Failure" },
            ] } } },
        },
    })
}

/// The OpenAPI document for `describe` (the `f50.meta describe` reply). `samples` maps `"object.method"` to the
/// inferred schema of its `data`; `version` is the daemon's.
pub fn generate(describe: &Value, samples: &BTreeMap<String, Value>, version: &str) -> Value {
    let mut paths = Map::new();
    let mut tags = Vec::new();
    for o in describe.get("objects").and_then(Value::as_array).into_iter().flatten() {
        let Some(object) = o.get("name").and_then(Value::as_str) else { continue };
        tags.push(json!({ "name": object }));
        for m in o.get("methods").and_then(Value::as_array).into_iter().flatten() {
            let Some(method) = m.get("name").and_then(Value::as_str) else { continue };
            let access = m.get("access").and_then(Value::as_str).unwrap_or("write");
            let doc = m.get("doc").and_then(Value::as_str).unwrap_or("");
            let fields: Vec<Value> = m.get("fields").and_then(Value::as_array).cloned().unwrap_or_default();
            let job = starts_job(doc);
            let summary = doc.split(". ").next().unwrap_or(doc).trim_end_matches('.');
            let mut op = json!({
                "tags": [object],
                "operationId": format!("{}.{method}", object.trim_start_matches("f50.")),
                "summary": summary,
                "description": format!(
                    "{doc}\n\n**Access:** {access} (rpcd ACL group `{}`). **ubus:** `{object}` `{method}`.{}",
                    if access == "read" { "f50-read" } else { "f50-admin" },
                    if job { " Long work: answers `{job}`; poll `f50.job status`." } else { "" },
                ),
                "x-ubus": { "object": object, "method": method, "access": access, "job": job },
                "responses": response(samples.get(&format!("{object}.{method}")), job),
            });
            if !fields.is_empty() {
                op["requestBody"] = request_body(&fields);
            }
            paths.insert(format!("/{object}/{method}"), json!({ "post": op }));
        }
    }
    paths.insert(
        "/ubus/subscribe/f50.live".into(),
        json!({ "get": {
        "tags": ["f50.live"],
        "operationId": "live.subscribe",
        "summary": "The live event stream",
        "description": "A real uhttpd path (not a virtual one): `text/event-stream` of `f50.live` notifications while \
                        subscribed. Each event's name is a topic (`status`, `system`, `modem`, `cells`, `sms`) and its \
                        data the envelope of the read method behind it. Needs `:subscribe` on `f50.live` \
                        (in `f50-read`). The session goes in `Authorization: Bearer <session>`.",
        "x-real-path": true,
        "responses": {
            "200": { "description": "The event stream.",
                     "content": { "text/event-stream": { "schema": { "type": "string" },
                       "example": "event: modem\ndata: {\"ok\":true,\"data\":{...}}\n\n" } } },
        },
    } }),
    );

    json!({
        "openapi": "3.1.0",
        "info": {
            "title": "f50d API",
            "version": version,
            "description": "The ZTE F50 device API, published by f50d as ubus objects and reached through uhttpd's \
                `/ubus` JSON-RPC endpoint with an rpcd session (OpenWrt's system accounts).\n\n\
                **Calling a method.** Every path here is virtual: `POST /{object}/{method}` stands for\n\n\
                ```json\n{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"call\",\
                \"params\":[\"<session>\",\"<object>\",\"<method>\",{ ...body }]}\n```\n\n\
                sent to `POST /ubus`. The JSON-RPC result is `[0, <envelope>]`; the envelope is what each operation \
                documents. A session comes from `session login` (`{\"username\":\"root\",\"password\":\"...\"}`, \
                with the anonymous session `00000000000000000000000000000000`). Access is granted per method by the \
                rpcd ACL f50d generates: `f50-read` (every read method) and `f50-admin` (everything).\n\n\
                **Generated** from the method table (`f50d openapi`): request schemas are the validation rules \
                themselves; response `data` schemas are inferred from replies sampled on a device (shape only).",
        },
        "servers": [{ "url": "http://192.168.77.1", "description": "The device (uhttpd)" }],
        "security": [{ "rpcdSession": [] }],
        "tags": tags,
        "paths": paths,
        "components": {
            "securitySchemes": {
                "rpcdSession": {
                    "type": "apiKey", "in": "header", "name": "Authorization",
                    "description": "The rpcd session id. On `/ubus` it is the first JSON-RPC parameter; on \
                                    `/ubus/subscribe/...` it is `Authorization: Bearer <session>`.",
                },
            },
            "schemas": {
                "Failure": {
                    "type": "object",
                    "required": ["ok", "code", "error"],
                    "properties": {
                        "ok": { "const": false },
                        "code": { "type": "string", "enum": CODES },
                        "error": { "type": "string", "description": "What happened, in f50d's words (English)." },
                        "field": { "type": "string", "description": "The request field at fault, when one is." },
                    },
                },
                "JobStarted": {
                    "type": "object",
                    "required": ["job"],
                    "properties": { "job": { "type": "string", "description": "Poll `f50.job status {id}`." } },
                },
            },
        },
    })
}

/// The JSON Schema of `samples` (replies of one method): their shape and types, never their values. Objects list
/// every key seen (`required` only for keys in all of them); a value seen only as `null` is left open.
pub fn infer(samples: &[&Value]) -> Value {
    let mut types: Vec<&'static str> = Vec::new();
    let mut objects: Vec<&Map<String, Value>> = Vec::new();
    let mut items: Vec<&Value> = Vec::new();
    for v in samples {
        let t = match v {
            Value::Null => "null",
            Value::Bool(_) => "boolean",
            Value::Number(n) if n.is_i64() || n.is_u64() => "integer",
            Value::Number(_) => "number",
            Value::String(_) => "string",
            Value::Array(a) => {
                items.extend(a.iter());
                "array"
            }
            Value::Object(o) => {
                objects.push(o);
                "object"
            }
        };
        if !types.contains(&t) {
            types.push(t);
        }
    }
    // a number seen both whole and fractional is a number
    if types.contains(&"number") {
        types.retain(|t| *t != "integer");
    }
    let mut schema = Map::new();
    let real: Vec<&str> = types.iter().copied().filter(|t| *t != "null").collect();
    match (real.len(), types.contains(&"null")) {
        (0, _) => return json!({ "description": "Only seen as null." }),
        (1, false) => {
            schema.insert("type".into(), json!(real[0]));
        }
        _ => {
            schema.insert("type".into(), json!(types));
        }
    }
    if !objects.is_empty() {
        let mut keys: BTreeMap<&str, Vec<&Value>> = BTreeMap::new();
        for o in &objects {
            for (k, v) in o.iter() {
                keys.entry(k.as_str()).or_default().push(v);
            }
        }
        let required: Vec<&str> = keys.keys().copied().filter(|k| objects.iter().all(|o| o.contains_key(*k))).collect();
        let props: Map<String, Value> = keys.iter().map(|(k, vs)| ((*k).to_owned(), infer(vs))).collect();
        schema.insert("properties".into(), Value::Object(props));
        if !required.is_empty() {
            schema.insert("required".into(), json!(required));
        }
    }
    if real.contains(&"array") {
        schema.insert("items".into(), if items.is_empty() { json!({}) } else { infer(&items) });
    }
    Value::Object(schema)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules_become_schemas() {
        let f = |v: Value| field_schema(&v);
        assert_eq!(
            f(json!({"type":"string","pattern":"[a-z]+","max_bytes":8})),
            json!({"type":"string","pattern":"^(?:[a-z]+)$","maxLength":8})
        );
        assert_eq!(f(json!({"type":"enum","values":["on","off"]})), json!({"type":"string","enum":["on","off"]}));
        let pw = f(json!({"type":"text","max_bytes":64,"max_chars":64,"secret":true}));
        assert_eq!((pw["writeOnly"].clone(), pw["maxLength"].clone()), (json!(true), json!(64)));
        assert_eq!(
            f(json!({"type":"integer","min":1,"max":60}))["oneOf"][0],
            json!({"type":"integer","minimum":1,"maximum":60})
        );
    }

    #[test]
    fn inference_keeps_shape_not_values() {
        let a = json!({"ssid": "home", "clients": [{"mac": "aa"}], "channel": 36, "rssi": null});
        let b = json!({"ssid": null, "clients": [], "channel": 1.5, "extra": true});
        let s = infer(&[&a, &b]);
        assert_eq!(s["type"], json!("object"));
        assert_eq!(s["properties"]["ssid"]["type"], json!(["string", "null"]));
        assert_eq!(s["properties"]["channel"]["type"], json!("number"));
        assert_eq!(s["properties"]["clients"]["items"]["properties"]["mac"]["type"], json!("string"));
        assert_eq!(s["properties"]["rssi"], json!({"description": "Only seen as null."}));
        assert_eq!(s["required"], json!(["channel", "clients", "ssid"]));
        assert!(!s.to_string().contains("home"), "no value leaks into the schema");
    }

    #[test]
    fn a_method_becomes_a_virtual_path() {
        let d = json!({"objects":[{"name":"f50.wifi","methods":[
            {"name":"get","access":"read","doc":"The hotspot.","fields":[]},
            {"name":"set","access":"write","doc":"Changes it; runs as a job.","fields":[
                {"name":"ssid","type":"text","max_bytes":32,"max_chars":32,"required":true}]}]}]});
        let mut samples = BTreeMap::new();
        samples.insert("f50.wifi.get".to_owned(), json!({"type":"object"}));
        let doc = generate(&d, &samples, "1.0");
        let get = &doc["paths"]["/f50.wifi/get"]["post"];
        assert_eq!(get["x-ubus"]["access"], json!("read"));
        assert!(get.get("requestBody").is_none());
        assert_eq!(
            get["responses"]["200"]["content"]["application/json"]["schema"]["oneOf"][0]["properties"]["data"],
            json!({"type":"object"})
        );
        let set = &doc["paths"]["/f50.wifi/set"]["post"];
        assert_eq!(set["x-ubus"]["job"], json!(true));
        assert_eq!(set["requestBody"]["content"]["application/json"]["schema"]["required"], json!(["ssid"]));
        assert!(doc["paths"].get("/ubus/subscribe/f50.live").is_some());
    }
}
