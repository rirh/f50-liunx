//! The carry rules, on made-up systems (the cases of tests/test_carry.py that apply to OpenWrt).

use std::collections::{HashMap, HashSet};

use super::super::manifest::{ManEntry, Manifest};
use super::super::rules::PkgDb;
use super::super::scan::Entry;
use super::*;

fn e(kind: char, mode: u32, size: u64, path: &str) -> Entry {
    Entry { kind, mode, uid: 0, gid: 0, size, nlink: 1, dev: 1, ino: 0, blocks: 8, path: path.into() }
}

fn m(kind: char, mode: u32, size: u64, hash: &str) -> ManEntry {
    ManEntry { kind, mode, uid: 0, gid: 0, size, hash: (hash != "-").then(|| hash.into()) }
}

struct Fake {
    old: HashMap<String, String>,
    new: HashMap<String, String>,
}

impl Sums for Fake {
    fn old(&mut self, p: &str) -> Option<String> {
        self.old.get(p).cloned()
    }
    fn new_side(&mut self, p: &str) -> Option<String> {
        self.new.get(p).cloned()
    }
}

struct World {
    old: Vec<Entry>,
    new: HashMap<String, Entry>,
    man: Manifest,
    sums: Fake,
    old_pkg: PkgDb,
    new_pkg: PkgDb,
}

impl World {
    fn new() -> Self {
        World {
            old: vec![
                e('d', 0o755, 0, "etc"),
                e('d', 0o755, 0, "etc/config"),
                e('d', 0o755, 0, "usr"),
                e('d', 0o755, 0, "usr/bin"),
            ],
            new: ["etc", "etc/config", "usr", "usr/bin"]
                .iter()
                .map(|p| ((*p).to_owned(), e('d', 0o755, 0, p)))
                .collect(),
            man: ["etc", "etc/config", "usr", "usr/bin"]
                .iter()
                .map(|p| ((*p).to_owned(), m('d', 0o755, 0, "-")))
                .collect(),
            sums: Fake { old: HashMap::new(), new: HashMap::new() },
            old_pkg: PkgDb::default(),
            new_pkg: PkgDb::default(),
        }
    }

    /// A file in the old image (`img`: its hash there), the old system (`old`), the new image (`new`).
    fn file(&mut self, p: &str, img: Option<&str>, old: Option<(&str, u32)>, new: Option<&str>) {
        if let Some(h) = img {
            self.man.insert(p.into(), m('f', 0o644, 1, h));
        }
        if let Some((h, mode)) = old {
            self.old.push(e('f', mode, 1, p));
            self.sums.old.insert(p.into(), h.into());
        }
        if let Some(h) = new {
            self.new.insert(p.into(), e('f', 0o644, 1, p));
            self.sums.new.insert(p.into(), h.into());
        }
    }

    fn plan(&mut self, with_manifest: bool) -> Plan {
        let ids = (HashSet::from([0]), HashSet::from([0]));
        let cmds = HashSet::new();
        let man = with_manifest.then_some(&self.man);
        let i = Inputs {
            old: &self.old,
            new: &self.new,
            manifest: man,
            old_pkg: &self.old_pkg,
            new_pkg: &self.new_pkg,
            cmds: &cmds,
            old_ids: &ids,
            new_ids: &ids,
        };
        make(&i, &mut self.sums)
    }
}

fn has(p: &Plan, a: &Act) -> bool {
    p.acts.contains(a)
}

#[test]
fn the_users_new_file_is_carried() {
    let mut w = World::new();
    w.file("etc/wireguard.conf", None, Some(("a", 0o600)), None);
    let p = w.plan(true);
    assert!(has(&p, &Act::Live("etc/wireguard.conf".into())));
    assert!(p.notes.is_empty());
}

#[test]
fn a_changed_setting_wins_and_the_release_version_is_set_aside() {
    let mut w = World::new();
    w.file("etc/config/network", Some("img"), Some(("mine", 0o644)), Some("rel"));
    let p = w.plan(true);
    assert!(has(&p, &Act::Save("etc/config/network".into())));
    assert!(has(&p, &Act::Live("etc/config/network".into())));
    assert!(p.notes.contains(&('u', "etc/config/network".into())));
    // the release did not change it: the user's, no line
    let mut w = World::new();
    w.file("etc/config/network", Some("img"), Some(("mine", 0o644)), Some("img"));
    let p = w.plan(true);
    assert!(has(&p, &Act::Live("etc/config/network".into())) && p.notes.is_empty());
}

