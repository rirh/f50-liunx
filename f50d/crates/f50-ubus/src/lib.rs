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

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;
use tokio_util::sync::CancellationToken;

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
    _priv: Arc<()>,
}

impl Client {
    /// Connects to ubusd at `socket` and completes the hello.
    pub async fn connect(socket: &Path) -> Result<Client, UbusError> {
        let _ = socket;
        todo!("f50-ubus: Client::connect")
    }

    /// `object.method(args)`: the reply's data (a JSON object; `Value::Null` when the method returns none).
    pub async fn call(&self, object: &str, method: &str, args: &Value, timeout: Duration) -> Result<Value, UbusError> {
        let _ = (object, method, args, timeout);
        todo!("f50-ubus: Client::call")
    }

    /// The object's id, or `None` when no such object is registered.
    pub async fn lookup(&self, object: &str) -> Result<Option<u32>, UbusError> {
        let _ = object;
        todo!("f50-ubus: Client::lookup")
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
pub async fn serve(
    socket: PathBuf,
    objects: Vec<ObjectSpec>,
    invoke: Invoke,
    shutdown: CancellationToken,
) -> Result<(), UbusError> {
    let _ = (socket, objects, invoke, shutdown);
    todo!("f50-ubus: serve")
}
