//! The carry plan: for every thing in the old system, what becomes of it in the new one (the shell's `carry_plan`,
//! pass 3, for OpenWrt; docs/research/FINDINGS.md 85). The question is asked the other way round from a keep list:
//! which files of the old system came with its image (the manifest says)? Everything else is the user's.
//!
//! - *Not the image's*: carried to its place. Where the new image has a file of that name, the user's wins in the
//!   user's directories (the release's is set aside, `image/`), elsewhere the release's wins (the user's is set aside,
//!   `user/`).
//! - *The image's, changed by the user*: in the user's directories the user's is used (and the release's set aside
//!   when the release changed it too); elsewhere - programs, the project's own files, a package's init script - the
//!   release's is used and the user's set aside.
//! - *Never less protected than the user left it*: a mode the user narrowed is put on the release's file; an enable
//!   link the user removed stays removed; a package the user removed is back with the image, and its services are as
//!   the old system had them.
//! - *The image's, deleted by the user*: stays deleted in the user's directories (unless it is a program the system
//!   needs, a uci-defaults script, or of a package that was removed and is back); elsewhere it is back.
//! - *A package's files* are not carried file by file (its configuration files are, as the user's): the report names
//!   them; this updater does not install packages again.
//! - *Without a manifest*: everything the new image does not have is carried, the places an update always kept and
//!   what protects the device are the user's, and every other file that differs is set aside.
//!
//! Nothing is dropped without a line of the report.

use std::collections::{HashMap, HashSet};

use super::manifest::{ManEntry, Manifest};
use super::rules::{self, PkgDb, listed};
use super::scan::Entry;

/// What to do, in this order: `Save` (the release's version aside), `Remove` (from the new system), `Live` (carry it),
/// `Vendor` (carry as a second name), `Hold` (the user's version aside), `Meta` (owner and mode on the new file).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Act {
    Save(String),
    Remove(String),
    Live(String),
    Vendor(String),
    Hold(String),
    Meta { mode: u32, uid: u32, gid: u32, path: String },
}

/// The plan and its report lines (`code`, path or name; the codes are `report`'s).
#[derive(Debug, Default)]
pub struct Plan {
    pub acts: Vec<Act>,
    pub notes: Vec<(char, String)>,
    /// 512-byte blocks carried, set aside (user's), set aside (release's).
    pub blocks_live: u64,
    pub blocks_hold: u64,
    pub blocks_save: u64,
    pub files: u64,
    /// Files of packages not carried.
    pub package_files: u64,
    pub fallback: bool,
}

/// The hashes the plan asks for (computed on demand).
pub trait Sums {
    fn old(&mut self, p: &str) -> Option<String>;
    fn new_side(&mut self, p: &str) -> Option<String>;
}

/// What the plan is made from.
#[derive(Debug)]
pub struct Inputs<'a> {
    pub old: &'a [Entry],
    pub new: &'a HashMap<String, Entry>,
    pub manifest: Option<&'a Manifest>,
    pub old_pkg: &'a PkgDb,
    pub new_pkg: &'a PkgDb,
    pub cmds: &'a HashSet<String>,
    /// The uids and gids the old and the new system have names for.
    pub old_ids: &'a (HashSet<u32>, HashSet<u32>),
    pub new_ids: &'a (HashSet<u32>, HashSet<u32>),
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum State {
    Same,
    Meta,
    Mod,
    New,
}

struct Planner<'a, 'b> {
    i: &'b Inputs<'a>,
    h: &'b mut dyn Sums,
    om: Option<&'a Manifest>,
    out: Plan,
}

impl Planner<'_, '_> {
    fn note(&mut self, code: char, p: &str) {
        self.out.notes.push((code, p.to_owned()));
    }

    fn ns(&self, p: &str) -> Option<&Entry> {
        self.i.new.get(p)
    }

    fn owned(&self, p: &str) -> bool {
        self.i.old_pkg.owned.contains_key(p)
    }

