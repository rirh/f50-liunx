//! adb's state: on or off (`/etc/f50/adb`), whom it lets in (`/etc/f50/adb_auth`: open or keys), the authorised
//! keys (`/etc/f50/adb_keys`, `/root/.android/adb_keys`).
//!
//! On unless turned off: no file is a device that was never asked, `off` is somebody's choice that stays, and a file
//! that says neither is a damaged choice: adb stays off. Open (the owner's decision of 2026-10-13, FINDINGS 70: every
//! computer on the cable gets a root shell, no key asked) is one of two things and nothing else: `open` in a whole
//! adb_auth that is root's alone, or no state at all in a directory that is root's alone. Everything else is keys -
//! "keys", an adb_auth that is empty, unreadable, a link, writable by others or says anything else, no adb_auth on a
//! device set up when adb took keys only (adb on, or a key authorised), a settings directory that is not root's
//! alone. In keys mode with no key nobody gets in. Ported from `mu300-adbd` (`auth_read`, `trusted`) and `mu300-usb
//! adb`.

use std::os::unix::fs::MetadataExt;
use std::path::Path;

use f50_core::{Error, Result, fsx};

use crate::Net;

/// Root's, and writable by nobody else (a file or directory another user could write to decides nothing here); never
/// a link.
pub(crate) fn trusted(p: &Path, owner: u32) -> bool {
    std::fs::symlink_metadata(p).is_ok_and(|m| (m.is_file() || m.is_dir()) && m.uid() == owner && m.mode() & 0o022 == 0)
}

fn there(p: &Path) -> bool {
    std::fs::symlink_metadata(p).is_ok()
}

/// The word in `/etc/f50/adb`, `None` without the file.
fn conf(net: &Net) -> Option<String> {
    let p = net.legacy("adb");
    there(&p).then(|| std::fs::read_to_string(&p).unwrap_or_default().trim().to_owned())
}

/// adb is to run.
pub(crate) fn on(net: &Net) -> bool {
    conf(net).is_none_or(|c| c == "on")
}

/// The file says neither on nor off.
pub(crate) fn damaged(net: &Net) -> bool {
    conf(net).is_some_and(|c| c != "on" && c != "off")
}

fn has_keys(net: &Net) -> bool {
    [net.legacy("adb_keys"), net.path("/root/.android/adb_keys")]
        .iter()
        .any(|f| std::fs::read_to_string(f).is_ok_and(|t| t.lines().any(|l| l.starts_with("QAAAA"))))
}

/// `open`, or `keys` with the reason when nobody wrote "keys".
pub(crate) fn read(net: &Net) -> (&'static str, Option<String>) {
    let owner = 0;
    let file = net.legacy("adb_auth");
    let dir = file.parent().map(Path::to_path_buf).unwrap_or_default();
    if !trusted(&dir, owner) {
        return ("keys", Some(format!("{} is missing or not root's alone", dir.display())));
    }
    if there(&file) {
        return match std::fs::read_to_string(&file).map(|t| t.trim().to_owned()).as_deref() {
            Ok("keys") => ("keys", None),
            Ok("open") if trusted(&file, owner) => ("open", None),
            Ok("open") => ("keys", Some(format!("{} is not a file of root's alone", file.display()))),
            _ => ("keys", Some(format!("{} is empty, unreadable or says neither open nor keys", file.display()))),
        };
    }
    if conf(net).is_some_and(|c| c != "off") {
        return ("keys", Some("adb was turned on when it took keys only (no adb_auth)".into()));
    }
    if has_keys(net) {
        return ("keys", Some("a key was authorised when adb took keys only (no adb_auth)".into()));
    }
    ("open", None)
}

/// The number of authorised keys.
pub(crate) fn key_count(net: &Net) -> usize {
    [net.legacy("adb_keys"), net.path("/root/.android/adb_keys")]
        .iter()
        .filter_map(|f| std::fs::read_to_string(f).ok())
        .map(|t| t.lines().filter(|l| l.starts_with("QAAAA")).count())
        .sum()
}

/// The whole file or the old one, root's, 644: anyone may read whom adb lets in, only root says so.
fn write(net: &Net, name: &str, word: &str) -> Result<()> {
    let p = net.legacy(name);
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d).map_err(|e| Error::io(d.display(), &e))?;
    }
    fsx::write_atomic(&p, format!("{word}\n").as_bytes(), 0o644)
}

/// Writes down whom adb lets in now where nothing says so yet, before "on", "off" or a key is written: without it,
/// "on" and a key would read as a device set up when adb took keys only (adding a key to an open device would close
/// it to every other computer).
fn keep_auth(net: &Net) -> Result<()> {
    if there(&net.legacy("adb_auth")) {
        return Ok(());
    }
    let (mode, _) = read(net);
    write(net, "adb_auth", mode)
}

pub(crate) fn set_on(net: &Net, on: bool) -> Result<()> {
    keep_auth(net)?;
    write(net, "adb", if on { "on" } else { "off" })
}

pub(crate) fn set_auth(net: &Net, mode: &str) -> Result<()> {
    write(net, "adb_auth", mode)
}

/// An RSA-2048 key in Android's format (adbkey.pub): 700 base64 characters, `QAAAA` (64 words) first, padded.
pub(crate) fn key_ok(k: &str) -> bool {
    k.len() == 700
        && k.starts_with("QAAAA")
        && k.strip_suffix('=')
            .is_some_and(|body| body.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'+' || b == b'/'))
}

/// The key's name, as it may be kept: `A-Za-z0-9@._:-`, 64 characters at most.
pub(crate) fn key_name(c: &str) -> String {
    c.chars().filter(|ch| ch.is_ascii_alphanumeric() || "@._:-".contains(*ch)).take(64).collect()
}

/// Authorises `line` (`BASE64 user@host`); `false` when it was there already.
pub(crate) fn add_key(net: &Net, line: &str) -> Result<bool> {
    let (k, c) = line.trim().split_once(' ').unwrap_or((line.trim(), ""));
    if !key_ok(k) {
        return Err(Error::bad_value("key"));
    }
    keep_auth(net)?;
    let p = net.legacy("adb_keys");
    let old = fsx::read_opt(&p)?.unwrap_or_default();
    if old.lines().any(|l| l.split_whitespace().next() == Some(k)) {
        return Ok(false);
    }
    let name = key_name(c);
    let mut new = old;
    if !new.is_empty() && !new.ends_with('\n') {
        new.push('\n');
    }
    new.push_str(&format!("{k} {}\n", if name.is_empty() { "unnamed" } else { &name }));
    fsx::write_atomic(&p, new.as_bytes(), 0o600)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys() {
        let k = format!("QAAAA{}=", "A".repeat(694));
        assert!(key_ok(&k));
        assert!(!key_ok(&format!("QAAAB{}=", "A".repeat(694))));
        assert!(!key_ok(&format!("QAAAA{}", "A".repeat(695))));
        assert!(!key_ok(&format!("QAAAA{}*=", "A".repeat(693))));
        assert_eq!(key_name("me@host; rm -rf /"), "me@hostrm-rf");
    }
}
