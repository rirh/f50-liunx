//! The saved lock settings, replayed at boot: one small file per setting under `<state>/lock/` (so applies never
//! overwrite each other and the replay reads exactly what was applied), the replay switch, and the reset's
//! tombstone. Mode and band locks live in the modem's NV too and survive a reboot (measured on an F50, #96); a cell
//! lock does not. An update once dropped the saved locks (FINDINGS 64): they live under f50d's own state directory.

use std::io;
use std::path::PathBuf;

use f50_core::paths::Paths;
use f50_core::{Error, Result, fsx};

use super::codec::{Cell, Kind};

/// The settings a reset removes and the replay re-sends.
pub const KEYS: [Kind; 5] = [Kind::Mode, Kind::Endc, Kind::Lte, Kind::Nr, Kind::Cell];

fn dir(paths: &Paths) -> PathBuf {
    paths.state().join("lock")
}

fn path(paths: &Paths, key: &str) -> PathBuf {
    dir(paths).join(key)
}

pub fn load(paths: &Paths, kind: Kind) -> Option<String> {
    fsx::read_opt(&path(paths, kind.key())).ok().flatten().map(|s| s.trim().to_owned())
}

pub fn save(paths: &Paths, kind: Kind, value: &str) -> Result<()> {
    fsx::private_dir(&dir(paths))?;
    fsx::write_atomic(&path(paths, kind.key()), value.as_bytes(), 0o600)
}

fn remove(paths: &Paths, key: &str) -> Result<()> {
    match std::fs::remove_file(path(paths, key)) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(Error::io(format!("remove the saved {key}"), &e)),
    }
}

/// Boot replay on (no file, or anything but `off`).
pub fn auto_apply(paths: &Paths) -> bool {
    load(paths, Kind::AutoApply).as_deref() != Some("off")
}

/// Any setting saved to replay.
pub fn any_saved(paths: &Paths) -> bool {
    KEYS.iter().any(|k| path(paths, k.key()).exists())
}

/// The saved cell locks, one per line (commas belong to the pair). `None` when there is no cell file.
pub fn cells(paths: &Paths) -> Option<Vec<Cell>> {
    let s = load(paths, Kind::Cell)?;
    Some(s.lines().filter_map(Cell::parse).collect())
}

fn save_cells(paths: &Paths, cells: &[Cell]) -> Result<()> {
    let body: String = cells.iter().map(|c| format!("{c}\n")).collect();
    save(paths, Kind::Cell, &body)
}

/// One cell lock more; the file is seeded once from the live reading when an older installation had locks before
/// cell persistence existed.
pub fn cell_add(paths: &Paths, seed: &[Cell], c: Cell) -> Result<()> {
    let mut all = cells(paths).unwrap_or_else(|| seed.to_vec());
    if !all.contains(&c) {
        all.push(c);
    }
    save_cells(paths, &all)
}

/// The cell locks of one RAT removed (`None`: all of them).
pub fn cell_remove(paths: &Paths, seed: &[Cell], nr: Option<bool>) -> Result<()> {
    let mut all = cells(paths).unwrap_or_else(|| seed.to_vec());
    match nr {
        None => all.clear(),
        Some(nr) => all.retain(|c| c.nr != nr),
    }
    save_cells(paths, &all)
}

/// A reset that committed (its tombstone) and was interrupted before every saved setting was gone is finished
/// first, before anything reads or saves them; an error when a setting could not be removed.
pub fn finish_reset(paths: &Paths) -> Result<bool> {
    if !path(paths, "reset_pending").exists() {
        return Ok(false);
    }
    for k in KEYS {
        remove(paths, k.key())?;
    }
    remove(paths, "reset_pending")?;
    Ok(true)
}

/// The reset's commit: the tombstone first, then the saved settings (the replay switch stays).
pub fn commit_reset(paths: &Paths) -> Result<()> {
    fsx::private_dir(&dir(paths))?;
    fsx::write_atomic(&path(paths, "reset_pending"), b"1", 0o600)?;
    finish_reset(paths).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> Paths {
        let d = std::env::temp_dir().join(format!("f50-modem-lock-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        Paths::new(d)
    }

    #[test]
    fn cells_and_reset() {
        let p = tmp("cells");
        let a = Cell::parse("nr:627264,393").unwrap();
        let b = Cell::parse("lte:1650,211").unwrap();
        cell_add(&p, &[b], a).unwrap();
        assert_eq!(cells(&p), Some(vec![b, a]));
        cell_remove(&p, &[], Some(true)).unwrap();
        assert_eq!(cells(&p), Some(vec![b]));
        save(&p, Kind::Mode, "sa").unwrap();
        save(&p, Kind::AutoApply, "off").unwrap();
        assert!(any_saved(&p) && !auto_apply(&p));
        commit_reset(&p).unwrap();
        assert!(!any_saved(&p));
        assert!(!auto_apply(&p)); // the replay switch stays
        let _ = std::fs::remove_dir_all(p.root());
    }
}
