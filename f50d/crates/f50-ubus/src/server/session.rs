//! One connection's life as an object server: dial, register every object, answer until the socket or the
//! daemon ends.
//!
//! The session task owns the write half and every task it starts: one reader that forwards whole messages over a
//! bounded channel (framing is not cancel-safe, the channel is), and the invocations in a `JoinSet`. Nothing is
//! read while [`MAX_CONCURRENT_INVOCATIONS`] are running, which pushes back on ubusd instead of queueing without
//! bound. Returning drops both sets, aborting whatever is still running.

use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use tokio::io::BufReader;
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};
use tokio::sync::mpsc;
use tokio::task::{Id, JoinSet};
use tokio_util::sync::CancellationToken;

use super::add_object_msg;
use super::invoke::{self, Done, MAX_CONCURRENT_INVOCATIONS, ReplyTo};
use crate::conn::dial;
use crate::frame::{read_message, write_message};
use crate::proto::{Message, attr, msg, status};
use crate::{Invoke, ObjectSpec, UbusError};

/// A write ubusd does not take within this long means the connection is wedged.
const WRITE_TIMEOUT: Duration = Duration::from_secs(10);
/// Messages the reader may hold ahead of the session.
const INBOX: usize = 32;

/// Why a session ended.
#[derive(Debug)]
pub(crate) enum End {
    Shutdown,
    /// The connection failed; `registered` says whether every object had been registered on it.
    Lost {
        registered: bool,
        error: UbusError,
    },
}

struct Session<'a> {
    objects: &'a [ObjectSpec],
    write: OwnedWriteHalf,
    /// ADD_OBJECT seq → object index, until its STATUS arrives.
    adding: HashMap<u16, usize>,
    /// ubusd's object id → object index.
    ids: HashMap<u32, usize>,
    /// Where each running invocation answers, in case its task panics.
    running: HashMap<Id, ReplyTo>,
}

pub(crate) async fn run(socket: &Path, objects: &[ObjectSpec], invoke: &Invoke, shutdown: &CancellationToken) -> End {
    let lost = |error| End::Lost { registered: false, error };
    let (stream, client_id) = match dial(socket).await {
        Ok(s) => s,
        Err(e) => return lost(e),
    };
    let (read, write) = stream.into_split();
    let mut reader = JoinSet::new();
    let (tx, mut inbox) = mpsc::channel(INBOX);
    reader.spawn(read_loop(read, tx));

    let mut s = Session { objects, write, adding: HashMap::new(), ids: HashMap::new(), running: HashMap::new() };
    if let Err(e) = s.register_all().await {
        return lost(e);
    }
    tracing::debug!(client_id, objects = objects.len(), "ubus server connected");

    let mut calls: JoinSet<Done> = JoinSet::new();
    let mut registered = objects.is_empty();
    let error = loop {
        tokio::select! {
            biased;
            () = shutdown.cancelled() => return End::Shutdown,
            Some(joined) = calls.join_next_with_id() => {
                if let Err(e) = s.finish(joined).await {
                    break e;
                }
            }
            next = inbox.recv(), if calls.len() < MAX_CONCURRENT_INVOCATIONS => {
                let m = match next {
                    Some(Ok(m)) => m,
                    Some(Err(e)) => break e,
                    None => break UbusError::Io(std::io::ErrorKind::UnexpectedEof.into()),
                };
                match s.handle(m, invoke, &mut calls).await {
                    Ok(true) if !registered => {
                        registered = true;
                        tracing::info!(objects = objects.len(), "ubus objects registered");
                    }
                    Ok(_) => {}
                    Err(e) => break e,
                }
            }
        }
    };
    End::Lost { registered, error }
}

