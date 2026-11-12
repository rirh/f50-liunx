//! UCI configuration: read a whole config, apply changes as one batch and commit.

use std::collections::BTreeMap;

use f50_core::Result;

use crate::Platform;

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
}

/// One change of a batch. Section and option names are checked (`[A-Za-z0-9_]+`); values may be anything,
/// including secrets: they reach `uci batch` on its stdin, quoted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    /// `uci set config.section=type`
    Section { section: String, stype: String },
    /// `uci set config.section.option=value`
    Set { section: String, option: String, value: String },
    /// `uci delete config.section.option` (or the section when `option` is `None`)
    Delete { section: String, option: Option<String> },
    /// `uci add_list config.section.option=value`
    AddList { section: String, option: String, value: String },
    /// `uci del_list config.section.option=value`
    DelList { section: String, option: String, value: String },
}

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
        let _ = (&self.platform, config);
        todo!("f50-platform: Uci::load")
    }

    /// One option, or `None`.
    pub async fn get(&self, config: &str, section: &str, option: &str) -> Result<Option<String>> {
        let _ = (config, section, option);
        todo!("f50-platform: Uci::get")
    }

    /// Applies `changes` to `config` as one batch and commits it; nothing is committed when one change fails.
    pub async fn apply(&self, config: &str, changes: &[Change]) -> Result<()> {
        let _ = (config, changes);
        todo!("f50-platform: Uci::apply")
    }
}
