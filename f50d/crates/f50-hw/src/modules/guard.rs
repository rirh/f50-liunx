//! The kernel's module helper (`f50-modprobe-guard`, FINDINGS 163).
//!
//! When code asks the kernel for something a module provides - a socket of a protocol family, a filesystem, a
//! netlink family, a link kind - the kernel runs its helper (`kernel.modprobe`) as root: `HELPER -q -- REQUEST`. Any
//! user can make it do that (`socket(AF_TIPC, ...)` is enough), so every installed module would be one any user can
//! load. The guard refuses a request that resolves to a module of [`BLOCKED`] - by its name or by one of its
//! `alias=` strings, with modprobe's glob rules and its `-` = `_` - and hands everything else to the real modprobe.
//! Root still loads a refused module on purpose with `modprobe NAME` (that is not the helper), and an allow file
//! turns the guard off.
//!
//! The alias table takes ~2 s to make, so [`prepare`] makes it at boot before the guard is installed; a request only
//! remakes a stale one. Without a table (the cache directory is not root's own) the names are checked and the kinds
//! of request the list is about fail closed. A fault here would stop every on-demand load (crypto, netfilter,
//! filesystems), so whatever cannot be decided is handed on.

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};

use f50_core::fsx;

/// The modules not loaded on demand, one per line (`#` comments), every module depending on one of them included.
pub(crate) const BLOCKED: &str = include_str!("autoload-blocked");

/// The table's first line: which list it was made from (a new binary with another list remakes it).
fn header() -> String {
    let h = BLOCKED.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ u64::from(b)).wrapping_mul(0x100_0000_01b3));
    format!("# f50d modprobe-guard {h:016x}")
}

fn listed() -> BTreeSet<String> {
    BLOCKED
        .lines()
        .map(|l| l.split('#').next().unwrap_or("").trim())
        .filter(|l| !l.is_empty())
        .map(str::to_owned)
        .collect()
}

/// `-` and `_` are one character to modprobe, except inside `[...]` of an alias pattern.
pub(crate) fn norm(s: &str) -> String {
    let mut in_br = false;
    s.chars()
        .map(|c| {
            match c {
                '[' => in_br = true,
                ']' => in_br = false,
                _ => {}
            }
            if c == '-' && !in_br { '_' } else { c }
        })
        .collect()
}

fn module_name(path: &str) -> String {
    let base = path.rsplit('/').next().unwrap_or(path);
    let base = base.split(".ko").next().unwrap_or(base);
    base.replace('-', "_")
}

/// The listed modules and every module whose dependencies (`modules.dep`, where there is one) reach a listed one.
fn names(moddir: &Path) -> BTreeSet<String> {
    let mut bad = listed();
    let Ok(dep) = std::fs::read_to_string(moddir.join("modules.dep")) else { return bad };
    let deps: Vec<(String, Vec<String>)> = dep
        .lines()
        .filter_map(|l| l.split_once(':'))
        .map(|(m, d)| (module_name(m), d.split_whitespace().map(module_name).collect()))
        .collect();
    loop {
        let more: Vec<String> = deps
            .iter()
            .filter(|(m, d)| !bad.contains(m) && d.iter().any(|x| bad.contains(x)))
            .map(|(m, _)| m.clone())
            .collect();
        if more.is_empty() {
            return bad;
        }
        bad.extend(more);
    }
}

/// The `alias=` strings of an uncompressed module file (its modinfo section is NUL-separated `key=value`).
fn aliases(ko: &Path) -> Vec<String> {
    let Ok(data) = std::fs::read(ko) else { return Vec::new() };
    data.split(|&b| b == 0)
        .filter_map(|s| s.strip_prefix(b"alias="))
        .filter_map(|a| std::str::from_utf8(a).ok())
        .filter(|a| !a.is_empty() && !a.contains(char::is_whitespace))
        .map(str::to_owned)
        .collect()
}

fn ko_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        match e.file_type() {
            Ok(t) if t.is_dir() => ko_files(&p, out),
            Ok(t) if t.is_file() && e.file_name().to_string_lossy().contains(".ko") => out.push(p),
            _ => {}
        }
    }
}

