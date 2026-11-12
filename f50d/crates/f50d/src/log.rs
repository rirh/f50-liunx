//! Logging: one line per event on stderr, which procd hands to logd (`logread -e f50d`).

use tracing::Level;

/// Installs the subscriber; `level` from the UCI option `f50d.main.log` or the `F50D_LOG` variable.
pub fn init(level: Option<&str>) {
    let lvl = match level.or(std::env::var("F50D_LOG").ok().as_deref()) {
        Some("error") => Level::ERROR,
        Some("warn") => Level::WARN,
        Some("debug") => Level::DEBUG,
        Some("trace") => Level::TRACE,
        _ => Level::INFO,
    };
    let _ = tracing_subscriber::fmt()
        .with_max_level(lvl)
        .with_ansi(false)
        .without_time() // logd stamps every line
        .with_target(false)
        .with_writer(std::io::stderr)
        .try_init();
}
