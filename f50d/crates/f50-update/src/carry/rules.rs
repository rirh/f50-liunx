//! What kind of path a path is, for the carry plan (the predicates of the shell's `carry_plan`, for OpenWrt).
//!
//! - *never carried*: what is below /proc /sys /dev /run /tmp /var/tmp /var/cache /var/run /var/lock, the Linux
//!   partition's mount point, and the carry's own working names;
//! - *other*: handled by another step (the accounts) or the package manager's state (its database, `/etc/apk/world`),
//!   or the new image's own description of itself (its version, package lists and manifest);
//! - *vendor*: the device's own files (firmware, Android's userspace, hand-built modules), never in a release;
//! - *project*: the project's programs (`opt/mu300`, `usr/libexec/unisoc-modem`, the `mu300-*` init scripts, the
//!   commands an image links into /usr/local/bin): the release's version is in use;
//! - *script*: the scripts that start and wire services (`/etc/init.d`, `/etc/hotplug.d`, ...): programs, not settings;
//! - *user area*: /etc /var /home /root /srv /mnt /media /usr/local /opt: where the user's version of a file wins.

use std::collections::{HashMap, HashSet};

/// `p` is `dir` or below it (`dir` a list of alternatives without slashes at the ends).
pub fn under(p: &str, dirs: &[&str]) -> bool {
    dirs.iter().any(|d| p.len() > d.len() && p.starts_with(d) && p.as_bytes()[d.len()] == b'/')
}

fn at_or_under(p: &str, dirs: &[&str]) -> bool {
    dirs.contains(&p) || under(p, dirs)
}

pub fn parent(p: &str) -> &str {
    p.rfind('/').map_or("", |i| &p[..i])
}

pub fn basename(p: &str) -> &str {
    p.rfind('/').map_or(p, |i| &p[i + 1..])
}

pub fn never(p: &str) -> bool {
    under(
        p,
        &[
            "proc",
            "sys",
            "dev",
            "run",
            "tmp",
            "var/tmp",
            "var/cache",
            "var/run",
            "var/lock",
            "lost+found",
            "mnt/mu300-disk",
            "mnt/f50-disk",
        ],
    ) || at_or_under(p, &[".mu300-image", ".mu300-carry", ".f50-carry", ".f50-carried"])
}

pub fn other(p: &str) -> bool {
    const ACCOUNTS: &[&str] = &["passwd", "group", "shadow", "gshadow", "subuid", "subgid"];
    if let Some(f) = p.strip_prefix("etc/")
        && (ACCOUNTS.contains(&f) || ACCOUNTS.iter().any(|a| f.strip_suffix('-') == Some(a)))
    {
        return true;
    }
    const IMAGE_OWN: &[&str] =
        &["image-version", "image-packages", "packages.txt", "image-manifest.gz", "update-report.new"];
    if p.strip_prefix("etc/mu300/").is_some_and(|f| IMAGE_OWN.contains(&f)) {
        return true;
    }
    matches!(p, "etc/.pwd.lock" | "etc/ld.so.cache" | "etc/.mu300-accounts-from-image" | "etc/apk/world")
        || at_or_under(p, &["etc/mu300/user-packages", "lib/apk", "usr/lib/opkg"])
}

pub fn vendor(p: &str) -> bool {
    under(p, &["opt/mu300/android", "lib/firmware", "usr/lib/firmware", "lib/modules"])
}

pub fn project(p: &str, cmds: &HashSet<String>) -> bool {
    if under(p, &["opt/mu300", "usr/libexec/unisoc-modem"]) {
        return true;
    }
    if let Some(n) = p.strip_prefix("etc/init.d/")
        && n.starts_with("mu300-")
        && !n.contains('/')
    {
        return true;
    }
    if let Some(n) = p.strip_prefix("usr/local/bin/")
        && !n.contains('/')
    {
        return cmds.contains(n);
    }
    false
}

pub fn script(p: &str) -> bool {
    under(p, &["etc/init.d", "etc/hotplug.d", "etc/hotplug-preinit.d", "etc/rc.button", "etc/board.d"])
}

pub fn user_area(p: &str) -> bool {
    under(p, &["etc", "var", "home", "root", "srv", "mnt", "media", "usr/local", "opt"])
}

/// Changes under /var are the system's own as much as the user's: carried without a line in the report.
pub fn quiet(p: &str) -> bool {
    under(p, &["var"])
}

/// A service's enable link (`/etc/rc.d/S50cron`).
pub fn enable_link(p: &str) -> bool {
    p.strip_prefix("etc/rc.d/").is_some_and(|n| !n.contains('/') && (n.starts_with('S') || n.starts_with('K')))
}

pub fn service_file(p: &str) -> bool {
    p.strip_prefix("etc/init.d/").is_some_and(|n| !n.is_empty() && !n.contains('/'))
}

/// The service an enable link is for (`S50cron` -> `cron`).
pub fn service_of(p: &str) -> &str {
    let b = basename(p);
    if p.starts_with("etc/rc.d/") {
        b.trim_start_matches(['S', 'K']).trim_start_matches(|c: char| c.is_ascii_digit())
    } else {
        b
    }
}

