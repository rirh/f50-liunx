//! Accounts are settings too (the shell's `merge_accounts` and the end of `carry_accounts`). `/etc/passwd`, `group`,
//! `shadow` and `gshadow` are never carried as files (the new image's accounts would be lost); instead:
//!
//! - every account both systems have keeps the old system's password (its shadow entry) and its group memberships
//!   are merged;
//! - the accounts the old system added (users from 1000 on, and system accounts a package made) come along, as long
//!   as their number is free in the new system; else the report names them (their files keep the number);
//! - an account of the new image that the old system did not have (the user deleted it, or the release added it)
//!   must not come with the image's well-known password or none: it is locked, and said.
//!
//! Each file is replaced by a rename, with the owner and mode of the new image's file: a crash never leaves half a
//! passwd.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Write;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt, lchown};
use std::path::Path;

use f50_core::{Error, Result};

fn read(path: &Path) -> Option<Vec<String>> {
    let m = fs::symlink_metadata(path).ok()?;
    if !m.file_type().is_file() {
        return None;
    }
    Some(fs::read_to_string(path).ok()?.lines().filter(|l| !l.is_empty()).map(str::to_owned).collect())
}

fn name(l: &str) -> &str {
    l.split(':').next().unwrap_or("")
}

fn num(l: &str) -> Option<u32> {
    l.split(':').nth(2)?.parse().ok()
}

fn write_like(path: &Path, lines: &[String]) -> Result<()> {
    let md = fs::symlink_metadata(path).map_err(|e| Error::io(path.display(), &e))?;
    let tmp = path.with_extension("f50-new");
    let _ = fs::remove_file(&tmp);
    let mut text = lines.join("\n");
    text.push('\n');
    let w = || -> std::io::Result<()> {
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&tmp)?;
        f.write_all(text.as_bytes())?;
        lchown(&tmp, Some(md.uid()), Some(md.gid()))?;
        f.set_permissions(fs::Permissions::from_mode(md.mode() & 0o7777))?;
        f.sync_all()?;
        fs::rename(&tmp, path)
    };
    w().map_err(|e| {
        let _ = fs::remove_file(&tmp);
        Error::io(format!("write {}", path.display()), &e)
    })
}

/// The ids each system has names for: (uids, gids).
pub fn ids(root: &Path) -> (HashSet<u32>, HashSet<u32>) {
    let get = |f: &str| read(&root.join("etc").join(f)).unwrap_or_default().iter().filter_map(|l| num(l)).collect();
    (get("passwd"), get("group"))
}

/// Brings the old system's accounts into the new one. The report's lines: `a` (not brought), `l` (locked).
pub fn merge(old: &Path, new: &Path) -> Result<Vec<(char, String)>> {
    let mut notes = Vec::new();
    let etc = |r: &Path, f: &str| r.join("etc").join(f);
    let (Some(op), Some(np)) = (read(&etc(old, "passwd")), read(&etc(new, "passwd"))) else { return Ok(notes) };
    let og = read(&etc(old, "group")).unwrap_or_default();
    let ng = read(&etc(new, "group")).unwrap_or_default();
    // who comes along: an account the new system lacks whose number is free there (a user from 1000 on, or a
    // system account a package made)
    let bring = |old_l: &[String], new_l: &[String], what: &str, notes: &mut Vec<(char, String)>| -> HashSet<String> {
        let have: HashSet<&str> = new_l.iter().map(|l| name(l)).collect();
        let used: HashSet<u32> = new_l.iter().filter_map(|l| num(l)).collect();
        let mut out = HashSet::new();
        for l in old_l {
            let (n, id) = (name(l), num(l));
            if have.contains(n) || n.is_empty() {
                continue;
            }
            match id {
                Some(id) if !used.contains(&id) => {
                    out.insert(n.to_owned());
                }
                _ => notes.push(('a', format!("{n} ({what} {})", id.map_or("?".into(), |i| i.to_string())))),
            }
        }
        out
    };
    let users = bring(&op, &np, "passwd", &mut notes);
    let groups = bring(&og, &ng, "group", &mut notes);
    let old_users: HashSet<&str> = op.iter().map(|l| name(l)).collect();
    for (f, set) in [("passwd", &users), ("shadow", &users), ("group", &groups), ("gshadow", &groups)] {
        let (Some(ol), Some(nl)) = (read(&etc(old, f)), read(&etc(new, f))) else { continue };
        let old_by: HashMap<&str, &String> = ol.iter().map(|l| (name(l), l)).collect();
        let mut out: Vec<String> = Vec::with_capacity(nl.len());
        for l in &nl {
            let n = name(l);
            let line = match (f, old_by.get(n)) {
                ("shadow" | "gshadow", Some(o)) => (*o).clone(),
                ("group", Some(o)) => merge_members(l, o),
                ("shadow", None) if !old_users.contains(n) => lock(l, n, &mut notes),
                _ => l.clone(),
            };
            out.push(line);
        }
        out.extend(ol.iter().filter(|l| set.contains(name(l))).cloned());
        if out != nl {
            write_like(&etc(new, f), &out)?;
        }
    }
    Ok(notes)
}

