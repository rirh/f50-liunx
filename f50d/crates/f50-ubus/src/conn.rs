//! One client connection to ubusd: the hello, a reader task, and the map of requests in flight.
//!
//! A request is matched to its replies by sequence number and peer, as `ubus_find_request` (`libubus-req.c`) does:
//! ubusd answers a LOOKUP with peer 0 and rewrites an object's replies to carry the object id as peer
//! (`ubusd_handle_response`). DATA messages accumulate on the request until its STATUS completes it.
//!
//! The reader task is owned by the [`Conn`] and aborted when it drops; when the socket ends, every pending request
//! fails and the connection is marked closed so the [`crate::Client`] can dial again.

use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::io;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use serde_json::{Map, Value};
use tokio::io::BufReader;
use tokio::net::UnixStream;
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

use crate::UbusError;
use crate::blob::proto_err;
use crate::frame::{read_message, write_message};
use crate::proto::{Body, CLIENT_ID_CHANNEL, Message, attr, msg, status};

/// How long ubusd may take to greet a new connection.
pub(crate) const HELLO_TIMEOUT: Duration = Duration::from_secs(5);
/// DATA messages kept per request; a wildcard lookup on a busy bus stays far below this.
const MAX_DATA_PER_REQUEST: usize = 4096;

/// One DATA message received for a request.
#[derive(Debug)]
pub(crate) struct DataMsg {
    pub objid: Option<u32>,
    pub data: Option<Map<String, Value>>,
}

type Outcome = Result<(u32, Vec<DataMsg>), UbusError>;

#[derive(Debug)]
struct Pending {
    peer: u32,
    data: Vec<DataMsg>,
    error: Option<UbusError>,
    done: Option<oneshot::Sender<Outcome>>,
}

#[derive(Debug, Default)]
struct Shared {
    pending: Mutex<HashMap<u16, Pending>>,
    next_seq: Mutex<u16>,
    closed: AtomicBool,
}

impl Shared {
    fn pending(&self) -> std::sync::MutexGuard<'_, HashMap<u16, Pending>> {
        self.pending.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Fails every request in flight and marks the connection unusable.
    fn close(&self, why: &str) {
        self.closed.store(true, Ordering::Release);
        let drained: Vec<Pending> = self.pending().drain().map(|(_, p)| p).collect();
        for mut p in drained {
            if let Some(tx) = p.done.take() {
                let _ = tx.send(Err(UbusError::Io(io::Error::new(io::ErrorKind::ConnectionAborted, why.to_owned()))));
            }
        }
    }
}

/// A connected, greeted ubusd socket.
#[derive(Debug)]
pub(crate) struct Conn {
    writer: tokio::sync::Mutex<OwnedWriteHalf>,
    shared: Arc<Shared>,
    reader: JoinHandle<()>,
}

impl Drop for Conn {
    fn drop(&mut self) {
        self.reader.abort();
        self.shared.closed.store(true, Ordering::Release);
    }
}

/// Removes a request's entry when the waiting call ends, whether by reply, timeout or cancellation.
struct PendingGuard<'a> {
    shared: &'a Shared,
    seq: u16,
}

impl Drop for PendingGuard<'_> {
    fn drop(&mut self) {
        self.shared.pending().remove(&self.seq);
    }
}

/// Connects to `socket` and waits for ubusd's HELLO; returns the stream and our client id.
pub(crate) async fn dial(socket: &Path) -> Result<(UnixStream, u32), UbusError> {
    let mut stream = UnixStream::connect(socket).await?;
    let hello = tokio::time::timeout(HELLO_TIMEOUT, read_message(&mut stream))
        .await
        .map_err(|_| UbusError::Timeout)??
        .ok_or_else(|| proto_err("ubusd closed the connection before its hello"))?;
    if hello.header.ty != msg::HELLO {
        return Err(UbusError::Protocol(format!("expected hello, got message type {}", hello.header.ty)));
    }
    if hello.header.peer <= CLIENT_ID_CHANNEL {
        return Err(UbusError::Protocol(format!("hello carries invalid client id {}", hello.header.peer)));
    }
    Ok((stream, hello.header.peer))
}

impl Conn {
    pub(crate) async fn connect(socket: &Path) -> Result<Conn, UbusError> {
        let (stream, id) = dial(socket).await?;
        tracing::debug!(client_id = id, socket = %socket.display(), "ubus client connected");
        let (read, write) = stream.into_split();
        let shared = Arc::new(Shared::default());
        let reader = tokio::spawn(read_loop(read, Arc::clone(&shared)));
        Ok(Conn { writer: tokio::sync::Mutex::new(write), shared, reader })
    }

    pub(crate) fn is_closed(&self) -> bool {
        self.shared.closed.load(Ordering::Acquire) || self.reader.is_finished()
    }

