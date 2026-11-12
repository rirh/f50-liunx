//! f50d's device-independent base.
//!
//! Nothing here knows the F50, OpenWrt or the modem. Every capability crate builds on these pieces:
//!
//! - [`error`]: the closed set of failure codes and the one error type every operation returns;
//! - [`rpc`]: the method model a capability publishes (objects, methods, typed fields) and the request validation
//!   that runs before any handler;
//! - [`exec`]: the only way a program is started (process group, hard timeout, bounded output, secrets on stdin);
//! - [`jobs`]: work that outlives a request, owned by the daemon and cancellable;
//! - [`fsx`] and [`paths`]: files written atomically and privately, under a root that tests can move;
//! - [`sanitize`]: text that leaves the daemon.

pub mod error;
pub mod exec;
pub mod fsx;
pub mod jobs;
pub mod paths;
pub mod rpc;
pub mod sanitize;
pub mod time;

pub use error::{Code, Error, Result};