#[test]
fn a_changed_program_is_the_releases() {
    let mut w = World::new();
    w.file("usr/bin/tool", Some("img"), Some(("mine", 0o755)), Some("rel"));
    let p = w.plan(true);
    assert!(has(&p, &Act::Hold("usr/bin/tool".into())));
    assert!(!has(&p, &Act::Live("usr/bin/tool".into())));
    assert!(p.notes.contains(&('i', "usr/bin/tool".into())));
}

#[test]
fn an_unchanged_image_file_is_left_to_the_release() {
    let mut w = World::new();
    w.file("etc/profile", Some("img"), Some(("img", 0o644)), Some("rel"));
    let p = w.plan(true);
    assert!(p.acts.is_empty() && p.notes.is_empty());
}

#[test]
fn a_narrowed_mode_stays_narrowed() {
    let mut w = World::new();
    w.file("usr/bin/tool", Some("img"), Some(("img", 0o644)), Some("img"));
    w.man.get_mut("usr/bin/tool").expect("m").mode = 0o755;
    w.old.last_mut().expect("e").mode = 0o700;
    w.new.get_mut("usr/bin/tool").expect("n").mode = 0o755;
    let p = w.plan(true);
    assert!(has(&p, &Act::Meta { mode: 0o700, uid: 0, gid: 0, path: "usr/bin/tool".into() }));
}

#[test]
fn deleted_by_the_user_stays_deleted_in_the_users_places_only() {
    let mut w = World::new();
    w.file("etc/banner", Some("img"), None, Some("rel"));
    w.file("usr/bin/tool", Some("img"), None, Some("rel"));
    w.file("etc/uci-defaults/99-x", Some("img"), None, Some("rel"));
    let p = w.plan(true);
    assert!(has(&p, &Act::Remove("etc/banner".into())) && p.notes.contains(&('d', "etc/banner".into())));
    assert!(!has(&p, &Act::Remove("usr/bin/tool".into())));
    assert!(!has(&p, &Act::Remove("etc/uci-defaults/99-x".into())), "a uci-defaults script must run again");
}

#[test]
fn a_disabled_service_stays_off() {
    let mut w = World::new();
    w.man.insert("etc/rc.d".into(), m('d', 0o755, 0, "-"));
    w.old.push(e('d', 0o755, 0, "etc/rc.d"));
    w.new.insert("etc/rc.d".into(), e('d', 0o755, 0, "etc/rc.d"));
    w.man.insert("etc/rc.d/S50cron".into(), m('l', 0o777, 1, "t"));
    w.new.insert("etc/rc.d/S50cron".into(), e('l', 0o777, 1, "etc/rc.d/S50cron"));
    let p = w.plan(true);
    assert!(has(&p, &Act::Remove("etc/rc.d/S50cron".into())));
}

#[test]
fn never_carried_and_package_files() {
    let mut w = World::new();
    w.old.push(e('d', 0o755, 0, "tmp"));
    w.file("tmp/x", None, Some(("a", 0o644)), None);
    w.file("etc/shadow", Some("img"), Some(("mine", 0o600)), Some("rel"));
    w.file("usr/bin/htop", None, Some(("a", 0o755)), None);
    w.old_pkg.owned.insert("usr/bin/htop".into(), "htop".into());
    let p = w.plan(true);
    assert!(p.acts.iter().all(|a| !matches!(a, Act::Live(x) if x.starts_with("tmp") || x == "etc/shadow")));
    assert_eq!(p.package_files, 1);
}

#[test]
fn a_file_where_the_release_has_a_directory_is_set_aside() {
    let mut w = World::new();
    w.file("etc/thing", None, Some(("a", 0o644)), None);
    w.new.insert("etc/thing".into(), e('d', 0o755, 0, "etc/thing"));
    let p = w.plan(true);
    assert!(has(&p, &Act::Hold("etc/thing".into())) && p.notes.contains(&('i', "etc/thing".into())));
}

#[test]
fn without_a_manifest() {
    let mut w = World::new();
    w.file("etc/config/network", None, Some(("mine", 0o644)), Some("rel"));
    w.file("etc/profile", None, Some(("mine", 0o644)), Some("rel"));
    w.file("etc/mine", None, Some(("mine", 0o644)), None);
    let p = w.plan(false);
    assert!(p.fallback);
    assert!(has(&p, &Act::Live("etc/config/network".into())), "a kept place");
    assert!(has(&p, &Act::Hold("etc/profile".into())) && p.notes.contains(&('f', "etc/profile".into())));
    assert!(has(&p, &Act::Live("etc/mine".into())));
}
