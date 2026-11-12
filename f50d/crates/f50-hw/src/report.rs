//! What a boot stage did, step by step, and where the last report of each stage is kept.
//!
//! The stages run as separate `f50d boot <stage>` processes started by procd, so the daemon's `f50.boot get` learns
//! their results from files: each report is written to `<run>/boot/<stage>.json` (private, atomic).

use std::path::{Path, PathBuf};
use std::time::Instant;

use f50_core::fsx;
use f50_core::time::{age_ms, unix_now};
use serde::{Deserialize, Serialize};

use crate::Stage;

/// How one step ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Outcome {
    Done,
    /// Not needed here (the kernel has no such module, the node is already right, ...).
    Skipped,
    Failed,
}

/// One step of a stage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Step {
    pub name: String,
    pub outcome: Outcome,
    pub detail: String,
}

/// A stage's result. `ok` is false when a step the stage exists for failed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageReport {
    pub stage: Stage,
    /// Unix time the stage started (0 before the clock is set).
    pub started: u64,
    /// The kernel's uptime when it started, in seconds.
    pub uptime: Option<f64>,
    pub duration_ms: u64,
    pub ok: bool,
    pub steps: Vec<Step>,
}

/// Collects the steps of a running stage.
#[derive(Debug)]
pub(crate) struct Steps {
    stage: Stage,
    started: u64,
    uptime: Option<f64>,
    t0: Instant,
    ok: bool,
    steps: Vec<Step>,
}

impl Steps {
    pub(crate) fn new(stage: Stage, uptime: Option<f64>) -> Self {
        Steps { stage, started: unix_now(), uptime, t0: Instant::now(), ok: true, steps: Vec::new() }
    }

    pub(crate) fn push(&mut self, name: &str, outcome: Outcome, detail: impl Into<String>) {
        let detail = f50_core::sanitize::sanitize_message(&detail.into());
        match outcome {
            Outcome::Failed => tracing::warn!(stage = ?self.stage, step = name, "{detail}"),
            _ => tracing::info!(stage = ?self.stage, step = name, ?outcome, "{detail}"),
        }
        self.steps.push(Step { name: name.to_owned(), outcome, detail });
    }

    pub(crate) fn done(&mut self, name: &str, detail: impl Into<String>) {
        self.push(name, Outcome::Done, detail);
    }

    pub(crate) fn skipped(&mut self, name: &str, detail: impl Into<String>) {
        self.push(name, Outcome::Skipped, detail);
    }

    /// A failure that the stage goes on after (the shell's "logged, not fatal").
    pub(crate) fn failed(&mut self, name: &str, detail: impl Into<String>) {
        self.push(name, Outcome::Failed, detail);
    }

    /// A failure of what the stage exists for: the report says `ok: false`.
    pub(crate) fn fatal(&mut self, name: &str, detail: impl Into<String>) {
        self.ok = false;
        self.push(name, Outcome::Failed, detail);
    }

    /// `Ok(detail)` done, `Err` failed (not fatal).
    pub(crate) fn result(&mut self, name: &str, r: f50_core::Result<String>) {
        match r {
            Ok(d) => self.done(name, d),
            Err(e) => self.failed(name, e.message()),
        }
    }

    pub(crate) fn finish(self) -> StageReport {
        StageReport {
            stage: self.stage,
            started: self.started,
            uptime: self.uptime,
            duration_ms: age_ms(self.t0),
            ok: self.ok,
            steps: self.steps,
        }
    }
}

fn report_path(dir: &Path, stage: Stage) -> PathBuf {
    dir.join(format!("{}.json", stage.as_str()))
}

/// Keeps the report for `f50.boot get`; a report that cannot be kept is only logged (the boot goes on).
pub(crate) fn save(dir: &Path, report: &StageReport) {
    let r = fsx::private_dir(dir).and_then(|()| {
        let data = serde_json::to_vec(report).map_err(|e| f50_core::Error::failed(e.to_string()))?;
        fsx::write_atomic(&report_path(dir, report.stage), &data, 0o600)
    });
    if let Err(e) = r {
        tracing::warn!("boot stage report not kept: {e}");
    }
}

/// The last report of every stage that left one.
pub(crate) fn load_all(dir: &Path) -> Vec<StageReport> {
    Stage::ALL
        .iter()
        .filter_map(|s| fsx::read_opt(&report_path(dir, *s)).ok().flatten())
        .filter_map(|text| serde_json::from_str(&text).ok())
        .collect()
}
