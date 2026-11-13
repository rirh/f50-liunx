//! The manifest of the image a system came from: which of its files the release brought (scripts/release/
//! image-manifest.py has the format: `TYPE MODE UID GID SIZE HASH PATH` lines, a backslash written `\\` and a newline
//! `\n`). The system's own (`etc/f50/image-manifest.gz`, first line `f50-image-manifest 1 IMAGE TAG`; `etc/mu300`, `mu300-` before the rename) counts
//! when it names this image and the release the system says it is; else the one the new image carries for that
//! release (`opt/f50/lib/image-manifests/IMAGE.gz`: `f50-image-manifests 1 IMAGE`, a line of tags, then lines
//! that start with the ranges of tag indexes they belong to). One bad line refuses the whole manifest, and the carry
//! goes on as without one.

use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use f50_core::exec::Cmd;

use crate::release::assets;

/// One file of an image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManEntry {
    pub kind: char,
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
    pub size: u64,
    /// sha256 of a file's content or a link's target; `None` for anything else.
    pub hash: Option<String>,
}

pub type Manifest = HashMap<String, ManEntry>;

/// A path of a manifest line: `\\` and `\n` unescaped; relative, without `.`, `..` or empty parts.
pub fn unescape(p: &str) -> Option<String> {
    let mut out = String::with_capacity(p.len());
    let mut it = p.chars();
    while let Some(c) = it.next() {
        if c == '\\' {
            match it.next()? {
                '\\' => out.push('\\'),
                'n' => out.push('\n'),
                _ => return None,
            }
        } else {
            out.push(c);
        }
    }
    let ok = !out.is_empty() && !out.starts_with('/') && out.split('/').all(|s| !s.is_empty() && s != "." && s != "..");
    ok.then_some(out)
}

/// One `TYPE MODE UID GID SIZE HASH PATH` line.
pub fn parse_line(l: &str) -> Option<(String, ManEntry)> {
    let mut it = l.splitn(7, ' ');
    let kind = it.next()?;
    let kind = match kind {
        "f" | "d" | "l" | "c" | "b" | "p" => kind.chars().next()?,
        _ => return None,
    };
    let mode = u32::from_str_radix(it.next()?, 8).ok()?;
    let uid = it.next()?.parse().ok()?;
    let gid = it.next()?.parse().ok()?;
    let size = it.next()?.parse().ok()?;
    let h = it.next()?;
    let hash = match h {
        "-" => None,
        h if !h.is_empty() && h.bytes().all(|b| b.is_ascii_hexdigit()) => Some(h.to_ascii_lowercase()),
        _ => return None,
    };
    let path = unescape(it.next()?)?;
    Some((path, ManEntry { kind, mode: mode & 0o7777, uid, gid, size, hash }))
}

/// A system's own manifest: the first line must name `image` and `tag`.
pub fn parse_own(text: &str, image: &str, tag: &str) -> Option<Manifest> {
    let mut lines = text.lines();
    let first = lines.next()?;
    if !assets::MANIFEST_HEADS.iter().any(|h| first == format!("{h} 1 {image} {tag}")) {
        return None;
    }
    let m: Option<Manifest> = lines.map(parse_line).collect();
    m.filter(|m| !m.is_empty())
}

/// One release's manifest out of the union the new image carries.
pub fn parse_union(text: &str, image: &str, tag: &str) -> Option<Manifest> {
    let mut lines = text.lines();
    let head: Vec<&str> = lines.next()?.split(' ').collect();
    if head.len() < 3 || !assets::UNION_HEADS.contains(&head[0]) || head[1] != "1" || head[2] != image {
        return None;
    }
    let tags: Vec<&str> = lines.next()?.split(' ').skip(1).collect();
    let n = tags.iter().position(|t| *t == tag)?;
    let mut m = Manifest::new();
    for l in lines {
        let (ranges, rest) = l.split_once(' ')?;
        let mut hit = false;
        for r in ranges.split(',') {
            let (a, b) = r.split_once('-').unwrap_or((r, r));
            let (a, b): (usize, usize) = (a.parse().ok()?, b.parse().ok()?);
            hit |= (a..=b).contains(&n);
        }
        if hit {
            let (p, e) = parse_line(rest)?;
            m.insert(p, e);
        }
    }
    (!m.is_empty()).then_some(m)
}

async fn gunzip(path: &Path) -> Option<String> {
    let md = std::fs::symlink_metadata(path).ok()?;
    if !md.file_type().is_file() {
        return None;
    }
    let data = std::fs::read(path).ok()?;
    let out = Cmd::new("gzip").arg("-dc").stdin(data).max_output(64 << 20).timeout(Duration::from_secs(60)).run().await;
    match out {
        Ok(o) if o.success() => Some(o.stdout),
        _ => None,
    }
}

/// The manifest of the image `old` (a system of image `image`, release `tag`) came from, from `old` itself or from
/// `new`'s collection.
pub async fn load(old: &Path, new: &Path, image: &str, tag: &str) -> Option<Manifest> {
    if let Some(t) = gunzip(&assets::within(old, assets::IMAGE_MANIFEST)).await
        && let Some(m) = parse_own(&t, image, tag)
    {
        return Some(m);
    }
    let t = gunzip(&assets::within(new, assets::LEGACY_MANIFESTS).join(format!("{image}.gz"))).await?;
    parse_union(&t, image, tag)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_and_paths() {
        let (p, e) = parse_line("f 644 0 0 3 ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad etc/a b")
            .expect("line");
        assert_eq!((p.as_str(), e.kind, e.mode, e.size), ("etc/a b", 'f', 0o644, 3));
        assert_eq!(parse_line("d 755 0 0 0 - etc").map(|(p, e)| (p, e.hash)), Some(("etc".into(), None)));
        assert!(parse_line("f 644 0 0 3 - ../etc/passwd").is_none());
        assert!(parse_line("x 644 0 0 3 - etc").is_none());
        assert_eq!(unescape("a\\nb\\\\c").as_deref(), Some("a\nb\\c"));
        assert!(unescape("a//b").is_none() && unescape("/abs").is_none());
    }

    #[test]
    fn own_and_union() {
        let own = "mu300-image-manifest 1 openwrt v2026.11.01.1\nd 755 0 0 0 - etc\n";
        assert!(parse_own(own, "openwrt", "v2026.11.01.1").is_some());
        assert!(parse_own(own, "openwrt", "v2026.11.02.1").is_none(), "another release's");
        assert!(parse_own(&format!("{own}garbage\n"), "openwrt", "v2026.11.01.1").is_none(), "one bad line refuses it");
        let union = "mu300-image-manifests 1 openwrt\ntags v2026.10.25 v2026.10.26 v2026.10.27\n0-1 d 755 0 0 0 - etc\n2 d 755 0 0 0 - www\n0,2 d 700 0 0 0 - root\n";
        let m = parse_union(union, "openwrt", "v2026.10.27").expect("union");
        assert!(m.contains_key("www") && m.contains_key("root") && !m.contains_key("etc"));
        assert!(parse_union(union, "openwrt", "v2026.10.20").is_none());
        let own = own.replacen("mu300-", "f50-", 1);
        assert!(parse_own(&own, "openwrt", "v2026.11.01.1").is_some(), "the F50 name");
        assert!(parse_union(&union.replacen("mu300-", "f50-", 1), "openwrt", "v2026.10.27").is_some());
    }
}
