//! Kernel modules at boot: the vendor drivers in their order ([`order`]), the kernel's module helper that keeps
//! users from loading modules on demand ([`guard`]), and the IPA delegate, which has to wait for the modem ([`dele`]).

pub(crate) mod dele;
pub(crate) mod guard;
pub(crate) mod order;
