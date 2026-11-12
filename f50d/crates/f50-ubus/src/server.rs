//! [`crate::serve`]: keeps f50d's objects published on ubusd for the life of the daemon.
//!
//! Each connection is one [`session`]: hello, ADD_OBJECT for every object, then answering invocations. When ubusd
//! goes away (restart, upgrade) the session ends and a new one starts after a backoff that grows from
//! [`BACKOFF_MIN`] to [`BACKOFF_MAX`] and resets once a session has registered everything. ubusd frees a client's
//! objects when its socket closes, so nothing has to be removed on the way out.

mod invoke;
mod session;

use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Duration;

use tokio_util::sync::CancellationToken;

use crate::blob::proto_err;
use crate::blobmsg;
use crate::proto::{Body, Message, attr, msg};
use crate::{Invoke, ObjectSpec, UbusError};

pub use invoke::{INVOKE_TIMEOUT, MAX_CONCURRENT_INVOCATIONS};

/// First wait before dialling ubusd again.
pub const BACKOFF_MIN: Duration = Duration::from_millis(200);
/// Longest wait between attempts.
pub const BACKOFF_MAX: Duration = Duration::from_secs(10);

pub(crate) async fn serve(
    socket: PathBuf,
    objects: Vec<ObjectSpec>,
    invoke: Invoke,
    shutdown: CancellationToken,
) -> Result<(), UbusError> {
    validate(&objects)?;
    let mut backoff = BACKOFF_MIN;
    loop {
        let end = tokio::select! {
            biased;
            () = shutdown.cancelled() => return Ok(()),
            end = session::run(&socket, &objects, &invoke, &shutdown) => end,
        };
        match end {
            session::End::Shutdown => return Ok(()),
            session::End::Lost { registered, error } => {
                if registered {
                    backoff = BACKOFF_MIN;
                    tracing::warn!(error = %error, "ubus connection lost; registering again");
                } else {
                    tracing::warn!(error = %error, retry_in = ?backoff, "ubus registration failed");
                }
            }
        }
        tokio::select! {
            biased;
            () = shutdown.cancelled() => return Ok(()),
            () = tokio::time::sleep(backoff) => {}
        }
        backoff = (backoff * 2).min(BACKOFF_MAX);
    }
}

/// Refuses specs ubusd could never accept, so a typo fails at start-up instead of retrying forever.
fn validate(objects: &[ObjectSpec]) -> Result<(), UbusError> {
    let mut names = HashSet::new();
    for o in objects {
        if o.name.is_empty() || !names.insert(o.name.as_str()) {
            return Err(UbusError::Protocol(format!("object name {:?} is empty or repeated", o.name)));
        }
        let mut methods = HashSet::new();
        for m in &o.methods {
            if m.name.is_empty() || !methods.insert(m.name.as_str()) {
                return Err(UbusError::Protocol(format!("{}: method name {:?} is empty or repeated", o.name, m.name)));
            }
        }
        // encoding checks NULs, name lengths and the message size limit
        add_object_msg(o, 0)?;
    }
    Ok(())
}

/// `ubus_add_object` with a fresh type: OBJPATH, then SIGNATURE holding one blobmsg table per method, each mapping
/// argument name to its blobmsg type as INT32 (`ubus_push_object_type`, `ubus_push_method_data`).
pub(crate) fn add_object_msg(spec: &ObjectSpec, seq: u16) -> Result<Message, UbusError> {
    Body::new()
        .string(attr::OBJPATH, &spec.name)?
        .nested(attr::SIGNATURE, |w| {
            for m in &spec.methods {
                let table = blobmsg::start_named(w, blobmsg::TYPE_TABLE, &m.name)?;
                for (arg, ty) in &m.args {
                    if arg.is_empty() {
                        return Err(proto_err("empty argument name"));
                    }
                    blobmsg::put_value(w, arg, &serde_json::Value::from(ty.blobmsg_type()))?;
                }
                w.end(table)?;
            }
            Ok(())
        })?
        .finish(msg::ADD_OBJECT, seq, 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ArgType, MethodSpec};

    fn spec() -> ObjectSpec {
        ObjectSpec {
            name: "o".to_owned(),
            methods: vec![MethodSpec { name: "m".to_owned(), args: vec![("a".to_owned(), ArgType::String)] }],
        }
    }

    #[test]
    fn add_object_bytes() -> Result<(), UbusError> {
        let m = add_object_msg(&spec(), 1)?;
        assert_eq!(
            m.to_bytes(),
            vec![
                0, 6, 0, 1, 0, 0, 0, 0, // ADD_OBJECT, seq 1, peer 0
                0, 0, 0, 0x24, // root: 4 + 8 + 24
                0x02, 0, 0, 0x06, b'o', 0, 0, 0, // OBJPATH "o"
                0x06, 0, 0, 0x18, // SIGNATURE: 4 + 20
                0x82, 0, 0, 0x14, 0, 1, b'm', 0, // table "m"
                0x85, 0, 0, 0x0c, 0, 1, b'a', 0, 0, 0, 0, 3, // "a": INT32 BLOBMSG_TYPE_STRING
            ]
        );
        Ok(())
    }

    #[test]
    fn arg_type_codes() {
        assert_eq!(ArgType::Array.blobmsg_type(), 1);
        assert_eq!(ArgType::Table.blobmsg_type(), 2);
        assert_eq!(ArgType::String.blobmsg_type(), 3);
        assert_eq!(ArgType::Int32.blobmsg_type(), 5);
        assert_eq!(ArgType::Bool.blobmsg_type(), 7);
    }

    #[test]
    fn validate_rejects_duplicates_and_empties() {
        assert!(validate(&[spec()]).is_ok());
        assert!(validate(&[spec(), spec()]).is_err());
        let mut no_name = spec();
        no_name.name.clear();
        assert!(validate(&[no_name]).is_err());
        let mut dup_method = spec();
        dup_method.methods.push(dup_method.methods[0].clone());
        assert!(validate(&[dup_method]).is_err());
    }
}