/// The table: `PATTERN MODULE` lines, the modules' own names first. A compressed module contributes its name only
/// (OpenWrt's are not compressed; the shell asked modinfo, which f50d does not run per module).
pub(crate) fn make_table(moddir: &Path) -> String {
    let want = names(moddir);
    let mut out = vec![header()];
    out.extend(want.iter().map(|m| format!("{m} {m}")));
    let mut files = Vec::new();
    ko_files(moddir, &mut files);
    files.sort();
    for f in files {
        let m = module_name(&f.to_string_lossy());
        if !want.contains(&m) || !f.to_string_lossy().ends_with(".ko") {
            continue;
        }
        out.extend(aliases(&f).into_iter().map(|a| format!("{} {m}", norm(&a))));
    }
    out.join("\n") + "\n"
}

/// Where the guard keeps its tables, and which kernel's.
#[derive(Debug, Clone)]
pub(crate) struct Guard {
    pub cache: PathBuf,
    pub moddir: PathBuf,
    pub krel: String,
}

impl Guard {
    fn table_path(&self) -> PathBuf {
        let safe: String =
            self.krel.chars().map(|c| if c.is_ascii_alphanumeric() || "._-".contains(c) { c } else { '_' }).collect();
        self.cache.join(safe)
    }

    /// This kernel's table, made again when missing, from another list, or older than the modules; `None` when it
    /// cannot be had (a cache directory that is not root's own: [`fsx::private_dir`] refuses it).
    pub(crate) fn fresh_table(&self) -> Option<String> {
        let t = self.table_path();
        let current = std::fs::read_to_string(&t).ok().filter(|s| s.lines().next() == Some(header().as_str()));
        let newer_modules = || {
            let tm = std::fs::metadata(&t).and_then(|m| m.modified()).ok();
            let mm = std::fs::metadata(&self.moddir).and_then(|m| m.modified()).ok();
            matches!((tm, mm), (Some(tm), Some(mm)) if mm > tm)
        };
        if let Some(s) = current
            && !newer_modules()
        {
            return Some(s);
        }
        if fsx::private_dir(&self.cache).is_err() {
            return std::fs::read_to_string(&t).ok().filter(|s| s.lines().count() > 1);
        }
        let table = make_table(&self.moddir);
        // a table that cannot be made again leaves the one there was (if any)
        if fsx::write_atomic(&t, table.as_bytes(), 0o600).is_err() {
            return std::fs::read_to_string(&t).ok();
        }
        Some(table)
    }

    /// The module a request resolves to when it is refused, `None` when it is handed on.
    pub(crate) fn refused(&self, request: &str) -> Option<String> {
        let req = norm(request);
        if let Some(t) = self.fresh_table() {
            return t
                .lines()
                .skip(1)
                .filter_map(|l| l.split_once(' '))
                .find(|(p, _)| fnmatch(p, &req))
                .map(|(_, m)| m.to_owned());
        }
        if let Some(m) = listed().into_iter().find(|m| *m == req) {
            return Some(m);
        }
        ["net_pf_", "fs_", "tty_ldisc_", "rtnl_"]
            .iter()
            .any(|k| req.starts_with(k))
            .then(|| "no alias table".to_owned())
    }
}

/// modprobe's `fnmatch(pattern, s, 0)`: `*`, `?`, `[...]` (with `!`/`^` and ranges), `\` escapes.
pub(crate) fn fnmatch(p: &str, s: &str) -> bool {
    let (p, s): (Vec<char>, Vec<char>) = (p.chars().collect(), s.chars().collect());
    let (mut pi, mut si) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while si < s.len() {
        let step = match p.get(pi) {
            Some('*') => {
                star = Some((pi, si));
                pi += 1;
                continue;
            }
            Some('?') => Some(pi + 1),
            Some('[') => class(&p, pi, s[si]),
            Some('\\') if pi + 1 < p.len() => (p[pi + 1] == s[si]).then_some(pi + 2),
            Some(&c) => (c == s[si]).then_some(pi + 1),
            None => None,
        };
        match (step, star) {
            (Some(next), _) => {
                pi = next;
                si += 1;
            }
            (None, Some((sp, ss))) => {
                pi = sp + 1;
                si = ss + 1;
                star = Some((sp, ss + 1));
            }
            (None, None) => return false,
        }
    }
    p[pi..].iter().all(|&c| c == '*')
}

