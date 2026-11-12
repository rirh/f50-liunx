//! wpa_supplicant's control interface, spoken directly over its Unix datagram socket (what `wpa_cli` does; OpenWrt's
//! wpad builds do not all ship `wpa_cli`). A request is one datagram, its reply the next one that is not an event
//! (`<N>...`); after `ATTACH` the socket also receives the events. Secrets (a passphrase in `SET_NETWORK`) cross only
//! this socket, never a command line or a log.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use f50_core::{Error, Result};
use tokio::net::UnixDatagram;

const REPLY_MAX: usize = 64 * 1024;
static SEQ: AtomicU64 = AtomicU64::new(0);

/// One connection to the supplicant of an interface.
#[derive(Debug)]
pub(crate) struct Ctrl {
    sock: UnixDatagram,
    local: PathBuf,
}

impl Ctrl {
    /// Connects to `dir/ifname`, from a socket of ours in `run_dir` (private: the supplicant answers there).
    pub async fn open(dir: &Path, ifname: &str, run_dir: &Path) -> Result<Ctrl> {
        f50_core::fsx::private_dir(run_dir)?;
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let local = run_dir.join(format!("wpa_ctrl_{}_{n}", std::process::id()));
        let _ = std::fs::remove_file(&local);
        let sock = UnixDatagram::bind(&local).map_err(|e| Error::io("wpa_supplicant control socket", &e))?;
        let ctrl = Ctrl { sock, local };
        ctrl.sock
            .connect(dir.join(ifname))
            .map_err(|e| Error::failed(format!("wpa_supplicant is not running on {ifname}: {e}")))?;
        Ok(ctrl)
    }

    /// One request and its reply (events in between are skipped).
    pub async fn request(&self, cmd: &str, timeout: Duration) -> Result<String> {
        self.sock.send(cmd.as_bytes()).await.map_err(|e| Error::io("wpa_supplicant", &e))?;
        let deadline = tokio::time::Instant::now() + timeout;
        let mut buf = vec![0u8; REPLY_MAX];
        loop {
            let n = tokio::time::timeout_at(deadline, self.sock.recv(&mut buf))
                .await
                .map_err(|_| Error::timeout("wpa_supplicant"))?
                .map_err(|e| Error::io("wpa_supplicant", &e))?;
            let msg = String::from_utf8_lossy(&buf[..n]);
            if !msg.starts_with('<') {
                return Ok(msg.into_owned());
            }
        }
    }

    /// A request whose reply must be `OK` (the verb only in an error: the arguments may be a secret).
    pub async fn ok(&self, cmd: &str) -> Result<()> {
        let r = self.request(cmd, Duration::from_secs(5)).await?;
        let verb = cmd.split_whitespace().next().unwrap_or("");
        if r.trim() == "OK" { Ok(()) } else { Err(Error::failed(format!("wpa_supplicant refused {verb}"))) }
    }

    /// Events from now on.
    pub async fn attach(&self) -> Result<()> {
        self.ok("ATTACH").await
    }

    /// The next event (`<3>CTRL-EVENT-CONNECTED ...`), without its priority.
    pub async fn event(&self) -> Result<String> {
        let mut buf = vec![0u8; REPLY_MAX];
        loop {
            let n = self.sock.recv(&mut buf).await.map_err(|e| Error::io("wpa_supplicant", &e))?;
            let msg = String::from_utf8_lossy(&buf[..n]);
            if let Some(rest) = msg.strip_prefix('<') {
                return Ok(rest.split_once('>').map_or(rest, |(_, m)| m).to_owned());
            }
        }
    }
}

impl Drop for Ctrl {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.local);
    }
}

/// `key=value` lines (`STATUS`, `SIGNAL_POLL`): one value.
pub(crate) fn field<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    text.lines().find_map(|l| l.strip_prefix(key)?.strip_prefix('='))
}

/// A string as the supplicant's configuration takes it in `SET_NETWORK`: quoted (it reads up to the last quote, so
/// quotes inside are kept). The caller has checked there is no control character.
pub(crate) fn quoted(s: &str) -> String {
    format!("\"{s}\"")
}

/// Bytes as hex (a network name in `SET_NETWORK ssid`: any byte goes as it is).
pub(crate) fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fields_and_encodings() {
        let st = "bssid=aa:bb:cc:dd:ee:ff\nssid=Home\nwpa_state=COMPLETED\n";
        assert_eq!(field(st, "wpa_state"), Some("COMPLETED"));
        assert_eq!(field(st, "ssid"), Some("Home"));
        assert_eq!(field(st, "ss"), None);
        assert_eq!(hex("Ç a".as_bytes()), "c38720 61".replace(' ', ""));
        assert_eq!(quoted("a\"b"), "\"a\"b\"");
    }
}
