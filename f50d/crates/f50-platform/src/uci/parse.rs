//! The UCI file format (`/etc/config/<name>`), parsed natively, as libuci's `file.c` reads it.
//!
//! - words are separated by blanks; `'...'` is literal, `"..."` takes `\` escapes, an unquoted `\` escapes the next
//!   character, and quoted and unquoted pieces next to each other make one word (`'it'\''s'`);
//! - an unquoted `#` ends the line, an unquoted `;` ends a statement; a `\` at the end of a line continues it;
//! - `config <type> [<name>]`, `option <name> <value>`, `list <name> <value>`, `package <name>` (ignored);
//! - a named section seen again is the same section (its options merge); `list` after `option` of the same name
//!   turns it into a list; `option` replaces in place;
//! - an anonymous section is named like libuci's `uci_fixup_section`: `cfg` + the count of anonymous sections so far
//!   (two hex digits) + a djb hash of its type and its options' names and string values, modulo 2^16 (four hex
//!   digits). The name matches `uci show -X` for a config without uncommitted changes.
//!
//! A malformed statement is skipped (libuci would refuse the whole file; f50d prefers reading what it can).

use std::collections::BTreeMap;

use super::{Section, Value};

/// Every section of a config file, in file order.
pub fn parse(text: &str) -> Vec<Section> {
    let mut p = Parser::default();
    for stmt in statements(text) {
        p.statement(&stmt);
    }
    p.finish()
}

#[derive(Default)]
struct Parser {
    done: Vec<Building>,
    current: Option<usize>,
    anonymous_count: u32,
}

/// A section being read: options in their file order (the hash of an anonymous name depends on it).
struct Building {
    name: String,
    stype: String,
    anonymous: bool,
    options: Vec<(String, Value)>,
}

impl Parser {
    fn statement(&mut self, words: &[String]) {
        let Some((kw, args)) = words.split_first() else { return };
        match (kw.as_str(), args) {
            ("config", [stype, rest @ ..]) if rest.len() <= 1 => {
                self.close();
                match rest.first().filter(|n| !n.is_empty()) {
                    Some(name) => {
                        if let Some(i) = self.done.iter().position(|b| !b.anonymous && b.name == *name) {
                            self.done[i].stype.clone_from(stype);
                            self.current = Some(i);
                        } else {
                            self.open(name.clone(), stype.clone(), false);
                        }
                    }
                    None => self.open(String::new(), stype.clone(), true),
                }
            }
            ("option", [name, value]) => {
                if let Some(b) = self.current_mut() {
                    match b.options.iter_mut().find(|(n, _)| n == name) {
                        Some(slot) => slot.1 = Value::Str(value.clone()),
                        None => b.options.push((name.clone(), Value::Str(value.clone()))),
                    }
                }
            }
            ("list", [name, value]) => {
                if let Some(b) = self.current_mut() {
                    match b.options.iter_mut().find(|(n, _)| n == name) {
                        Some((_, Value::List(l))) => l.push(value.clone()),
                        Some(slot) => {
                            let old = match &slot.1 {
                                Value::Str(s) => s.clone(),
                                Value::List(_) => String::new(),
                            };
                            slot.1 = Value::List(vec![old, value.clone()]);
                        }
                        None => b.options.push((name.clone(), Value::List(vec![value.clone()]))),
                    }
                }
            }
            _ => {} // package, or a malformed statement
        }
    }

    fn open(&mut self, name: String, stype: String, anonymous: bool) {
        self.done.push(Building { name, stype, anonymous, options: Vec::new() });
        self.current = Some(self.done.len() - 1);
    }

    fn current_mut(&mut self) -> Option<&mut Building> {
        self.current.and_then(|i| self.done.get_mut(i))
    }

    /// The section that ends here gets its generated name when it is anonymous (libuci fixes it up at this point).
    fn close(&mut self) {
        let Some(i) = self.current.take() else { return };
        let b = &mut self.done[i];
        if b.anonymous && b.name.is_empty() {
            self.anonymous_count += 1;
            b.name = anonymous_name(self.anonymous_count, &b.stype, &b.options);
        }
    }

    fn finish(mut self) -> Vec<Section> {
        self.close();
        self.done
            .into_iter()
            .map(|b| Section {
                name: b.name,
                stype: b.stype,
                anonymous: b.anonymous,
                options: b.options.into_iter().collect::<BTreeMap<_, _>>(),
            })
            .collect()
    }
}

/// libuci's `djbhash` step: starts at 5381 (from ~0), keeps 31 bits. Bytes are unsigned, as `char` is on aarch64.
fn djb(hash: u32, s: &str) -> u32 {
    let mut h = if hash == u32::MAX { 5381 } else { hash };
    for b in s.bytes() {
        h = h.wrapping_shl(5).wrapping_add(h).wrapping_add(u32::from(b));
    }
    h & 0x7FFF_FFFF
}

