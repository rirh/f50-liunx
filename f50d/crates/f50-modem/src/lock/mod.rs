//! The network locks: network mode (`SPTESTMODE` + `SP5GRAN`), LTE and NR band locks (`SPLBAND` masks), cell locks
//! (`SPFORCEFRQ`), EN-DC (`SPENDC`). Ported from `unisoc-modem/lock` and `boot-replay`.
//!
//! [`codec`] the encodings and the value checks, [`read`] the cached reading, [`apply`] one change, [`reset`] all
//! back to automatic, verified, [`replay`] the boot replay, [`state`] the saved settings.
//!
//! Applying anything restarts the protocol stack (`AT+SFUN=5`, then `AT+SFUN=4`): cellular drops for half a
//! minute and the data watchdog brings it back. Locks on the internal card (SIM2) are refused, as the shell did:
//! its cold start differs and the replay there is not validated.

pub mod apply;
pub mod codec;
pub mod read;
pub mod replay;
pub mod reset;
pub mod state;

use serde_json::{Value, json};

use crate::shared::{Inner, lock};

/// `f50.lock get`: the cached reading, the replay switch and the last reset's result; never a modem read inside
/// the request (a missing or old reading starts one in the background).
pub(crate) fn get(inner: &std::sync::Arc<Inner>, fresh: bool) -> Value {
    let reading = read::cached(inner);
    let old = reading
        .as_ref()
        .is_none_or(|r| f50_core::time::unix_now().saturating_sub(r.ts) > inner.cfg.lock_cache_max_age.as_secs());
    if fresh || old {
        read::spawn_refresh(inner);
    }
    json!({
        "auto_apply": state::auto_apply(inner.paths()),
        "reset": lock(&inner.reset_result).clone(),
        "lock": reading.as_deref(),
        "refreshing": inner.lock_refreshing.load(std::sync::atomic::Ordering::SeqCst),
        "applying": inner.jobs.running("radio"),
    })
}
