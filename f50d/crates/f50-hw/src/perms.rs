//! Android's device node ownership on OpenWrt: the vendor daemons open `/dev/modem`, the SIPC channels and the
//! modem's partitions as their own account, not as root (FINDINGS 5, requirement 2; FINDINGS 41).
//!
//! [`table`] is the rule set (generated from Android's files), [`apply`] applies it with the ids of the moment.

mod apply;
mod table;

pub(crate) use apply::apply_all;

/// A node's owner as the table names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Owner {
    Root,
    /// The vendor account (`$V`).
    Vendor,
}

/// A node's group as the table names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Group {
    Root,
    /// The vendor account's group (`$VG`).
    Vendor,
    /// The `video` group, else root (`$GPU`).
    Gpu,
}

/// One rule: every existing path the glob names gets the owner, then (only if that worked) the mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Entry {
    /// An absolute path; at most one `*`, in its last component.
    pub glob: &'static str,
    pub owner: Option<(Owner, Group)>,
    pub mode: Option<u32>,
}

pub(crate) const fn own(glob: &'static str, o: Owner, g: Group, mode: u32) -> Entry {
    Entry { glob, owner: Some((o, g)), mode: Some(mode) }
}

pub(crate) const fn chown(glob: &'static str, o: Owner, g: Group) -> Entry {
    Entry { glob, owner: Some((o, g)), mode: None }
}

pub(crate) const fn chmod(glob: &'static str, mode: u32) -> Entry {
    Entry { glob, owner: None, mode: Some(mode) }
}

/// The whole table, in the script's order.
pub(crate) fn entries() -> impl Iterator<Item = &'static Entry> {
    table::UEVENTD.iter().chain(table::INIT_RC).chain(table::SYSTEM_RC)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_is_the_scripts() {
        // ueventd-perms.sh: 590 "for n in" lines, the last naming two paths
        assert_eq!(entries().count(), 591);
        assert_eq!(table::UEVENTD.len(), 115);
        for e in entries() {
            assert!(e.glob.starts_with('/'), "{}", e.glob);
            assert!(e.owner.is_some() || e.mode.is_some(), "{}", e.glob);
            let (dir, name) = e.glob.rsplit_once('/').unwrap_or_default();
            assert!(!dir.contains('*') && name.matches('*').count() <= 1, "{}", e.glob);
            if let Some(m) = e.mode {
                assert_eq!(m & 0o002, 0, "nothing writable by everyone: {}", e.glob);
                if e.glob.starts_with("/dev/") {
                    assert_eq!(m & 0o007, 0, "no bits for others on a node: {}", e.glob);
                }
            }
        }
    }

    #[test]
    fn the_lessons_are_kept() {
        let find = |g: &str| entries().filter(|e| e.glob == g).copied().collect::<Vec<_>>();
        // the modem's calibration NV: the vendor's, never a login user's (FINDINGS 41)
        assert_eq!(
            find("/dev/block/by-name/nr_*"),
            [own("/dev/block/by-name/nr_*", Owner::Vendor, Group::Vendor, 0o660)]
        );
        // the GPU: root:video 0660, not everyone's
        assert_eq!(find("/dev/mali0"), [own("/dev/mali0", Owner::Root, Group::Gpu, 0o660)]);
        // root's alone: input events and the USB serial console
        assert_eq!(find("/dev/input/event*"), [own("/dev/input/event*", Owner::Root, Group::Root, 0o600)]);
        assert_eq!(find("/dev/ttyGS*"), [own("/dev/ttyGS*", Owner::Root, Group::Root, 0o600)]);
        // the RTC: root's, in the vendor group (refnotify opens it)
        assert_eq!(find("/dev/rtc*"), [own("/dev/rtc*", Owner::Root, Group::Vendor, 0o660)]);
        // a later rule wins: spipe_nr4 is root's after spipe_nr* was the vendor's
        let order: Vec<_> = entries().map(|e| e.glob).collect();
        let pos = |g| order.iter().position(|x| *x == g).unwrap_or(usize::MAX);
        assert!(pos("/dev/spipe_nr*") < pos("/dev/spipe_nr4"));
        assert_eq!(find("/dev/spipe_nr4"), [own("/dev/spipe_nr4", Owner::Root, Group::Root, 0o600)]);
        // no block device by number but the RPMB (FINDINGS 31l)
        assert!(entries().all(|e| !e.glob.starts_with("/dev/block/mmcblk") || e.glob.ends_with("rpmb")));
    }
}
