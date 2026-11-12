//! The state behind [`crate::Client`]: the socket path and the current connection, dialled again on demand.
//!
//! ubusd restarts drop every connection; rather than failing forever, the next call after a drop dials again. Calls
//! that were in flight when the socket closed fail with an I/O error.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use serde_json::{Map, Value};

use crate::UbusError;
use crate::conn::Conn;
use crate::proto::status;

/// How long [`crate::Client::lookup`] waits, its signature having no timeout of its own.
pub const LOOKUP_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug)]
pub(crate) struct ClientInner {
    socket: PathBuf,
    conn: tokio::sync::Mutex<Option<Arc<Conn>>>,
}

impl ClientInner {
    pub(crate) async fn connect(socket: &Path) -> Result<ClientInner, UbusError> {
        let conn = Conn::connect(socket).await?;
        Ok(ClientInner { socket: socket.to_owned(), conn: tokio::sync::Mutex::new(Some(Arc::new(conn))) })
    }

    /// The live connection, dialling a new one if the last has closed. The async mutex serialises redials.
    async fn conn(&self) -> Result<Arc<Conn>, UbusError> {
        let mut slot = self.conn.lock().await;
        if let Some(c) = slot.as_ref().filter(|c| !c.is_closed()) {
            return Ok(Arc::clone(c));
        }
        *slot = None;
        let fresh = Arc::new(Conn::connect(&self.socket).await?);
        *slot = Some(Arc::clone(&fresh));
        Ok(fresh)
    }

    pub(crate) async fn call(
        &self,
        object: &str,
        method: &str,
        args: &Value,
        timeout: Duration,
    ) -> Result<Value, UbusError> {
        let empty = Map::new();
        let args = match args {
            Value::Object(map) => map,
            Value::Null => &empty,
            _ => return Err(UbusError::Protocol("call arguments must be a JSON object".to_owned())),
        };
        let work = async {
            let conn = self.conn().await?;
            let id = conn.lookup(object).await?.ok_or(UbusError::Status(status::NOT_FOUND))?;
            conn.invoke(id, method, args).await
        };
        tokio::time::timeout(timeout, work).await.map_err(|_| UbusError::Timeout)?
    }

    pub(crate) async fn lookup(&self, object: &str) -> Result<Option<u32>, UbusError> {
        let work = async { self.conn().await?.lookup(object).await };
        tokio::time::timeout(LOOKUP_TIMEOUT, work).await.map_err(|_| UbusError::Timeout)?
    }
}
