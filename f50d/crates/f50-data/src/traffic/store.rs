//! Where the count and its settings live, and their settings file.
//!
//! Flash wear decides the layout: the live ledger is in RAM (`/var/run/f50d/traffic/ledger`, rewritten every
//! round, so a restarted daemon loses nothing), the copy on flash is written rarely ([`super::sampler`]). The flash
//! copy is on the Linux data partition (`/mnt/f50-disk/.f50d/traffic`) when it is there, so it survives a system
//! update, else in `/etc/f50d/traffic`. The shell's count (`/mnt/mu300-disk/.mu300/traffic` or
//! `/etc/mu300/traffic`, the same format) is taken over once when there is none of ours yet.
//!
//! Settings: `conf` beside the ledger, `KEY=VALUE` lines read (never executed), each value checked against its
//! rule and its default used otherwise.

use std::path::{Path, PathBuf};

use f50_core::paths::Paths;
use f50_core::rpc::matches;
use f50_core::{Result, fsx};

/// The settings of the count.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conf {
    /// The day a billing cycle starts, 1-31 (31: the month's last day).
    pub reset_day: u32,
    /// The monthly cap in bytes; 0: none.
    pub cap: u64,
    /// Warn at this percentage of the cap.
    pub warn: u64,
    /// At the cap, turn data off until the next cycle.
    pub cut: bool,
    /// The plan's name, shown with the count.
    pub plan: String,
    /// `trust`: the clock is right from the start (a device that keeps time).
    pub trust_clock: bool,
}

impl Default for Conf {
    fn default() -> Self {
        Conf { reset_day: 1, cap: 0, warn: 90, cut: false, plan: String::new(), trust_clock: false }
    }
}

pub const RESET_DAY_RE: &str = "[1-9]|[12][0-9]|3[01]";
pub const CAP_RE: &str = "0|[1-9][0-9]{0,15}";
pub const WARN_RE: &str = "[1-9]|[1-9][0-9]|100";

/// A plan's name: one line, no control character, at most 96 bytes (UTF-8 is fine).
pub fn plan_ok(s: &str) -> bool {
    s.len() <= 96 && !s.chars().any(char::is_control)
}

impl Conf {
    pub fn parse(text: &str) -> Conf {
        let mut c = Conf::default();
        let last = |k: &str| text.lines().rev().find_map(|l| l.strip_prefix(k)?.strip_prefix('='));
        if let Some(v) = last("RESET_DAY").filter(|v| matches(RESET_DAY_RE, v)) {
            c.reset_day = v.parse().unwrap_or(1);
        }
        if let Some(v) = last("CAP").filter(|v| matches("[0-9]{1,16}", v)) {
            c.cap = v.parse().unwrap_or(0);
        }
        if let Some(v) = last("WARN").filter(|v| matches(WARN_RE, v)) {
            c.warn = v.parse().unwrap_or(90);
        }
        c.cut = last("CUT") == Some("1");
        if let Some(v) = last("PLAN").filter(|v| plan_ok(v)) {
            v.clone_into(&mut c.plan);
        }
        c.trust_clock = last("CLOCK") == Some("trust");
        c
    }

    pub fn render(&self) -> String {
        format!(
            "RESET_DAY={}\nCAP={}\nWARN={}\nCUT={}\nPLAN={}\nCLOCK={}\n",
            self.reset_day,
            self.cap,
            self.warn,
            u8::from(self.cut),
            self.plan,
            if self.trust_clock { "trust" } else { "auto" }
        )
    }
}

/// The files of the count.
#[derive(Debug, Clone)]
pub struct Store {
    /// On flash: `ledger` and `conf`.
    pub dir: PathBuf,
    /// In RAM: the live `ledger`.
    pub run: PathBuf,
    old: Vec<PathBuf>,
}

impl Store {
    pub fn new(paths: &Paths) -> Store {
        let disk = paths.disk();
        let dir = if disk.is_dir() { disk.join(".f50d/traffic") } else { paths.state().join("traffic") };
        let old = vec![paths.sys("/mnt/mu300-disk/.mu300/traffic"), paths.sys("/etc/mu300/traffic")];
        Store { dir, run: paths.run().join("traffic"), old }
    }

    fn read(dir: &Path, name: &str) -> Option<String> {
        fsx::read_opt(&dir.join(name)).ok().flatten().filter(|s| !s.trim().is_empty())
    }

    /// The live ledger of this boot, else the flash copy, else the shell's.
    pub fn load_ledger(&self) -> Option<String> {
        Self::read(&self.run, "ledger")
            .or_else(|| Self::read(&self.dir, "ledger"))
            .or_else(|| self.old.iter().find_map(|d| Self::read(d, "ledger")))
    }

    pub fn load_conf(&self) -> Conf {
        Self::read(&self.dir, "conf")
            .or_else(|| self.old.iter().find_map(|d| Self::read(d, "conf")))
            .map(|t| Conf::parse(&t))
            .unwrap_or_default()
    }

    pub fn save_conf(&self, c: &Conf) -> Result<()> {
        fsx::private_dir(&self.dir)?;
        fsx::write_atomic(&self.dir.join("conf"), c.render().as_bytes(), 0o600)
    }

    /// The live copy (RAM, every round).
    pub fn save_live(&self, ledger: &str) -> Result<()> {
        fsx::private_dir(&self.run)?;
        fsx::write_atomic(&self.run.join("ledger"), ledger.as_bytes(), 0o600)
    }

    /// The flash copy (rarely).
    pub fn save_disk(&self, ledger: &str) -> Result<()> {
        fsx::private_dir(&self.dir)?;
        fsx::write_atomic(&self.dir.join("ledger"), ledger.as_bytes(), 0o600)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_are_checked_and_kept() {
        let c = Conf::parse("RESET_DAY=15\nCAP=53687091200\nWARN=101\nCUT=1\nPLAN=Plan ş\nDEVICE=x\nCAP=junk\n");
        assert_eq!(c.reset_day, 15);
        assert_eq!(c.cap, 0, "the last line of a key counts, and junk is the default");
        assert_eq!(c.warn, 90);
        assert!(c.cut);
        assert_eq!(c.plan, "Plan ş");
        assert_eq!(Conf::parse(&c.render()), c);
        assert!(!plan_ok("a\nb") && !plan_ok(&"x".repeat(97)) && plan_ok(""));
        assert!(matches(CAP_RE, "0") && !matches(CAP_RE, "01") && !matches(CAP_RE, "12345678901234567"));
    }
}