    /// Sends the message `build` makes for a fresh sequence number and waits for the matching STATUS.
    async fn request(
        &self,
        peer: u32,
        build: impl FnOnce(u16) -> Result<Message, UbusError>,
    ) -> Result<(u32, Vec<DataMsg>), UbusError> {
        if self.is_closed() {
            return Err(UbusError::Io(io::Error::new(io::ErrorKind::NotConnected, "ubus connection closed")));
        }
        let (tx, rx) = oneshot::channel();
        let seq = self.register(peer, tx)?;
        let _guard = PendingGuard { shared: &self.shared, seq };
        // the reader may have closed between the first check and the insert; `close` sets the flag before it
        // drains under the same lock, so an entry it missed is seen here
        if self.is_closed() {
            return Err(UbusError::Io(io::Error::new(io::ErrorKind::NotConnected, "ubus connection closed")));
        }
        let message = build(seq)?;
        {
            let mut w = self.writer.lock().await;
            if let Err(e) = write_message(&mut *w, &message).await {
                drop(w);
                self.shared.close("write to ubusd failed");
                return Err(e);
            }
        }
        rx.await.map_err(|_| UbusError::Io(io::Error::new(io::ErrorKind::ConnectionAborted, "ubus reader stopped")))?
    }

    /// Takes the next free sequence number and records the request under it.
    fn register(&self, peer: u32, done: oneshot::Sender<Outcome>) -> Result<u16, UbusError> {
        let mut pending = self.shared.pending();
        let mut next = self.shared.next_seq.lock().unwrap_or_else(PoisonError::into_inner);
        for _ in 0..=u16::MAX {
            *next = next.wrapping_add(1);
            let seq = *next;
            if let Entry::Vacant(slot) = pending.entry(seq) {
                slot.insert(Pending { peer, data: Vec::new(), error: None, done: Some(done) });
                return Ok(seq);
            }
        }
        Err(proto_err("65536 ubus requests in flight"))
    }

    /// `ubus_lookup_id`: the id of the object at `path`, `None` when ubusd reports NOT_FOUND.
    pub(crate) async fn lookup(&self, path: &str) -> Result<Option<u32>, UbusError> {
        if path.is_empty() {
            return Err(proto_err("empty object path"));
        }
        let (code, data) =
            self.request(0, |seq| Body::new().string(attr::OBJPATH, path)?.finish(msg::LOOKUP, seq, 0)).await?;
        match code {
            status::OK => Ok(data.iter().find_map(|d| d.objid)),
            status::NOT_FOUND => Ok(None),
            other => Err(UbusError::Status(other)),
        }
    }

    /// `ubus_invoke`: calls `method` on object `id`; the reply's first DATA table, or `Value::Null`.
    pub(crate) async fn invoke(&self, id: u32, method: &str, args: &Map<String, Value>) -> Result<Value, UbusError> {
        let (code, data) = self
            .request(id, |seq| {
                Body::new().u32(attr::OBJID, id)?.string(attr::METHOD, method)?.data(args)?.finish(msg::INVOKE, seq, id)
            })
            .await?;
        if code != status::OK {
            return Err(UbusError::Status(code));
        }
        Ok(data.into_iter().find_map(|d| d.data).map_or(Value::Null, Value::Object))
    }
}

async fn read_loop(read: OwnedReadHalf, shared: Arc<Shared>) {
    let mut read = BufReader::new(read);
    let why = loop {
        match read_message(&mut read).await {
            Ok(Some(m)) => dispatch(&shared, &m),
            Ok(None) => break "ubusd closed the connection",
            Err(e) => {
                tracing::warn!(error = %e, "ubus client connection failed");
                break "ubus connection failed";
            }
        }
    };
    shared.close(why);
}

/// Routes a STATUS or DATA to its request; anything else is not meant for a client without objects.
fn dispatch(shared: &Shared, m: &Message) {
    let ty = m.header.ty;
    if ty != msg::STATUS && ty != msg::DATA {
        tracing::debug!(ty, "ubus client ignored a message");
        return;
    }
    let parsed = m.attrs();
    let mut pending = shared.pending();
    let Some(p) = pending.get_mut(&m.header.seq).filter(|p| p.peer == m.header.peer) else {
        return;
    };
    let attrs = match parsed {
        Ok(a) => a,
        Err(e) => {
            p.error.get_or_insert(e);
            return;
        }
    };
    if ty == msg::DATA {
        match attrs.data() {
            Ok(data) if p.data.len() < MAX_DATA_PER_REQUEST => {
                p.data.push(DataMsg { objid: attrs.u32(attr::OBJID), data });
            }
            Ok(_) => {
                p.error.get_or_insert_with(|| proto_err("too many DATA replies"));
            }
            Err(e) => {
                p.error.get_or_insert(e);
            }
        }
        return;
    }
    // STATUS without the attribute counts as INVALID_ARGUMENT, as in ubus_process_req_status
    let code = attrs.u32(attr::STATUS).unwrap_or(status::INVALID_ARGUMENT);
    if let Some(mut done) = pending.remove(&m.header.seq)
        && let Some(tx) = done.done.take()
    {
        let outcome = match done.error.take() {
            Some(e) => Err(e),
            None => Ok((code, std::mem::take(&mut done.data))),
        };
        let _ = tx.send(outcome);
    }
}