/// A bracket expression at `p[i]` against `c`: the index after it when `c` matches; a `[` without its `]` is literal.
fn class(p: &[char], i: usize, c: char) -> Option<usize> {
    let mut j = i + 1;
    let neg = matches!(p.get(j), Some('!' | '^'));
    if neg {
        j += 1;
    }
    let mut hit = false;
    let mut first = true;
    while j < p.len() && (first || p[j] != ']') {
        first = false;
        if j + 2 < p.len() && p[j + 1] == '-' && p[j + 2] != ']' {
            hit |= (p[j]..=p[j + 2]).contains(&c);
            j += 3;
        } else {
            hit |= p[j] == c;
            j += 1;
        }
    }
    if j >= p.len() {
        return (c == '[').then_some(i + 1);
    }
    (hit != neg).then_some(j + 1)
}

/// What the kernel runs: refuse (exit status 1, with a line in the kernel log) or hand on to modprobe (this process
/// is replaced; the status is 127 only when that fails).
pub(crate) fn run_helper(guard: &Guard, allow: &[PathBuf], modprobe: &Path, kmsg: &Path, args: &[OsString]) -> i32 {
    let name = args.last().map(|a| a.to_string_lossy().into_owned()).unwrap_or_default();
    let off = name.is_empty() || allow.iter().any(|a| a.exists());
    if !off && let Some(hit) = guard.refused(&name) {
        let fix = if hit == "no alias table" { name.clone() } else { hit.clone() };
        crate::timeline::kmsg(
            kmsg,
            "f50d",
            &format!("on-demand loading of {name} ({hit}) refused (root: modprobe {fix}; FINDINGS 163)"),
        );
        return 1;
    }
    let e = std::process::Command::new(modprobe).args(args).exec();
    tracing::error!("modprobe-guard: cannot run {}: {e}", modprobe.display());
    127
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn globs_are_modprobes() {
        assert!(fnmatch("net_pf_30", "net_pf_30"));
        assert!(fnmatch("net_pf_*", "net_pf_30"));
        assert!(!fnmatch("net_pf_3", "net_pf_30"));
        assert!(fnmatch("fs_hf?", "fs_hfs"));
        assert!(fnmatch("pci:v*d*sv*", "pci:v00001234d0001sv0002"));
        assert!(fnmatch("tty_ldisc_[0-9]", "tty_ldisc_7"));
        assert!(!fnmatch("tty_ldisc_[!0-9]", "tty_ldisc_7"));
        assert!(fnmatch("a[-]b", "a-b"));
        assert!(!fnmatch("*a", "b"));
    }

    #[test]
    fn requests_are_refused_by_module() {
        assert_eq!(norm("net-pf-30"), "net_pf_30");
        assert_eq!(norm("a-[x-z]-b"), "a_[x-z]_b");
        assert!(listed().contains("tipc") && listed().contains("vhost_vsock"));
        let d = std::env::temp_dir().join(format!("f50-hw-guard-{}", std::process::id()));
        let moddir = d.join("lib/modules/5.4.254");
        std::fs::create_dir_all(&moddir).expect("mkdir");
        std::fs::write(moddir.join("tipc.ko"), b"\0license=GPL\0alias=net-pf-30\0alias=tipc_*\0").expect("ko");
        std::fs::write(moddir.join("l2tp_ppp.ko"), b"\0alias=net-pf-24-proto-1\0").expect("ko");
        std::fs::write(moddir.join("modules.dep"), "kernel/x/tipc_x.ko: tipc.ko\nl2tp_ppp.ko:\n").expect("dep");
        let g = Guard { cache: d.join("cache"), moddir: moddir.clone(), krel: "5.4.254".into() };
        assert_eq!(g.refused("net-pf-30").as_deref(), Some("tipc"));
        assert_eq!(g.refused("tipc_diag").as_deref(), Some("tipc_diag"));
        assert_eq!(g.refused("tipc_x").as_deref(), Some("tipc_x"));
        assert_eq!(g.refused("net-pf-24-proto-1"), None);
        assert_eq!(g.refused("crypto-aes"), None);
        let _ = std::fs::remove_dir_all(&d);
    }
}
