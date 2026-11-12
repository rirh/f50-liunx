//! The text `uci batch` reads for a list of [`Change`]s, and section references.
//!
//! Every value is single-quoted (an embedded `'` becomes `'\''`, which `uci`'s tokenizer joins back into one word,
//! as the shell does); names are checked before they are written. A value that spans lines cannot be expressed in
//! a batch line and is refused.

use super::{Change, Section};
use crate::check::{is_dashed_word, is_one_line, is_word, refused, require};
use f50_core::Result;

/// A value as one `uci batch` word.
pub fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}

/// A section reference `uci` accepts: a name (`wan`, `cfg023579`) or `@type[n]` (`@wifi-iface[0]`,
/// `@rule[-1]`).
pub fn is_section_ref(s: &str) -> bool {
    is_word(s) || parse_type_ref(s).is_some()
}

/// `@type[n]` split into the type and the index.
pub(crate) fn parse_type_ref(s: &str) -> Option<(&str, i64)> {
    let inner = s.strip_prefix('@')?.strip_suffix(']')?;
    let (stype, n) = inner.split_once('[')?;
    let digits = n.strip_prefix('-').unwrap_or(n);
    if !is_dashed_word(stype) || digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some((stype, n.parse().ok()?))
}

/// The section `reference` names in `sections`: by name, or by `@type[n]` (negative `n` counts from the end), as
/// `uci` resolves it.
pub fn resolve<'a>(sections: &'a [Section], reference: &str) -> Option<&'a Section> {
    match parse_type_ref(reference) {
        Some((stype, n)) => {
            let of_type: Vec<&Section> = sections.iter().filter(|s| s.stype == stype).collect();
            let i = if n < 0 {
                of_type.len().checked_sub(n.unsigned_abs().try_into().ok()?)?
            } else {
                n.try_into().ok()?
            };
            of_type.get(i).copied()
        }
        None => sections.iter().find(|s| s.name == reference),
    }
}

/// `@type[n]` for a section of `sections` (its place among the sections of its type).
pub fn type_ref(sections: &[Section], section: &Section) -> Option<String> {
    let i = sections.iter().filter(|s| s.stype == section.stype).position(|s| s.name == section.name)?;
    Some(format!("@{}[{i}]", section.stype))
}

fn section_of(c: &Change) -> &str {
    match c {
        Change::Section { section, .. }
        | Change::Set { section, .. }
        | Change::Delete { section, .. }
        | Change::AddList { section, .. }
        | Change::DelList { section, .. } => section,
        Change::Add { .. } => "",
    }
}

/// The `uci batch` text for `changes` to `config` (without `commit`).
///
/// `current` is the config as committed: a `Delete` of a section or option that is not there, and that no earlier
/// change of the batch touches, is left out (in a batch `uci` stops at it, and "make sure it is gone" is what
/// callers mean). After an `Add`, the new section is `@type[-1]`.
pub fn build(config: &str, changes: &[Change], current: &[Section]) -> Result<String> {
    require(is_dashed_word(config), "uci config")?;
    let mut out = String::new();
    for (i, c) in changes.iter().enumerate() {
        let section = section_of(c);
        if !matches!(c, Change::Add { .. }) {
            require(is_section_ref(section), "uci section")?;
        }
        match c {
            Change::Section { stype, .. } => {
                require(is_dashed_word(stype), "uci section type")?;
                out.push_str(&format!("set {config}.{section}={stype}\n"));
            }
            Change::Add { stype } => {
                require(is_dashed_word(stype), "uci section type")?;
                out.push_str(&format!("add {config} {stype}\n"));
            }
            Change::Set { option, value, .. } => {
                line(&mut out, "set", config, section, option, value)?;
            }
            Change::AddList { option, value, .. } => line(&mut out, "add_list", config, section, option, value)?,
            Change::DelList { option, value, .. } => line(&mut out, "del_list", config, section, option, value)?,
            Change::Delete { option, .. } => {
                if let Some(o) = option {
                    require(is_word(o), "uci option")?;
                }
                let touched_before =
                    changes[..i].iter().any(|p| section_of(p) == section || matches!(p, Change::Add { .. }));
                if !touched_before {
                    let present = resolve(current, section)
                        .is_some_and(|s| option.as_ref().is_none_or(|o| s.options.contains_key(o)));
                    if !present {
                        continue;
                    }
                }
                match option {
                    Some(o) => out.push_str(&format!("delete {config}.{section}.{o}\n")),
                    None => out.push_str(&format!("delete {config}.{section}\n")),
                }
            }
        }
    }
    Ok(out)
}

