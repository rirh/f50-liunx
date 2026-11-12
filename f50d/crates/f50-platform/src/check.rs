//! Name checks shared by the modules: what may reach a command line, a path or a `uci batch` line.
//!
//! A refused name is `bad-value`; the message names what was refused, never the value (it may be a secret's
//! neighbour, and it is caller input).

use f50_core::{Code, Error};

/// `[A-Za-z0-9_]+`: UCI option and section names, sysctl path components without the dot.
pub(crate) fn is_word(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

/// `[A-Za-z0-9_-]+`: UCI config and section type names, kernel module names.
pub(crate) fn is_dashed_word(s: &str) -> bool {
    !s.is_empty() && !s.starts_with('-') && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// A procd service or netifd interface/device name: `[A-Za-z0-9_.@-]{1,64}`, not starting with `-` or `.`.
pub(crate) fn is_object_name(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && !s.starts_with(['-', '.'])
        && !s.contains("..")
        && s.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.' | b'@'))
}

/// A value that fits on one line of a line-oriented protocol (`uci batch`, `passwd`'s stdin).
pub(crate) fn is_one_line(s: &str) -> bool {
    !s.bytes().any(|b| matches!(b, b'\n' | b'\r' | 0))
}

/// The refusal for a name or value that did not pass its check.
pub(crate) fn refused(what: &str) -> Error {
    Error::new(Code::BadValue, format!("{what}: not an accepted value"))
}

/// `Ok` when `ok`, else [`refused`].
pub(crate) fn require(ok: bool, what: &str) -> f50_core::Result<()> {
    if ok { Ok(()) } else { Err(refused(what)) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert!(is_word("lan_6"));
        assert!(!is_word("lan-6"));
        assert!(!is_word(""));
        assert!(is_dashed_word("wifi-iface"));
        assert!(!is_dashed_word("-x"));
        assert!(is_object_name("network.interface"));
        assert!(is_object_name("br-lan"));
        assert!(!is_object_name("../etc"));
        assert!(!is_object_name("-help"));
        assert!(!is_object_name("a/b"));
        assert!(is_one_line("it's fine"));
        assert!(!is_one_line("two\nlines"));
    }
}
