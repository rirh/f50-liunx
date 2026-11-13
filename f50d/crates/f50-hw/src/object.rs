//! `f50.boot`: the boot control and the boot stages' results, and one switch: `android` makes the next boot
//! Android's and reboots (Linux is started again from Android). Arming Linux, locking it and the attempts stay out of
//! the API.

use f50_core::rpc::{Access, Object, Params};

use crate::Hw;

pub(crate) fn boot(hw: Hw) -> Object {
    let hw2 = hw.clone();
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
        .method(
            "android",
            Access::Write,
            &[],
            "Makes the next boot Android's (the initramfs' Android block, written and read back) and reboots in two \
             seconds. The device stays in Android until Linux is started from there.",
            move |_: Params| {
                let hw = hw2.clone();
                async move { hw.switch_to_android(std::time::Duration::from_secs(2)).await }
            },
        )
        .build()
}
