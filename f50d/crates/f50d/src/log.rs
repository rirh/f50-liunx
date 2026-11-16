//! Logging: one line per event, which procd hands to logd (`logread -e f50d`).
//!
//! procd logs a service's stdout at `daemon.info` and its stderr at `daemon.err`, whatever the line says. The
//! services ([`init_service`]) therefore write warnings and errors to stderr and everything else to stdout (their
//! init scripts set both `stdout 1` and `stderr 1`), so `logread` shows an INFO line as `daemon.info`, not
//! `daemon.err`. The short commands ([`init`]) keep every line on stderr: their stdout is their output.

use tracing::Level;
use tracing_subscriber::fmt::writer::MakeWriterExt;

/// Installs the subscriber of a short command (all on stderr); `level` from `F50D_LOG`.
pub fn init(level: Option<&str>) {
    let _ = tracing_subscriber::fmt()
        .with_max_level(max_level(level))
        .with_ansi(false)
        .without_time() // logd stamps every line
        .with_target(false)
        .with_writer(std::io::stderr)
        .try_init();
}

/// Installs the subscriber of a procd service: WARN and ERROR on stderr, the rest on stdout. `level` from the UCI
/// option `f50d.main.log` or the `F50D_LOG` variable.
pub fn init_service(level: Option<&str>) {
    let _ = tracing_subscriber::fmt()
        .with_max_level(max_level(level))
        .with_ansi(false)
        .without_time()
        .with_target(false)
        .with_writer(std::io::stderr.with_max_level(Level::WARN).or_else(std::io::stdout))
        .try_init();
}

fn max_level(level: Option<&str>) -> Level {
    match level.or(std::env::var("F50D_LOG").ok().as_deref()) {
        Some("error") => Level::ERROR,
        Some("warn") => Level::WARN,
        Some("debug") => Level::DEBUG,
        Some("trace") => Level::TRACE,
        _ => Level::INFO,
    }
}
