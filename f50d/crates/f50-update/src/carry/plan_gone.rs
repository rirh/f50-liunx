//! The end of the plan: what the image had and the old system has not (deleted by the user, or of a package that
//! was removed and is back with the image), and the enable links of the new image's services.

use std::collections::HashSet;

use super::super::rules;
use super::{Act, Planner};

/// What the image had and the old system has not (deleted by the user), packages that are back, and the services
/// of both.
pub(super) fn finish(pl: &mut Planner<'_, '_>, seen: &HashSet<&str>, had: &HashSet<&str>, retyped: &HashSet<String>) {
    let i = pl.i;
    let mut back: HashSet<String> = HashSet::new();
    if let Some(om) = pl.om {
        let gone: HashSet<&str> = om.keys().map(String::as_str).filter(|p| !seen.contains(p)).collect();
        let mut gone_sorted: Vec<&str> = gone.iter().copied().collect();
        gone_sorted.sort_unstable();
        for p in gone_sorted {
            let d = rules::parent(p);
            if gone.contains(d) || retyped.contains(d) || !i.new.contains_key(p) {
                continue;
            }
            if rules::never(p)
                || rules::other(p)
                || rules::vendor(p)
                || rules::quiet(p)
                || p.starts_with("etc/uci-defaults/")
            {
                continue;
            }
            if let Some(pkg) = i.new_pkg.owned.get(p)
                && !i.old_pkg.installed.contains(pkg)
            {
                back.insert(pkg.clone());
                continue;
            }
            if !rules::user_area(p) {
                continue;
            }
            let upgraded = i.new_pkg.owned.contains_key(p) && !i.old_pkg.owned.contains_key(p);
            if rules::enable_link(p) || (!rules::project(p, i.cmds) && !rules::script(p) && !upgraded) {
                pl.out.acts.push(Act::Remove(p.to_owned()));
                pl.note('d', p);
            } else {
                pl.note('k', p);
            }
        }
    }
    let mut back_sorted: Vec<&String> = back.iter().collect();
    back_sorted.sort();
    for a in back_sorted {
        pl.note('p', a);
    }
    let (mut svc, mut svcold) = (HashSet::new(), HashSet::new());
    if !back.is_empty() {
        for (p, pkg) in &i.new_pkg.owned {
            if back.contains(pkg) && rules::service_file(p) {
                if seen.contains(p.as_str()) {
                    svcold.insert(rules::basename(p))
                } else {
                    svc.insert(rules::basename(p))
                };
            }
        }
    }
    let known: HashSet<&str> = had.iter().filter(|p| rules::service_file(p)).map(|p| rules::basename(p)).collect();
    let mut links: Vec<&String> = i.new.keys().filter(|p| rules::enable_link(p)).collect();
    links.sort();
    for p in links {
        let b = rules::service_of(p);
        if svc.contains(b) || (svcold.contains(b) && !seen.contains(p.as_str())) {
            pl.out.acts.push(Act::Remove(p.clone()));
        } else if pl.out.fallback && !had.contains(p.as_str()) && known.contains(b) {
            // without a manifest: a service the old system had and did not enable stays off
            pl.out.acts.push(Act::Remove(p.clone()));
            pl.note('e', p);
        }
    }
}
