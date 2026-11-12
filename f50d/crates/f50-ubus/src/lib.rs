//! OpenWrt's ubus, spoken natively: no libubus, no C.
//!
//! ubusd listens on a Unix socket; every message is a 12-byte header (version, type, sequence, peer) followed by a
//! blob of attributes, and method arguments and replies are blobmsg tables, converted here to and from
//! `serde_json::Value`. f50d uses both halves:
//!
//! - [`Client`]: calls other objects (`network.interface.wan status`, `service list`, `uci get`).
//! - [`serve`]: registers f50d's objects with their method signatures and answers invocations. uhttpd's `/ubus`
//!   endpoint checks the caller's rpcd session against the ACLs before an invocation ever reaches us.
//!
//! The crate knows nothing of f50d: objects are described by [`ObjectSpec`] and answered by one [`Invoke`] callback.
//!
//! CONTRACT: the items re-exported below are used by other crates. Extend them freely; do not change a signature.

mod blob;
mod blobmsg;
mod client;
mod conn;
mod frame;
mod proto;
mod server;

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;
use tokio_util::sync::CancellationToken;

pub use client::LOOKUP_TIMEOUT;
pub use proto::{MAX_MSG_LEN, status};
pub use server::{BACKOFF_MAX, BACKOFF_MIN, INVOKE_TIMEOUT, MAX_CONCURRENT_INVOCATIONS};

/// Where ubusd listens on OpenWrt.
pub const DEFAULT_SOCKET: &str = "/var/run/ubus/ubus.sock";

/// What went wrong talking to ubusd.
#[derive(Debug, thiserror::Error)]
pub enum UbusError {
    #[error("ubus socket: {0}")]
    Io(#[from] std::io::Error),
    /// ubusd or the called object answered with a ubus status code (UBUS_STATUS_*).
    #[error("ubus status {0}")]
    Status(u32),
    #[error("ubus protocol: {0}")]
    Protocol(String),
    #[error("ubus call timed out")]
    Timeout,
}

/// A connection for calling other objects. Cheap to clone; calls may run concurrently.
#[derive(Debug, Clone)]
pub struct Client {
    inner: Arc<client::ClientInner>,
}

impl Client {
    /// Connects to ubusd at `socket` and completes the hello. If ubusd later restarts, the next call dials again.
    pub async fn connect(socket: &Path) -> Result<Client, UbusError> {
        let inner = client::ClientInner::connect(socket).await?;
        Ok(Client { inner: Arc::new(inner) })
    }

    /// `object.method(args)`: the reply's data (a JSON object; `Value::Null` when the method returns none).
    ///
    /// `args` must be a JSON object or `null`. The object is looked up by name on every call (ids change when a
    /// service restarts); `timeout` covers lookup and invocation. A missing object is `Status(NOT_FOUND)`.
    pub async fn call(&self, object: &str, method: &str, args: &Value, timeout: Duration) -> Result<Value, UbusError> {
        self.inner.call(object, method, args, timeout).await
    }

    /// The object's id, or `None` when no such object is registered. Waits at most [`LOOKUP_TIMEOUT`].
    pub async fn lookup(&self, object: &str) -> Result<Option<u32>, UbusError> {
        self.inner.lookup(object).await
    }
}

/// The type an argument is declared with in a method signature.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArgType {
    String,
    Int32,
    Bool,
    Table,
    Array,
}

impl ArgType {
    /// The `BLOBMSG_TYPE_*` value the signature declares for this type.
    pub const fn blobmsg_type(self) -> u32 {
        match self {
            ArgType::Array => blobmsg::TYPE_ARRAY as u32,
            ArgType::Table => blobmsg::TYPE_TABLE as u32,
            ArgType::String => blobmsg::TYPE_STRING as u32,
            ArgType::Int32 => blobmsg::TYPE_INT32 as u32,
            ArgType::Bool => blobmsg::TYPE_BOOL as u32,
        }
    }
}

#[derive(Debug, Clone)]
pub struct MethodSpec {
    pub name: String,
    pub args: Vec<(String, ArgType)>,
}

#[derive(Debug, Clone)]
pub struct ObjectSpec {
    pub name: String,
    pub methods: Vec<MethodSpec>,
}

pub type InvokeFuture = Pin<Box<dyn Future<Output = Value> + Send>>;

/// Answers one invocation: `(object, method, args) -> reply table`. The reply is sent as the call's data with
/// status 0; f50d reports its own failures inside the reply (`{"ok":false,...}`), never as a ubus status.
pub type Invoke = Arc<dyn Fn(String, String, Value) -> InvokeFuture + Send + Sync>;

/// Registers `objects` on the bus and answers invocations until `shutdown`. Reconnects and registers again when
/// ubusd restarts. Invocations run concurrently, at most a fixed number at a time.
///
/// At most [`MAX_CONCURRENT_INVOCATIONS`] run at once, each for at most [`INVOKE_TIMEOUT`] (the caller then gets
/// `UBUS_STATUS_TIMEOUT`). A reply that is not a JSON object or `null` is sent as `{"value": reply}`. Returns
/// `Ok(())` at shutdown, aborting invocations still running; `Err` only for specs ubusd could never accept
/// (empty or repeated names, NUL bytes, a signature over [`MAX_MSG_LEN`]).
pub async fn serve(
    socket: PathBuf,
    objects: Vec<ObjectSpec>,
    invoke: Invoke,
    shutdown: CancellationToken,
) -> Result<(), UbusError> {
    server::serve(socket, objects, invoke, shutdown).await
}
