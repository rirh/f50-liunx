//! The report of a carry (the shell's `carry_report`): what was carried, set aside or put back, for people, on top of
//! the new system's `etc/f50/update-report` (`etc/mu300/...` before the rename) (what earlier updates wrote stays below it). Root's alone: it names the
//! user's files. `update-report.new` beside it says it has not been read yet.

use std::fs;
use std::path::Path;

use f50_core::{Result, fsx};

use super::plan::Plan;
use crate::release::assets;

const TITLES: &[(char, &str)] = &[
    ('u', "Yours is in use where the new release has another version (the release's is in {kept}/image):"),
    ('g', "Yours is in use; the new release no longer has this file:"),
    ('i', "The version of the release is in use; yours is in {kept}/user:"),
    ('f', "Differed from the new release and were replaced; as they were: {kept}/user:"),
    ('x', "The release has a file or a link where this directory was; what you had in it: {kept}/user:"),
    ('d', "Deleted again, as you had deleted them:"),
    ('k', "Back, though you had deleted them (the system needs them, or it cannot be told):"),
    (
        'p',
        "Packages you had removed are part of the image, and back (their services are as the previous system had them):",
    ),
    ('e', "Left off, as in the previous system (the new release enables them):"),
    ('l', "Accounts of the release that the previous system did not have are locked (passwd NAME sets a password):"),
    ('m', "The owner or mode you gave these is the one of the release again:"),
    (
        'o',
        "Owned by a number that no account had in the previous system and an account of the new one has: not put in place, but in {kept}/user:",
    ),
    ('a', "Accounts that could not be carried over (the files they own keep the number):"),
    ('n', "Not made (device nodes, FIFOs, sockets):"),
];

/// The report's text.
pub fn text(os: &str, from: &str, to: &str, when: &str, plan: &Plan, gone: u64, kept_dir: &str) -> String {
    let kept = format!("{kept_dir}/{from}");
    let mut s = format!("==== {os}: {from} -> {to}, {when} ====\n");
    s += &format!(
        "Everything you added to the system was carried over: {} files, {} MB.\n",
        plan.files,
        plan.blocks_live / 2048
    );
    if plan.fallback {
        s += "The image this system came from has no list of its files here, so a file you changed could not be\n\
              told from one the old release shipped: where the new release has another file, the new one is in use,\n\
              and a file of the image that you had deleted is back.\n";
    }
    for (code, title) in TITLES {
        let items: Vec<&str> = plan.notes.iter().filter(|(c, _)| c == code).map(|(_, p)| p.as_str()).collect();
        if items.is_empty() {
            continue;
        }
        s += &title.replace("{kept}", &kept);
        s.push('\n');
        let pre = if matches!(code, 'p' | 'l' | 'a') { "" } else { "/" };
        for i in items {
            s += &format!("  {pre}{}\n", i.replace('\n', "\\n"));
        }
    }
    if gone > 0 {
        s += &format!("Files that were there when the update looked and gone when it came to carry them: {gone}\n");
    }
    if plan.package_files > 0 {
        s += &format!(
            "{} files of packages installed on the device are not carried: install those packages again (apk add).\n",
            plan.package_files
        );
    }
    s.push('\n');
    s
}

/// Where `new` keeps what updates set aside, as a path of that system (`/etc/f50/update-kept`).
pub fn kept_dir(new: &Path) -> String {
    let p = assets::within(new, assets::KEPT);
    format!("/{}", p.strip_prefix(new).unwrap_or(&p).display())
}

/// The report on top of `new`'s, root's alone; the "not read yet" mark when it says more than the totals.
pub fn write(new: &Path, text: &str, mark: bool) -> Result<()> {
    let path = assets::within(new, assets::REPORT);
    let mut all = text.to_owned();
    if let Ok(m) = fs::symlink_metadata(&path)
        && m.file_type().is_file()
        && let Ok(old) = fs::read_to_string(&path)
    {
        // the newest first; the whole of it bounded
        all.push_str(f50_core::sanitize::truncate_bytes(&old, 256 * 1024));
    }
    if let Some(d) = path.parent() {
        fs::create_dir_all(d).map_err(|e| f50_core::Error::io(d.display(), &e))?;
    }
    let _ = fs::remove_file(&path);
    fsx::write_atomic(&path, all.as_bytes(), 0o600)?;
    let note = path.with_file_name("update-report.new");
    if mark {
        return fsx::write_atomic(&note, b"", 0o600);
    }
    let _ = fs::remove_file(&note);
    Ok(())
}
