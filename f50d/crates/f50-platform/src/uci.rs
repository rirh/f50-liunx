//! UCI configuration: read a whole config, apply changes as one batch and commit.
//!
//! Reading parses `/etc/config/<name>` natively ([`parse`]): no process, and the committed state is what f50d acts
//! on (uncommitted changes another tool left in `/tmp/.uci` are not seen). Writing goes through `uci batch` (the
//! program *is* the platform: it keeps the file format, the delta files and the locking right): one run with every
//! change, read from stdin, then `uci commit <config>`. Any failure reverts the config's pending changes, so nothing
//! half-applied stays behind.
//!
//! Section references are names (`wan`), generated names of anonymous sections (`cfg023579`, named as `uci` names
//! them) or `@type[n]` (`@wifi-iface[0]`, `@rule[-1]`). Prefer `@type[n]` for anonymous sections in changes: it does
//! not depend on the section's contents.

mod batch;
mod parse;

use std::collections::BTreeMap;
use std::time::Duration;

use f50_core::exec::Cmd;
use f50_core::{Error, Result};
use tokio::sync::Mutex;

use crate::Platform;
use crate::check::{is_dashed_word, require};

pub use batch::{is_section_ref, quote, resolve, type_ref};
pub use parse::parse;

/// An option's value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Str(String),
    List(Vec<String>),
}

/// One section of a config.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    /// The section's name (`wan`), or the generated name of an anonymous one (`cfg023579`).
    pub name: String,
    /// Its type (`interface`, `wifi-iface`).
    pub stype: String,
    pub anonymous: bool,
    pub options: BTreeMap<String, Value>,
}

impl Section {
    /// A plain option's value (the first item of a list).
    pub fn get(&self, option: &str) -> Option<&str> {
        match self.options.get(option)? {
            Value::Str(s) => Some(s),
            Value::List(l) => l.first().map(String::as_str),
        }
    }

    pub fn list(&self, option: &str) -> Vec<String> {
        match self.options.get(option) {
            Some(Value::Str(s)) => vec![s.clone()],
            Some(Value::List(l)) => l.clone(),
            None => Vec::new(),
        }
    }

    /// A boolean option as UCI tools read one (`1`/`on`/`true`/`yes`/`enabled` and their opposites); `None` when
    /// absent or not a boolean.
    pub fn flag(&self, option: &str) -> Option<bool> {
        match self.get(option)? {
            "1" | "on" | "true" | "yes" | "enabled" => Some(true),
            "0" | "off" | "false" | "no" | "disabled" => Some(false),
            _ => None,
        }
    }
}

/// One change of a batch. Section and option names are checked (`[A-Za-z0-9_]+`); values may be anything,
/// including secrets: they reach `uci batch` on its stdin, quoted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    /// `uci set config.section=type`
    Section { section: String, stype: String },
    /// `uci set config.section.option=value`
    Set { section: String, option: String, value: String },
    /// `uci delete config.section.option` (or the section when `option` is `None`). Deleting what is not there is
    /// not an error.
    Delete { section: String, option: Option<String> },
    /// `uci add_list config.section.option=value`
    AddList { section: String, option: String, value: String },
    /// `uci del_list config.section.option=value`
    DelList { section: String, option: String, value: String },
    /// `uci add config type`: a new anonymous section, `@type[-1]` for the changes after it.
    Add { stype: String },
}

/// How long one `uci` run may take.
const UCI_TIMEOUT: Duration = Duration::from_secs(30);

/// One apply at a time in this process: `uci revert` on a failure must not take another apply's changes with it.
static APPLY: Mutex<()> = Mutex::const_new(());

#[derive(Debug, Clone)]
pub struct Uci {
    platform: Platform,
}

impl Uci {
    pub(crate) fn new(platform: Platform) -> Self {
        Uci { platform }
    }