fn line(out: &mut String, verb: &str, config: &str, section: &str, option: &str, value: &str) -> Result<()> {
    require(is_word(option), "uci option")?;
    if !is_one_line(value) {
        return Err(refused(&format!("{config}.{section}.{option}")));
    }
    out.push_str(&format!("{verb} {config}.{section}.{option}={}\n", quote(value)));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::parse::parse;
    use super::*;
    use f50_core::Code;

    fn set(section: &str, option: &str, value: &str) -> Change {
        Change::Set { section: section.into(), option: option.into(), value: value.into() }
    }

    #[test]
    fn quoting() {
        assert_eq!(quote("plain"), "'plain'");
        assert_eq!(quote("it's"), r"'it'\''s'");
        assert_eq!(quote(""), "''");
        assert_eq!(quote("$(reboot) `x` \"y\""), "'$(reboot) `x` \"y\"'");
    }

    #[test]
    fn batch_text() {
        let current = parse("config wifi-iface\n option ssid 'a'\nconfig interface 'wan'\n option proto 'dhcp'\n");
        let changes = vec![
            Change::Section { section: "f50cell".into(), stype: "interface".into() },
            set("f50cell", "proto", "f50cell"),
            set("@wifi-iface[0]", "key", "p'w d"),
            Change::AddList { section: "wan".into(), option: "dns".into(), value: "1.1.1.1".into() },
            Change::DelList { section: "wan".into(), option: "dns".into(), value: "8.8.8.8".into() },
            Change::Delete { section: "wan".into(), option: Some("peerdns".into()) },
            Change::Delete { section: "wan".into(), option: Some("proto".into()) },
            Change::Delete { section: "gone".into(), option: None },
            Change::Add { stype: "rule".into() },
            set("@rule[-1]", "name", "x"),
        ];
        let text = build("network", &changes, &current).expect("valid");
        assert_eq!(
            text,
            "set network.f50cell=interface\n\
             set network.f50cell.proto='f50cell'\n\
             set network.@wifi-iface[0].key='p'\\''w d'\n\
             add_list network.wan.dns='1.1.1.1'\n\
             del_list network.wan.dns='8.8.8.8'\n\
             delete network.wan.peerdns\n\
             delete network.wan.proto\n\
             add network rule\n\
             set network.@rule[-1].name='x'\n"
        );
    }

    #[test]
    fn absent_deletes_are_skipped() {
        let current = parse("config interface 'wan'\n option proto 'dhcp'\n");
        let text = build("network", &[Change::Delete { section: "wan".into(), option: Some("mtu".into()) }], &current);
        assert_eq!(text.expect("valid"), "");
    }

    #[test]
    fn names_and_values_are_checked() {
        let bad = |c: Change| build("network", &[c], &[]).expect_err("refused").code();
        assert_eq!(bad(set("wan;reboot", "proto", "x")), Code::BadValue);
        assert_eq!(bad(set("wan", "pro to", "x")), Code::BadValue);
        assert_eq!(bad(set("wan", "proto", "a\nb")), Code::BadValue);
        assert_eq!(bad(set("@x[1", "proto", "x")), Code::BadValue);
        assert_eq!(bad(Change::Add { stype: "a b".into() }), Code::BadValue);
        assert!(build("net work", &[], &[]).is_err());
    }

    #[test]
    fn references_resolve() {
        let s = parse("config a 'x'\nconfig b\n option k '1'\nconfig b\n option k '2'\n");
        assert_eq!(resolve(&s, "x").map(|s| s.stype.as_str()), Some("a"));
        assert_eq!(resolve(&s, "@b[0]").and_then(|s| s.get("k")), Some("1"));
        assert_eq!(resolve(&s, "@b[-1]").and_then(|s| s.get("k")), Some("2"));
        assert!(resolve(&s, "@b[2]").is_none());
        assert!(resolve(&s, "@b[-3]").is_none());
        assert_eq!(type_ref(&s, &s[2]).as_deref(), Some("@b[1]"));
        let anon = &s[1].name;
        assert_eq!(resolve(&s, anon).and_then(|s| s.get("k")), Some("1"));
    }
}