    /// A package's file that is not its configuration: it comes with its package.
    fn package_file(&self, p: &str) -> bool {
        self.owned(p) && !listed(&self.i.old_pkg.conf, p)
    }

    /// The owner has no name in the old system, and an account of the new one has it.
    fn foreign(&self, e: &Entry) -> bool {
        let (ou, og) = self.i.old_ids;
        let (nu, ng) = self.i.new_ids;
        (!ou.contains(&e.uid) && nu.contains(&e.uid)) || (!og.contains(&e.gid) && ng.contains(&e.gid))
    }

    fn om(&self, p: &str) -> Option<&ManEntry> {
        self.om.and_then(|m| m.get(p))
    }

    /// The user's version is the one in use.
    fn wins(&self, e: &Entry) -> bool {
        let p = e.path.as_str();
        if !rules::user_area(p) || rules::project(p, self.i.cmds) {
            return false;
        }
        // a script of a package (init.d, hotplug.d) is a program, not a setting
        !(e.kind == 'f' && rules::script(p) && self.package_file(p) && self.om(p).is_none_or(|m| m.kind == 'f'))
    }

    fn state(&mut self, e: &Entry) -> State {
        let Some(m) = self.om(&e.path).cloned() else { return State::New };
        let ty = if e.kind == 'm' { 'd' } else { e.kind };
        if m.kind != ty {
            return State::Mod;
        }
        if ty == 'f' && (m.size != e.size || self.h.old(&e.path) != m.hash) {
            return State::Mod;
        }
        if ty == 'l' && self.h.old(&e.path) != m.hash {
            return State::Mod;
        }
        if m.uid != e.uid || m.gid != e.gid || (ty != 'l' && m.mode != e.mode) {
            return State::Meta;
        }
        State::Same
    }

    fn same_as_new(&mut self, e: &Entry) -> bool {
        let Some(n) = self.ns(&e.path).cloned() else { return false };
        if n.kind != e.kind {
            return false;
        }
        match e.kind {
            'f' => n.size == e.size && self.h.old(&e.path).is_some() && self.h.old(&e.path) == self.h.new_side(&e.path),
            'l' => self.h.old(&e.path).is_some() && self.h.old(&e.path) == self.h.new_side(&e.path),
            k => k != 'd',
        }
    }

    fn new_is_old_image(&mut self, e: &Entry) -> bool {
        let (Some(m), Some(n)) = (self.om(&e.path).cloned(), self.ns(&e.path).cloned()) else { return false };
        if m.kind != n.kind {
            return false;
        }
        match m.kind {
            'f' => m.size == n.size && self.h.new_side(&e.path) == m.hash,
            'l' => self.h.new_side(&e.path) == m.hash,
            _ => true,
        }
    }

    fn live(&mut self, e: &Entry, block: &mut Option<String>) {
        if self.foreign(e) {
            self.hold(e, 'o');
            if e.kind == 'd' || e.kind == 'm' {
                *block = Some(e.path.clone());
            }
            return;
        }
        // the device's own files are not the user's and nothing writes them in place: second names, never a copy
        if e.kind == 'f' && rules::vendor(&e.path) {
            self.out.acts.push(Act::Vendor(e.path.clone()));
            self.out.files += 1;
            return;
        }
        self.out.acts.push(Act::Live(e.path.clone()));
        if e.kind != 'd' && e.kind != 'm' {
            self.out.blocks_live += e.blocks;
            self.out.files += 1;
        }
    }

    fn hold(&mut self, e: &Entry, why: char) {
        self.out.acts.push(Act::Hold(e.path.clone()));
        self.out.blocks_hold += e.blocks;
        if why != ' ' {
            self.note(why, &e.path);
        }
        // the content is the release's; a mode the user narrowed stays narrowed
        if let Some(n) = self.ns(&e.path).cloned()
            && e.kind == 'f'
            && n.kind == 'f'
            && rules::narrower(e.mode, n.mode)
        {
            self.out.acts.push(Act::Meta { mode: e.mode, uid: n.uid, gid: n.gid, path: e.path.clone() });
        }
    }

