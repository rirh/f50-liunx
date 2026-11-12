//! The method model every capability publishes, independent of the transport.
//!
//! A capability crate builds one or more [`Object`]s (`f50.sms`, `f50.modem`, ...). Each [`Method`] declares its
//! [`Access`] (which ACL grant it belongs to), its typed [`Field`]s, and an async handler that receives checked
//! [`Params`]. The daemon publishes the objects on ubus; `dispatch` is the one path from a raw request to a reply:
//! look up, validate, run, wrap in the envelope.
//!
//! Two rules hold for every method: an unknown field is refused (`ubus_rpc_session`, which uhttpd adds, excepted),
//! and a string that is not a secret may not start with "-" (it may reach a program's command line).

mod field;
mod method;
mod params;

pub use field::{Field, Rule, UBUS_SESSION, WireType};
pub use method::{Access, Method, Object, ObjectBuilder, Reply, dispatch, envelope};
pub use params::{Params, check_word, is_ascii_line, is_text_line, matches, validate};

/// The largest request accepted, in bytes of JSON.
pub const REQUEST_MAX: usize = 64 * 1024;
