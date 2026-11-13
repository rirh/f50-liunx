//! The names of a release's files and of what is inside its images: the one place that knows them.
//!
//! The releases are built by `scripts/release/make-release.sh`: each system is a root filesystem tarball, each kernel
//! a bundle, and `SHA256SUMS` (with `f50-release`, the tag as a file, so that the checksums say which release they are
//! of) is signed. Every name is the F50's (`f50-*`, `etc/f50/...`) first and the one releases and images had before
//! the rename (`mu300-*`, `etc/mu300/...`) second: a device takes whichever a release or a system has.

use std::path::{Path, PathBuf};

/// The checksums of every file of a release ("HASH  NAME" lines).
pub const SUMS: &str = "SHA256SUMS";
/// The signature of `SHA256SUMS` in minisign's format (verified in Rust; preferred when a release has it).
pub const SUMS_MINISIG: &str = "SHA256SUMS.minisig";
/// The signature in usign's format (what releases publish today: a comment line and one base64 line).
pub const SUMS_SIG: &str = "SHA256SUMS.sig";
/// The tag of the release as a file ("TAG\n"), listed in `SHA256SUMS`: its F50 name, then its older one.
pub const RELEASE_TAGS: [&str; 2] = ["f50-release", "mu300-release"];

/// Inside an image (and an installed system): the release it is.
pub const IMAGE_VERSION: &str = "etc/f50/image-version";
/// Inside an image: the manifest of its own files (gzip; first line `f50-image-manifest 1 IMAGE TAG`).
pub const IMAGE_MANIFEST: &str = "etc/f50/image-manifest.gz";
/// The first word of an image's own manifest, and of the union of older images' manifests.
pub const MANIFEST_HEADS: [&str; 2] = ["f50-image-manifest", "mu300-image-manifest"];
pub const UNION_HEADS: [&str; 2] = ["f50-image-manifests", "mu300-image-manifests"];
/// Inside an image: the manifests of the images of releases before images carried their own.
pub const LEGACY_MANIFESTS: &str = "opt/f50/lib/image-manifests";
/// Inside an image: the commands it links into /usr/local/bin (the project's own, not the user's).
pub const PATH_COMMANDS: &str = "opt/f50/lib/path-commands";
/// Inside a system: what the last updates carried over, set aside or put back, and its "not read yet" mark.
pub const REPORT: &str = "etc/f50/update-report";
/// Inside a system: where the versions an update set aside go (`<from>/user`, `<from>/image`).
pub const KEPT: &str = "etc/f50/update-kept";

/// `rel` (one of the names above) inside the system or image at `root`: under its older directory (`etc/mu300`,
/// `opt/mu300`) when that is the one `root` has.
pub fn within(root: &Path, rel: &str) -> PathBuf {
    f50_core::paths::Paths::new(root).sys(format!("/{rel}"))
}

/// The name `rel` had before the rename (`etc/f50/x` -> `etc/mu300/x`), the same when it had no other.
pub fn legacy(rel: &str) -> String {
    for (new, old) in [("etc/f50/", "etc/mu300/"), ("opt/f50/", "opt/mu300/")] {
        if let Some(rest) = rel.strip_prefix(new) {
            return format!("{old}{rest}");
        }
    }
    rel.to_owned()
}

/// The root filesystem of a system: its F50 name, then its older one.
pub fn rootfs(system: &str) -> [String; 2] {
    ["f50", "mu300"].map(|p| format!("{p}-{system}-rootfs.tar.gz"))
}

/// The kernel bundle of a kernel choice (5.4 is the vendor kernel's): its F50 name, then its older one.
pub fn kernel(choice: &str) -> [String; 2] {
    ["f50", "mu300"]
        .map(|p| if choice == "5.4" { format!("{p}-kernel.tar.gz") } else { format!("{p}-kernel-{choice}.tar.gz") })
}

/// The first of `names` that `has` finds.
pub fn first<S: AsRef<str>>(names: &[S], has: impl Fn(&str) -> bool) -> Option<&str> {
    names.iter().map(AsRef::as_ref).find(|n| has(n))
}

/// Inside a kernel bundle.
pub mod bundle {
    pub const IMAGE: &str = "./Image";
    pub const RAMDISK: &str = "./ramdisk-generic.lz4";
    pub const RELEASE: &str = "./kernel.release";
    pub const MODULES: &str = "./modules";
    pub const DEVICES: &str = "./devices";
    pub const FEATURES: &str = "./features";
}

/// The members an offline bundle may hold (`apply_file`): every name a release of these systems and kernels has.
pub fn offline_names(systems: &[String]) -> Vec<String> {
    let mut v: Vec<String> = [SUMS, SUMS_MINISIG, SUMS_SIG].iter().map(|s| (*s).to_owned()).collect();
    v.extend(RELEASE_TAGS.iter().map(|s| (*s).to_owned()));
    v.extend(systems.iter().flat_map(|s| rootfs(s)));
    v.extend(crate::config::KERNELS.iter().flat_map(|k| kernel(k)));
    v
}

/// A file name of a release as it may be used under the staging directory: one plain name.
pub fn name_ok(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && !name.starts_with('.')
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || b"._+-".contains(&b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert_eq!(rootfs("openwrt-luci"), ["f50-openwrt-luci-rootfs.tar.gz", "mu300-openwrt-luci-rootfs.tar.gz"]);
        assert_eq!(kernel("5.4"), ["f50-kernel.tar.gz", "mu300-kernel.tar.gz"]);
        assert_eq!(kernel("6.18")[1], "mu300-kernel-6.18.tar.gz");
        assert_eq!(legacy(IMAGE_VERSION), "etc/mu300/image-version");
        assert_eq!(legacy(LEGACY_MANIFESTS), "opt/mu300/lib/image-manifests");
        assert!(offline_names(&["openwrt".into()]).iter().all(|n| name_ok(n)));
        assert!(!name_ok("../x") && !name_ok(".hidden") && !name_ok("a/b"));
    }
}
