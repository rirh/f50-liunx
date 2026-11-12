//! mu300d: one JSON API for the MU300 / ZTE F50 / U30 Air router, over the device tools in /opt/mu300/bin.
//!
//! Every surface (`mu300d call` over adb, `mu300d serve` over HTTP) goes through `api::registry::dispatch`: one
//! method table, one validation path, one envelope.

pub mod api;
pub mod at_guard;
pub mod auth;
pub mod cache;
pub mod config;
pub mod exec;
pub mod http;
pub mod jobs;
pub mod methods;
pub mod sanitize;
pub mod sys;