    fn save(&mut self, p: &str) {
        if let Some(n) = self.ns(p).cloned()
            && n.kind != 'd'
        {
            self.out.acts.push(Act::Save(p.to_owned()));
            self.out.blocks_save += n.blocks;
        }
    }

    fn meta(&mut self, e: &Entry) {
        let Some(n) = self.ns(&e.path).cloned() else { return };
        if e.kind == 'l' || n.kind != e.kind {
            return;
        }
        let p = e.path.clone();
        if self.wins(e) && !self.foreign(e) {
            self.out.acts.push(Act::Meta { mode: e.mode, uid: e.uid, gid: e.gid, path: p });
        } else if rules::narrower(e.mode, n.mode) {
            self.out.acts.push(Act::Meta { mode: e.mode, uid: n.uid, gid: n.gid, path: p });
        } else if e.mode != n.mode || e.uid != n.uid || e.gid != n.gid {
            self.note('m', &p);
        }
    }

    /// No manifest: which file is the old image's own cannot be told.
    fn fallback(&mut self, e: &Entry, block: &mut Option<String>) {
        let p = e.path.as_str();
        let innew = self.ns(p).cloned();
        let k = (rules::kept_place(p) || rules::secure(p)) && !rules::project(p, self.i.cmds) && !rules::script(p);
        if e.kind == 'd' || e.kind == 'm' {
            match innew {
                Some(n) if n.kind == 'd' => {}
                Some(_) if k => {
                    self.out.acts.push(Act::Remove(p.to_owned()));
                    self.live(e, block);
                }
                Some(_) => {
                    *block = Some(p.to_owned());
                    self.hold(e, 'i');
                }
                None if self.owned(p) => {}
                None if rules::project(p, self.i.cmds) && !rules::vendor(p) => {
                    *block = Some(p.to_owned());
                    self.hold(e, 'f');
                }
                None => self.live(e, block),
            }
            return;
        }
        let Some(n) = innew else {
            if rules::vendor(p) {
                self.live(e, block);
            } else if self.package_file(p) {
                self.out.package_files += 1;
            } else if rules::project(p, self.i.cmds) {
                self.hold(e, 'f');
            } else {
                self.live(e, block);
            }
            return;
        };
        if self.same_as_new(e) {
            if e.kind == 'f' && rules::narrower(e.mode, n.mode) {
                self.out.acts.push(Act::Meta { mode: e.mode, uid: n.uid, gid: n.gid, path: p.to_owned() });
            }
            return;
        }
        if rules::vendor(p) {
            return;
        }
        if n.kind == 'd' {
            self.hold(e, 'i');
        } else if k {
            self.live(e, block);
        } else {
            self.hold(e, 'f');
        }
    }

