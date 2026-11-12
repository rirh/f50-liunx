//! The rpcd ACL, generated from the method table.
//!
//! Two groups: `f50-read` grants every read method (nothing changes, nothing private comes back), `f50-admin` grants
//! everything. A login gets them through `/etc/config/rpcd` (`list read 'f50-read'`, ...); root has every group.
//! uhttpd's `/ubus` checks the caller's session against these before an invocation reaches f50d.

use std::collections::BTreeMap;

use f50_core::rpc::{Access, Object};
use serde_json::{Value, json};

fn methods(objects: &[Object], only_read: bool) -> BTreeMap<&'static str, Vec<&'static str>> {
    let mut m: BTreeMap<&'static str, Vec<&'static str>> = BTreeMap::new();
    for o in objects {
        let names: Vec<&'static str> =
            o.methods.iter().filter(|x| !only_read || x.access == Access::Read).map(|x| x.name).collect();
        if !names.is_empty() {
            m.insert(o.name, names);
        }
    }
    m
}

/// `/usr/share/rpcd/acl.d/f50.json` for `objects`.
pub fn generate(objects: &[Object]) -> Value {
    json!({
        "f50-read": {
            "description": "F50: read the device's state (no changes, nothing private)",
            "read": { "ubus": methods(objects, true) },
        },
        "f50-admin": {
            "description": "F50: read and change everything",
            "read": { "ubus": methods(objects, true) },
            "write": { "ubus": methods(objects, false) },
        },
    })
}
