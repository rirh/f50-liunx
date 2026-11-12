//! f50d's own settings: `/etc/config/f50d`, section `main`. Everything has a default; the file may be absent.
//!
//! ```text
//! config f50d 'main'
//!     option log 'info'            # error | warn | info | debug
//!     option factory_reset '0'     # 1: the reset key held 10 s wipes the settings and reboots
//!     option adbd '1'              # 0: f50d leaves adbd alone (another service runs it)
//! ```

use f50_platform::Platform;

#[derive(Debug, Clone)]
pub struct Config {
    pub log: Option<String>,
    pub factory_reset: bool,
    /// f50d supervises adbd (the default). Off while another service owns it (a trial beside the old system).
    pub adbd: bool,
}

impl Default for Config {
    fn default() -> Self {
        Config { log: None, factory_reset: false, adbd: true }
    }
}

/// Reads the config through f50-platform's UCI parser (a file read: no process, no ubus).
pub async fn load(platform: &Platform) -> Config {
    let Ok(sections) = platform.uci().load("f50d").await else { return Config::default() };
    let Some(main) = sections.iter().find(|s| s.name == "main") else { return Config::default() };
    Config {
        log: main.get("log").map(str::to_owned),
        factory_reset: main.get("factory_reset") == Some("1"),
        adbd: main.get("adbd") != Some("0"),
    }
}
