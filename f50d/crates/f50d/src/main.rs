//! f50d: the ZTE F50's device daemon for OpenWrt.
//!
//! One binary, several roles, chosen by its first argument (see [`cli`]): the daemon procd keeps running, the AT
//! descriptor keeper, the boot stages, and the small commands netifd, hotplug and scripts call. Everything that
//! talks to the running daemon does so over ubus.

mod boot;
mod bus;
mod cli;
mod config;
mod daemon;
mod internal;
mod log;
mod wiring;

use std::process::ExitCode;

fn main() -> ExitCode {
    cli::run(std::env::args_os().collect())
}