/// The new group's line with the old one's members added.
fn merge_members(new: &str, old: &str) -> String {
    let mut f: Vec<String> = new.split(':').map(str::to_owned).collect();
    let Some(old_m) = old.split(':').nth(3) else { return new.to_owned() };
    if f.len() < 4 {
        return new.to_owned();
    }
    let mut m: Vec<String> = f[3].split(',').filter(|s| !s.is_empty()).map(str::to_owned).collect();
    for x in old_m.split(',').filter(|s| !s.is_empty()) {
        if !m.iter().any(|y| y == x) {
            m.push(x.to_owned());
        }
    }
    f[3] = m.join(",");
    f.join(":")
}

/// A shadow line, locked unless it is already.
fn lock(l: &str, n: &str, notes: &mut Vec<(char, String)>) -> String {
    let mut f: Vec<&str> = l.split(':').collect();
    if f.len() < 2 || f[1].starts_with('!') || f[1].starts_with('*') {
        return l.to_owned();
    }
    notes.push(('l', n.to_owned()));
    let locked = format!("!{}", f[1]);
    f[1] = &locked;
    f.join(":")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passwords_users_and_locks() {
        let root = std::env::temp_dir().join(format!("f50-update-acct-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let (old, new) = (root.join("old"), root.join("new"));
        for d in [&old, &new] {
            fs::create_dir_all(d.join("etc")).expect("etc");
        }
        let w = |r: &Path, f: &str, s: &str| fs::write(r.join("etc").join(f), s).expect("write");
        w(&old, "passwd", "root:x:0:0::/root:/bin/ash\nalice:x:1000:1000::/home/alice:/bin/ash\n");
        w(&old, "shadow", "root:$6$mine:1::::::\nalice:$6$alice:1::::::\n");
        w(&old, "group", "root:x:0:\nwheel:x:10:alice\nalice:x:1000:\n");
        w(&new, "passwd", "root:x:0:0::/root:/bin/ash\nguest:x:999:999::/:/bin/false\n");
        w(&new, "shadow", "root::0::::::\nguest:$6$known:0::::::\n");
        w(&new, "group", "root:x:0:\nwheel:x:10:\n");
        let notes = merge(&old, &new).expect("merge");
        let r = |f: &str| fs::read_to_string(new.join("etc").join(f)).expect("read");
        assert!(r("shadow").contains("root:$6$mine:") && r("shadow").contains("alice:$6$alice"));
        assert!(r("shadow").contains("guest:!$6$known:"));
        assert!(r("passwd").contains("alice:x:1000:1000"));
        assert!(r("group").contains("wheel:x:10:alice") && r("group").contains("alice:x:1000:"));
        assert!(notes.contains(&('l', "guest".into())));
        let _ = fs::remove_dir_all(&root);
    }
}