/// `cfg%02x%04x` of libuci's `uci_fixup_section`.
pub(crate) fn anonymous_name(n: u32, stype: &str, options: &[(String, Value)]) -> String {
    let mut h = djb(u32::MAX, stype);
    for (name, value) in options {
        h = djb(h, name);
        if let Value::Str(s) = value {
            h = djb(h, s);
        }
    }
    format!("cfg{:02x}{:04x}", n, h % (1 << 16))
}

/// The statements of a file: each a list of words.
fn statements(text: &str) -> Vec<Vec<String>> {
    let mut out = Vec::new();
    let mut words: Vec<String> = Vec::new();
    let mut word = String::new();
    let mut in_word = false;
    let mut chars = text.chars().peekable();

    let end_word = |words: &mut Vec<String>, word: &mut String, in_word: &mut bool| {
        if *in_word {
            words.push(std::mem::take(word));
            *in_word = false;
        }
    };

    while let Some(c) = chars.next() {
        match c {
            '\n' | ';' => {
                end_word(&mut words, &mut word, &mut in_word);
                if !words.is_empty() {
                    out.push(std::mem::take(&mut words));
                }
            }
            ' ' | '\t' | '\r' => end_word(&mut words, &mut word, &mut in_word),
            '#' => {
                // the rest of the line is a comment
                for c in chars.by_ref() {
                    if c == '\n' {
                        break;
                    }
                }
                end_word(&mut words, &mut word, &mut in_word);
                if !words.is_empty() {
                    out.push(std::mem::take(&mut words));
                }
            }
            '\'' => {
                in_word = true;
                for c in chars.by_ref() {
                    if c == '\'' {
                        break;
                    }
                    word.push(c);
                }
            }
            '"' => {
                in_word = true;
                while let Some(c) = chars.next() {
                    match c {
                        '"' => break,
                        '\\' => match chars.next() {
                            Some('\n') | None => {}
                            Some(e) => word.push(e),
                        },
                        _ => word.push(c),
                    }
                }
            }
            '\\' => match chars.next() {
                Some('\n') => {} // continued on the next line
                Some(e) => {
                    in_word = true;
                    word.push(e);
                }
                None => {}
            },
            _ => {
                in_word = true;
                word.push(c);
            }
        }
    }
    end_word(&mut words, &mut word, &mut in_word);
    if !words.is_empty() {
        out.push(words);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const NETWORK: &str = r#"
config interface 'loopback'
	option device 'lo'
	option proto 'static'
	option ipaddr '127.0.0.1'

config globals 'globals'
	option ula_prefix 'fd3c:1234::/48'

config device
	option name 'br-lan'
	option type 'bridge'
	list ports 'usb0'
	list ports "wlan0"   # a comment
	list ports rndis\ 1

config interface 'lan'
	option device 'br-lan'
	option ipaddr '192.168.0.1'
	option note 'it'\''s ok; really'
"#;

    #[test]
    fn sections_options_and_lists() {
        let s = parse(NETWORK);
        assert_eq!(s.len(), 4);
        assert_eq!(s[0].name, "loopback");
        assert_eq!(s[0].stype, "interface");
        assert!(!s[0].anonymous);
        assert_eq!(s[0].get("ipaddr"), Some("127.0.0.1"));
        assert!(s[2].anonymous);
        assert_eq!(s[2].stype, "device");
        assert!(s[2].name.starts_with("cfg01"), "{}", s[2].name);
        assert_eq!(s[2].list("ports"), vec!["usb0", "wlan0", "rndis 1"]);
        assert_eq!(s[3].get("note"), Some("it's ok; really"));
    }

    #[test]
    fn anonymous_names_follow_libuci() {
        // djb("ab") from 5381: (5381*33 + 97)*33 + 98
        assert_eq!(djb(u32::MAX, "ab"), (5381 * 33 + 97) * 33 + 98);
        let s = parse("config dnsmasq\nconfig dnsmasq\n\toption port '53'\n");
        assert!(s[0].name.starts_with("cfg01"));
        assert!(s[1].name.starts_with("cfg02"));
        assert_ne!(s[0].name[5..], s[1].name[5..]);
    }

    #[test]
    fn named_sections_merge_and_lists_convert() {
        let s = parse("config a 'x'\n option o '1'\nconfig b\nconfig a 'x'\n list o '2'\n option p 3\n");
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].list("o"), vec!["1", "2"]);
        assert_eq!(s[0].get("p"), Some("3"));
    }

    #[test]
    fn statements_and_continuations() {
        let st = statements("config a; option b c\noption d 'e f'\\\n g\n# only a comment\n");
        assert_eq!(st, vec![vec!["config", "a"], vec!["option", "b", "c"], vec!["option", "d", "e f", "g"]]);
        assert!(parse("option orphan 1\nconfig\n").is_empty());
    }
}