/// What protects the device: without a manifest the user's version of these wins (the fallback's rule).
pub fn secure(p: &str) -> bool {
    at_or_under(
        p,
        &[
            "etc/config",
            "etc/sudoers",
            "etc/sudoers.d",
            "etc/sysctl.conf",
            "etc/sysctl.d",
            "etc/nftables.conf",
            "etc/nftables.d",
            "etc/firewall.user",
            "etc/security",
            "etc/hosts.allow",
            "etc/hosts.deny",
            "etc/pam.d",
            "etc/modprobe.d",
            "etc/ssl/private",
            "etc/uhttpd.crt",
            "etc/uhttpd.key",
            "etc/rc.local",
            "etc/rc.d",
            "etc/crontabs",
            "etc/cron.d",
        ],
    )
}

/// The places an update always kept, before there were manifests (the fallback's list for OpenWrt).
pub const KEEP_LIST: &[&str] =
    &["etc/config", "etc/mu300", "etc/unisoc-modem", "etc/dropbear", "etc/rc.local", "etc/crontabs", "root"];

pub fn kept_place(p: &str) -> bool {
    at_or_under(p, KEEP_LIST)
}

/// Every permission bit of mode `a` is one of mode `b`, and they differ: `a` is the narrower.
pub fn narrower(a: u32, b: u32) -> bool {
    a != b && a & !b & 0o7777 == 0
}

/// `p` or a directory above it is listed (a package's conffiles name a directory as "dir/").
pub fn listed(set: &HashSet<String>, p: &str) -> bool {
    if set.contains(p) {
        return true;
    }
    let mut d = parent(p);
    while !d.is_empty() {
        if set.contains(&format!("{d}/")) {
            return true;
        }
        d = parent(d);
    }
    false
}

/// What apk says of a system: the package of each path, the configuration files, the installed packages.
#[derive(Debug, Default, Clone)]
pub struct PkgDb {
    pub owned: HashMap<String, String>,
    pub conf: HashSet<String>,
    pub installed: HashSet<String>,
}

impl PkgDb {
    /// `lib/apk/db/installed` (P: package, F: directory, R: file) and `lib/apk/packages/*.conffiles*` (absolute paths).
    pub fn parse(installed: &str, conffiles: &[String]) -> PkgDb {
        let mut db = PkgDb::default();
        let (mut pkg, mut dir) = (String::new(), String::new());
        for l in installed.lines() {
            if let Some(v) = l.strip_prefix("P:") {
                pkg = v.to_owned();
                db.installed.insert(pkg.clone());
            } else if let Some(v) = l.strip_prefix("F:") {
                dir = v.to_owned();
                if !dir.is_empty() {
                    db.owned.insert(dir.clone(), pkg.clone());
                }
            } else if let Some(v) = l.strip_prefix("R:") {
                let f = if dir.is_empty() { v.to_owned() } else { format!("{dir}/{v}") };
                db.owned.insert(f, pkg.clone());
            }
        }
        for c in conffiles {
            for l in c.lines() {
                if let Some(p) = l.strip_prefix('/').filter(|p| !p.is_empty()) {
                    db.conf.insert(p.to_owned());
                }
            }
        }
        db
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_of_paths() {
        assert!(never("proc/1/stat") && never("tmp/x") && !never("tmpfile") && never(".f50-carry/x"));
        assert!(
            other("etc/shadow")
                && other("etc/passwd-")
                && other("etc/mu300/image-version")
                && other("lib/apk/db/installed")
        );
        assert!(!other("etc/mu300/hotspot.conf"));
        assert!(vendor("lib/firmware/wcn.bin") && vendor("lib/modules/6.18.57/x.ko") && !vendor("lib/libc.so"));
        let cmds: HashSet<String> = ["mu300-at".to_owned()].into();
        assert!(project("opt/mu300/bin/x", &cmds) && project("etc/init.d/mu300-atd", &cmds));
        assert!(project("usr/local/bin/mu300-at", &cmds) && !project("usr/local/bin/mine", &cmds));
        assert!(script("etc/init.d/cron") && user_area("etc/config/network") && !user_area("usr/bin/ls"));
        assert!(enable_link("etc/rc.d/S50cron") && !enable_link("etc/rc.d/README"));
        assert_eq!(service_of("etc/rc.d/S50cron"), "cron");
        assert!(secure("etc/config/firewall") && secure("etc/crontabs/root") && !secure("etc/hosts"));
        assert!(kept_place("root/.profile") && kept_place("etc/config"));
    }

    #[test]
    fn modes_and_lists() {
        assert!(narrower(0o600, 0o644) && narrower(0o700, 0o755));
        assert!(!narrower(0o644, 0o644) && !narrower(0o666, 0o644) && !narrower(0o4755, 0o755));
        let conf: HashSet<String> = ["etc/config/".to_owned(), "etc/dropbear/dropbear_rsa_host_key".to_owned()].into();
        assert!(listed(&conf, "etc/config/network") && listed(&conf, "etc/dropbear/dropbear_rsa_host_key"));
        assert!(!listed(&conf, "etc/dropbear/authorized_keys"));
    }

    #[test]
    fn apk_database() {
        let db = PkgDb::parse(
            "P:cron\nV:1\nF:usr/sbin\nR:crond\nZ:Q1abc=\n\nP:dropbear\nF:etc/init.d\nR:dropbear\n",
            &["/etc/config/dropbear\n/etc/dropbear/\n".to_owned()],
        );
        assert_eq!(db.owned.get("usr/sbin/crond").map(String::as_str), Some("cron"));
        assert_eq!(db.owned.get("etc/init.d/dropbear").map(String::as_str), Some("dropbear"));
        assert!(db.installed.contains("cron") && db.conf.contains("etc/dropbear/"));
    }
}
