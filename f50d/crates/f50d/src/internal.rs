//! `f50.daemon`: what the device's own scripts ask of the running daemon. Never in an ACL: uhttpd refuses it to
//! every login; only root on the device (hotplug, netifd, the command line) reaches it.

use std::sync::{Arc, OnceLock};

use f50_core::rpc::{Access, Field, Object, Params, Rule};
use serde_json::{Value, json};

const HOTPLUG: &[Field] = &[
    Field::req("action", Rule::Word { re: "[a-z]{1,16}", max: 16 }),
    Field::req("interface", Rule::Word { re: "[A-Za-z0-9_.:@][A-Za-z0-9_.:@-]{0,31}", max: 32 }),
];

/// `acl` is filled in once the whole API exists (it is generated from it).
pub fn object(net: f50_net::Net, data: f50_data::Data, acl: Arc<OnceLock<Value>>) -> Object {
    let (n1, n2) = (net.clone(), net);
    Object::builder("f50.daemon")
        .method(
            "hotplug_iface",
            Access::Write,
            HOTPLUG,
            "netifd's iface hotplug (ACTION, INTERFACE).",
            move |p: Params| {
                let n = n1.clone();
                async move {
                    n.hotplug_iface(p.need_str("action")?, p.need_str("interface")?).await;
                    Ok(json!({}))
                }
            },
        )
        .method(
            "hotplug_net",
            Access::Write,
            HOTPLUG,
            "The kernel's net hotplug (ACTION, INTERFACE).",
            move |p: Params| {
                let n = n2.clone();
                async move {
                    n.hotplug_net(p.need_str("action")?, p.need_str("interface")?).await;
                    Ok(json!({}))
                }
            },
        )
        .method("clock_synced", Access::Write, &[], "NTP set the clock: data usage may trust it.", move |_p: Params| {
            let r = data.clock_synced().map(|()| json!({}));
            async move { r }
        })
        .method("acl", Access::Read, &[], "The rpcd ACL generated from the method table.", move |_p: Params| {
            let v = acl.get().cloned().unwrap_or(Value::Null);
            async move { Ok(v) }
        })
        .build()
}
