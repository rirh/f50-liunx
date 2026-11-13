//! A release's checksums and the decision whether to use them (the shell's `sums_trusted` / `sums_verdict`).
//!
//! `SHA256SUMS` is fetched fresh in every run, never trusted from an earlier one (a release published again under
//! the same tag left a stale copy). It is judged once and pinned by its own hash: every later look is at those very
//! bytes or the run stops. It must list `f50-release` with the hash of the tag that was asked for, so a signed
//! `SHA256SUMS` of one release cannot be handed out as another's; signed checksums that name no release are not
//! taken as signed. The rules (FINDINGS 86):
//!
//! | signature | GitHub or a mirror | a file brought to the device |
//! |---|---|---|
//! | valid | used | used |
//! | bad, or a key that is not root's own | refused | refused |
//! | the system has no key | used, as before signing | only with `unsigned` |
//! | none, or nothing can check it | only with `unsigned` | only with `unsigned` |

use std::collections::HashMap;

use f50_core::{Error, Result};

use super::assets;
use super::sig::SigState;
use crate::util::{is_sha256, sha256_hex};

/// A release's checksums as they were judged.
#[derive(Debug, Clone)]
pub struct Sums {
    /// The hash of the file that was judged.
    pub pin: String,
    files: HashMap<String, String>,
}

impl Sums {
    /// "HASH  NAME" or "HASH *NAME" lines; anything else is passed over. `None` when no line is one.
    pub fn parse(text: &[u8]) -> Option<Sums> {
        let s = std::str::from_utf8(text).ok()?;
        let mut files = HashMap::new();
        for line in s.lines() {
            let mut it = line.splitn(2, char::is_whitespace);
            let (Some(h), Some(n)) = (it.next(), it.next()) else { continue };
            let n = n.trim_start().trim_start_matches('*').trim_end();
            if is_sha256(h) && assets::name_ok(n) {
                files.entry(n.to_owned()).or_insert_with(|| h.to_owned());
            }
        }
        (!files.is_empty()).then(|| Sums { pin: sha256_hex(text), files })
    }

    /// The hash the release gives `name`.
    pub fn want(&self, name: &str) -> Option<&str> {
        self.files.get(name).map(String::as_str)
    }

    /// The release these checksums say they are of matches `tag` (`Some(false)`: another one; `None`: they say none).
    pub fn names_tag(&self, tag: &str) -> Option<bool> {
        self.want(assets::RELEASE_TAG).map(|h| h == tag_sha(tag))
    }
}

/// The hash of the `f50-release` file of `tag`: the tag and a newline.
pub fn tag_sha(tag: &str) -> String {
    sha256_hex(format!("{tag}\n").as_bytes())
}

/// Where the checksums came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// GitHub or a mirror, over the network.
    Online,
    /// A file brought to the device (an upload): no origin at all.
    Offline,
}

/// May `sums`, whose signature checked as `state`, be used as release `tag`'s? `Ok` with a line for the log.
pub fn verdict(tag: &str, state: SigState, sums: &Sums, origin: Origin, allow_unsigned: bool) -> Result<String> {
    match state {
        SigState::Bad => {
            return Err(Error::failed("the signature of the release does not match the project's release key"));
        }
        SigState::BadKey => {
            return Err(Error::failed(
                "the release key of this system is not a file of root's own: nothing is updated",
            ));
        }
        _ => {}
    }
    let named = sums.names_tag(tag);
    if named == Some(false) {
        return Err(Error::failed(format!("these are the checksums of another release than {tag}")));
    }
    match state {
        SigState::Valid if named == Some(true) => {
            return Ok(format!("release {tag}: signed with the project's release key"));
        }
        SigState::Valid => {
            return Err(Error::failed("the release is signed, but its checksums do not say which release they are"));
        }
        SigState::NoKey if origin == Origin::Online => {
            return Ok(format!("release {tag}: this system has no release key; trusted as it came"));
        }
        _ => {}
    }
    if allow_unsigned {
        return Ok(format!(
            "release {tag} is not signed, or cannot be checked here ({state:?}): used because you said so"
        ));
    }
    Err(Error::forbidden(format!(
        "release {tag} is not signed with the project's release key ({}): not used",
        format!("{state:?}").to_lowercase()
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sums(tag: &str) -> Sums {
        let text = format!(
            "{}  f50-openwrt-rootfs.tar.gz\n{} *{}\n{}  ../evil\nnot a line\n",
            sha256_hex(b"a"),
            tag_sha(tag),
            assets::RELEASE_TAG,
            sha256_hex(b"b")
        );
        Sums::parse(text.as_bytes()).expect("sums")
    }

    #[test]
    fn parse_and_bind_the_tag() {
        let s = sums("v2026.11.01.1");
        assert_eq!(s.want("f50-openwrt-rootfs.tar.gz"), Some(sha256_hex(b"a").as_str()));
        assert!(s.want("../evil").is_none());
        assert_eq!(s.names_tag("v2026.11.01.1"), Some(true));
        assert_eq!(s.names_tag("v2026.12.01.1"), Some(false));
        assert!(Sums::parse(b"nothing here").is_none());
    }

    #[test]
    fn the_rules() {
        let t = "v2026.11.01.1";
        let s = sums(t);
        let ok = |st, o, u| verdict(t, st, &s, o, u).is_ok();
        assert!(ok(SigState::Valid, Origin::Offline, false));
        assert!(!ok(SigState::Bad, Origin::Online, true), "never, whatever is said");
        assert!(!ok(SigState::BadKey, Origin::Online, true));
        assert!(ok(SigState::NoKey, Origin::Online, false), "as before signing");
        assert!(!ok(SigState::NoKey, Origin::Offline, false));
        assert!(ok(SigState::NoKey, Origin::Offline, true));
        assert!(!ok(SigState::Unsigned, Origin::Online, false), "a system with a key takes no unsigned release");
        assert!(ok(SigState::Unsigned, Origin::Online, true));
        // signed checksums of another release, or of none
        assert!(verdict("v2026.12.01.1", SigState::Valid, &s, Origin::Online, true).is_err());
        let none = Sums::parse(format!("{}  x.tar.gz\n", sha256_hex(b"x")).as_bytes()).expect("sums");
        assert!(verdict(t, SigState::Valid, &none, Origin::Online, false).is_err());
    }
}
