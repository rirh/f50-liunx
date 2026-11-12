//! The bridge between the API and ubus: the objects' signatures for registration, the invoke callback, and the
//! client side the command line uses to reach the running daemon.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use f50_api::Api;
use f50_core::rpc::{Object, WireType};
use f50_ubus::{ArgType, Client, DEFAULT_SOCKET, Invoke, MethodSpec, ObjectSpec, UbusError};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

fn specs(objects: &[Object]) -> Vec<ObjectSpec> {
    objects
        .iter()
        .map(|o| ObjectSpec {
            name: o.name.to_owned(),
            methods: o
                .methods
                .iter()
                .map(|m| MethodSpec {
                    name: m.name.to_owned(),
                    args: m
                        .fields
                        .iter()
                        .map(|f| {
                            let t = match f.rule.wire_type() {
                                WireType::Int => ArgType::Int32,
                                WireType::Bool => ArgType::Bool,
                                WireType::String => ArgType::String,
                            };
                            (f.name.to_owned(), t)
                        })
                        .collect(),
                })
                .collect(),
        })
        .collect()
}

/// Registers every object and answers until `shutdown`. Each invocation goes through `Api::call`: validation,
/// dispatch, the envelope.
pub async fn serve(api: Api, shutdown: CancellationToken) {
    let objects = specs(api.all());
    let api = Arc::new(api);
    let invoke: Invoke = Arc::new(move |object: String, method: String, args: Value| {
        let api = api.clone();
        Box::pin(async move { api.call(&object, &method, &args).await })
    });
    if let Err(e) = f50_ubus::serve(PathBuf::from(DEFAULT_SOCKET), objects, invoke, shutdown).await {
        tracing::error!(error = %e, "the ubus server stopped");
    }
}

/// Connects to ubusd, waiting up to `wait` for it (procd starts it early, but f50d may be earlier still).
pub async fn connect(wait: Duration, shutdown: &CancellationToken) -> Result<Client, UbusError> {
    let deadline = tokio::time::Instant::now() + wait;
    loop {
        match Client::connect(std::path::Path::new(DEFAULT_SOCKET)).await {
            Ok(c) => return Ok(c),
            Err(e) if tokio::time::Instant::now() >= deadline || shutdown.is_cancelled() => return Err(e),
            Err(_) => tokio::time::sleep(Duration::from_millis(500)).await,
        }
    }
}

/// One call to the running daemon, for the command line: the envelope it answered, or an error envelope.
pub async fn call(object: &str, method: &str, args: &Value, timeout: Duration) -> Value {
    let reply = async {
        let c = Client::connect(std::path::Path::new(DEFAULT_SOCKET)).await?;
        c.call(object, method, args, timeout).await
    };
    match reply.await {
        Ok(v) => v,
        Err(UbusError::Status(4)) => {
            serde_json::json!({"ok": false, "code": "unsupported", "error": "f50d is not running"})
        }
        Err(e) => serde_json::json!({"ok": false, "code": "failed", "error": e.to_string()}),
    }
}
