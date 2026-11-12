//! `f50.boot`: the boot control and the boot stages' results, read only. Switching the operating system is not part
//! of the API.

use f50_core::rpc::{Access, Object, Params};

use crate::Hw;

pub(crate) fn boot(hw: Hw) -> Object {
    Object::builder("f50.boot")
        .method(
            "get",
            Access::Read,
            &[],
            "The slot Linux runs from, the default boot, the failed boots allowed before Android, whether Linux is \
             locked, the boot control block, and the last result of every boot stage.",
            move |_: Params| {
                let hw = hw.clone();
                async move { hw.boot_info().await }
            },
        )
        .build()
}