async fn read_loop(read: OwnedReadHalf, tx: mpsc::Sender<Result<Message, UbusError>>) {
    let mut read = BufReader::new(read);
    loop {
        let next = match read_message(&mut read).await {
            Ok(Some(m)) => Ok(m),
            Ok(None) => Err(UbusError::Io(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "ubusd closed the connection",
            ))),
            Err(e) => Err(e),
        };
        let stop = next.is_err();
        if tx.send(next).await.is_err() || stop {
            return;
        }
    }
}

impl Session<'_> {
    async fn send(&mut self, m: &Message) -> Result<(), UbusError> {
        tokio::time::timeout(WRITE_TIMEOUT, write_message(&mut self.write, m)).await.map_err(|_| UbusError::Timeout)?
    }

    /// Sends ADD_OBJECT for every object; the replies are handled as they arrive.
    async fn register_all(&mut self) -> Result<(), UbusError> {
        for (i, spec) in self.objects.iter().enumerate() {
            // seq 0 is avoided only for readability in traces; ubusd does not care
            let seq = u16::try_from(i + 1).map_err(|_| UbusError::Protocol("too many objects".to_owned()))?;
            let m = add_object_msg(spec, seq)?;
            self.send(&m).await?;
            self.adding.insert(seq, i);
        }
        Ok(())
    }

    /// Handles one message from ubusd. `Ok(true)` once the last registration has been confirmed.
    async fn handle(&mut self, m: Message, invoke: &Invoke, calls: &mut JoinSet<Done>) -> Result<bool, UbusError> {
        match m.header.ty {
            msg::DATA if m.header.peer == 0 => {
                if let Some(&i) = self.adding.get(&m.header.seq)
                    && let Some(id) = m.attrs()?.u32(attr::OBJID)
                {
                    self.ids.insert(id, i);
                }
            }
            msg::STATUS if m.header.peer == 0 => {
                if let Some(i) = self.adding.remove(&m.header.seq) {
                    let code = m.attrs()?.u32(attr::STATUS).unwrap_or(status::INVALID_ARGUMENT);
                    let name = self.objects.get(i).map_or("?", |o| o.name.as_str());
                    if code != status::OK {
                        tracing::error!(object = name, code, "ubusd refused the object");
                        return Err(UbusError::Status(code));
                    }
                    if !self.ids.values().any(|&v| v == i) {
                        return Err(UbusError::Protocol(format!("ubusd registered {name} without an id")));
                    }
                    return Ok(self.adding.is_empty());
                }
            }
            msg::INVOKE => match invoke::parse(&m, self.objects, &self.ids) {
                Ok(call) => {
                    let to = call.to;
                    let handle = calls.spawn(invoke::run(call, invoke.clone()));
                    self.running.insert(handle.id(), to);
                }
                Err(Some((to, code))) => {
                    tracing::debug!(code, objid = to.objid, "ubus invoke refused");
                    if let Some(reply) = invoke::refusal(to, code) {
                        self.send(&reply).await?;
                    }
                }
                Err(None) => tracing::warn!("ubus invoke without an object id dropped"),
            },
            // ubusd_send_pong: the same message back as DATA
            msg::PING => {
                let pong = Message { header: crate::proto::Header { ty: msg::DATA, ..m.header }, body: m.body };
                self.send(&pong).await?;
            }
            other => tracing::debug!(ty = other, "ubus server ignored a message"),
        }
        Ok(false)
    }

    /// Sends the answer of a finished invocation; a panicked one answers UNKNOWN_ERROR.
    async fn finish(&mut self, joined: Result<(Id, Done), tokio::task::JoinError>) -> Result<(), UbusError> {
        let done = match joined {
            Ok((id, done)) => {
                self.running.remove(&id);
                done
            }
            Err(e) => {
                let Some(to) = self.running.remove(&e.id()) else {
                    return Ok(());
                };
                tracing::error!(error = %e, "ubus invocation task failed");
                Done { to, result: Err(status::UNKNOWN_ERROR) }
            }
        };
        for reply in invoke::replies(done) {
            self.send(&reply).await?;
        }
        Ok(())
    }
}