    fn one(&mut self, e: &Entry, block: &mut Option<String>, retyped: &mut HashSet<String>) {
        let p = e.path.clone();
        let innew = self.ns(&p).cloned();
        if self.out.fallback {
            return self.fallback(e, block);
        }
        let st = self.state(e);
        if e.kind == 'd' || e.kind == 'm' {
            let mine = st == State::Mod || (st == State::New && !self.owned(&p));
            match innew {
                Some(n) if n.kind != 'd' => {
                    // the release has a file or a link where the old system has a directory
                    if mine && self.wins(e) {
                        self.save(&p);
                        self.out.acts.push(Act::Remove(p.clone()));
                        self.live(e, block);
                        self.note('u', &p);
                    } else if mine {
                        *block = Some(p.clone());
                        self.hold(e, 'i');
                    } else {
                        *block = Some(p.clone());
                        self.note('x', &p);
                    }
                }
                None if mine => self.live(e, block),
                Some(_) if st == State::Meta => self.meta(e),
                _ => {}
            }
            return;
        }
        match st {
            State::Same => {}
            State::Meta => {
                if innew.is_some() {
                    self.meta(e);
                }
            }
            State::New => {
                if rules::vendor(&p) {
                    if innew.is_none() {
                        self.live(e, block);
                    }
                } else if self.package_file(&p) {
                    self.out.package_files += 1;
                } else if innew.is_none() {
                    self.live(e, block);
                } else if self.same_as_new(e) {
                } else if innew.as_ref().is_some_and(|n| n.kind == 'd') || !self.wins(e) {
                    self.hold(e, 'i');
                } else {
                    self.save(&p);
                    self.live(e, block);
                    self.note('u', &p);
                }
            }
            State::Mod => {
                if self.om(&p).is_some_and(|m| m.kind == 'd') {
                    retyped.insert(p.clone());
                }
                if rules::vendor(&p) || !self.wins(e) {
                    if innew.is_none() || !self.same_as_new(e) {
                        self.hold(e, 'i');
                    }
                } else if innew.is_none() {
                    self.live(e, block);
                    self.note('g', &p);
                } else if self.same_as_new(e) {
                } else if innew.as_ref().is_some_and(|n| n.kind == 'd') {
                    // the user replaced a directory of the image by something else: theirs, as it was
                    if self.om(&p).is_some_and(|m| m.kind == 'd') {
                        self.out.acts.push(Act::Remove(p.clone()));
                        self.live(e, block);
                        self.note('u', &p);
                    } else {
                        self.hold(e, 'i');
                    }
                } else if self.new_is_old_image(e) || rules::quiet(&p) {
                    self.live(e, block);
                } else {
                    self.save(&p);
                    self.live(e, block);
                    self.note('u', &p);
                }
            }
        }
    }
}

/// Makes the plan.
pub fn make(i: &Inputs<'_>, h: &mut dyn Sums) -> Plan {
    let fallback = i.manifest.is_none_or(|m| m.is_empty());
    let mut pl = Planner { i, h, om: i.manifest, out: Plan { fallback, ..Plan::default() } };
    let mut seen: HashSet<&str> = HashSet::new();
    let mut had: HashSet<&str> = HashSet::new();
    let mut retyped: HashSet<String> = HashSet::new();
    let mut block: Option<String> = None;
    let mut block_noted = true;
    for e in i.old {
        let p = e.path.as_str();
        if pl.om(p).is_some() {
            seen.insert(p);
        }
        if fallback && (rules::enable_link(p) || rules::service_file(p) || p.starts_with("etc/sudoers.d/")) {
            had.insert(p);
        }
        if let Some(b) = &block {
            if p.len() > b.len() && p.starts_with(b.as_str()) && p.as_bytes()[b.len()] == b'/' {
                // below a directory that cannot be there: what is not the image's is set aside
                let pkg = pl.om(p).is_none() && pl.package_file(p);
                let keep = e.kind != 's' && !rules::never(p) && !rules::other(p) && !rules::vendor(p);
                if keep && !pkg && (fallback || pl.state(e) != State::Same) {
                    if !block_noted {
                        let b = b.clone();
                        pl.note('x', &b);
                        block_noted = true;
                    }
                    pl.hold(e, ' ');
                }
                continue;
            }
            block = None;
        }
        if e.kind == 's' || rules::never(p) || rules::other(p) {
            continue;
        }
        let before = pl.out.notes.len();
        pl.one(e, &mut block, &mut retyped);
        // a directory that only blocks ("x") is said when something below it is set aside
        if block.as_deref() == Some(p) && pl.out.notes.get(before).is_some_and(|n| n.0 == 'x') {
            pl.out.notes.truncate(before);
            block_noted = false;
        } else {
            block_noted = true;
        }
    }
    finish(&mut pl, &seen, &had, &retyped);
    pl.out
}

#[path = "plan_gone.rs"]
mod gone;
use gone::finish;

#[cfg(test)]
#[path = "plan_tests.rs"]
mod tests;