    /// Every section of `config` (`/etc/config/<config>`), in file order. A missing config is empty.
    pub async fn load(&self, config: &str) -> Result<Vec<Section>> {
        require(is_dashed_word(config), "uci config")?;
        let path = self.platform.paths().sys(format!("/etc/config/{config}"));
        match tokio::fs::read(&path).await {
            Ok(bytes) => Ok(parse(&String::from_utf8_lossy(&bytes))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(e) => Err(Error::io(format!("read {}", path.display()), &e)),
        }
    }

    /// One option, or `None`. A list is its items joined by spaces, as `uci get` prints it.
    pub async fn get(&self, config: &str, section: &str, option: &str) -> Result<Option<String>> {
        let sections = self.load(config).await?;
        Ok(resolve(&sections, section).and_then(|s| match s.options.get(option)? {
            Value::Str(v) => Some(v.clone()),
            Value::List(l) => Some(l.join(" ")),
        }))
    }

    /// One section by reference (name, generated name or `@type[n]`), or `None`.
    pub async fn section(&self, config: &str, reference: &str) -> Result<Option<Section>> {
        Ok(resolve(&self.load(config).await?, reference).cloned())
    }

    /// The `n`th section of type `stype` (negative counts from the end), as `@stype[n]` names it.
    pub async fn section_of_type(&self, config: &str, stype: &str, n: i64) -> Result<Option<Section>> {
        self.section(config, &format!("@{stype}[{n}]")).await
    }

    /// Every section of type `stype`, in file order.
    pub async fn sections_of_type(&self, config: &str, stype: &str) -> Result<Vec<Section>> {
        Ok(self.load(config).await?.into_iter().filter(|s| s.stype == stype).collect())
    }

    /// Applies `changes` to `config` as one batch and commits it; nothing is committed when one change fails.
    ///
    /// Pending changes of `config` that someone else left uncommitted are reverted first, so the commit carries
    /// these changes only.
    pub async fn apply(&self, config: &str, changes: &[Change]) -> Result<()> {
        require(is_dashed_word(config), "uci config")?;
        if changes.is_empty() {
            return Ok(());
        }
        let _one = APPLY.lock().await;
        let current = self.load(config).await?;
        let text = batch::build(config, changes, &current)?;
        if text.is_empty() {
            return Ok(()); // only deletes of what is not there
        }
        self.revert(config).await;
        let result = self.batch_and_commit(config, text).await;
        if result.is_err() {
            self.revert(config).await;
        }
        result
    }

    async fn batch_and_commit(&self, config: &str, text: String) -> Result<()> {
        let out = self.cmd().arg("batch").stdin(text).output().await?;
        // `uci batch` stops at the first failing line; its message goes to stderr whatever the status says
        if !out.success() || !out.stderr.trim().is_empty() {
            return Err(out.error("uci").context(format!("uci {config}")));
        }
        self.cmd().args(["commit", config]).ok().await.map(drop).map_err(|e| e.context(format!("uci commit {config}")))
    }

    async fn revert(&self, config: &str) {
        if let Err(e) = self.cmd().args(["revert", config]).ok().await {
            tracing::warn!(config, error = %e, "uci revert failed");
        }
    }

    /// `uci` with the root's config and delta directories when the root is not `/` (tests).
    fn cmd(&self) -> Cmd {
        let paths = self.platform.paths();
        let cmd = Cmd::new("uci").timeout(UCI_TIMEOUT);
        if paths.root() == std::path::Path::new("/") {
            cmd
        } else {
            cmd.arg("-c").arg(paths.sys("/etc/config")).arg("-t").arg(paths.sys("/tmp/.uci"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use f50_core::paths::Paths;

    fn platform(tag: &str, wireless: &str) -> (Platform, std::path::PathBuf) {
        let root = std::env::temp_dir().join(format!("f50-platform-uci-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(root.join("etc/config")).expect("mkdir");
        std::fs::write(root.join("etc/config/wireless"), wireless).expect("write");
        (Platform::new(Paths::new(&root), None), root)
    }

    #[tokio::test]
    async fn load_and_get() {
        let (p, root) = platform(
            "get",
            "config wifi-device 'radio0'\n option country 'TR'\nconfig wifi-iface\n option ssid 'F50'\n list maclist 'a'\n list maclist 'b'\n",
        );
        let uci = p.uci();
        assert_eq!(uci.load("wireless").await.expect("load").len(), 2);
        assert_eq!(uci.get("wireless", "radio0", "country").await.expect("get").as_deref(), Some("TR"));
        assert_eq!(uci.get("wireless", "@wifi-iface[0]", "ssid").await.expect("get").as_deref(), Some("F50"));
        assert_eq!(uci.get("wireless", "@wifi-iface[0]", "maclist").await.expect("get").as_deref(), Some("a b"));
        assert_eq!(uci.get("wireless", "radio1", "country").await.expect("get"), None);
        assert!(uci.load("nonexistent").await.expect("missing is empty").is_empty());
        assert!(uci.load("../passwd").await.is_err());
        let s = uci.section_of_type("wireless", "wifi-iface", -1).await.expect("section").expect("there");
        assert!(s.anonymous);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn flags() {
        let s = parse("config x 'y'\n option a 'on'\n option b '0'\n option c 'maybe'\n");
        assert_eq!(s[0].flag("a"), Some(true));
        assert_eq!(s[0].flag("b"), Some(false));
        assert_eq!(s[0].flag("c"), None);
    }
}
