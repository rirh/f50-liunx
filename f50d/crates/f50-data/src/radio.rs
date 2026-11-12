//! The few radio questions data asks the modem, and the watchdog's radio off and on. Bringing the radio up (the
//! RIL handshake, AT+SFUN=4, each card's radio switch of FINDINGS 185) is the modem service's `radio_on`, which the
//! dial and the watchdog call under the radio lock.

use std::time::Duration;

use f50_at::{At, SendOpts};
use f50_modem::{Modem, RadioOn};

const Q: Duration = Duration::from_secs(4);

/// `AT+CFUN?`: 1 on, 0 off (cold, or AT+SFUN=5), 2 off, 4 flight mode; `None` without an answer.
pub(crate) async fn cfun(at: &At) -> Option<u8> {
    let r = at.send("AT+CFUN?", SendOpts::control(Q)).await.ok()?;
    r.value("+CFUN")?.trim().split(',').next()?.trim().parse().ok()
}

/// `<stat>` of a registration query's answer, `+C...REG: <n>,<stat>[,...]`.
pub(crate) fn reg_stat_of(value: &str) -> Option<u8> {
    let f = value.split(',').nth(1)?;
    let d: String = f.chars().filter(char::is_ascii_digit).collect();
    d.parse().ok()
}

/// `<stat>` of `AT+CEREG?` or `AT+C5GREG?`.
pub(crate) async fn reg_stat(at: &At, cmd: &str) -> Option<u8> {
    let r = at.send(cmd, SendOpts::control(Q)).await.ok()?;
    let prefix = if cmd.contains("C5GREG") { "+C5GREG" } else { "+CEREG" };
    reg_stat_of(r.value(prefix)?)
}

/// Registered on a network, home (1) or roaming (5).
pub(crate) fn registered(stat: Option<u8>) -> bool {
    matches!(stat, Some(1 | 5))
}

/// The packet domain is attached (`+CGATT: 1`).
pub(crate) async fn attached(at: &At) -> bool {
    let Ok(r) = at.send("AT+CGATT?", SendOpts::control(Duration::from_secs(8))).await else { return false };
    r.value("+CGATT").is_some_and(|v| v.trim() == "1")
}

/// The radio on again when it is allowed and found off (the watchdog: a switched-off radio never comes back on
/// its own). The modem service's own sequence, under the radio lock.
pub(crate) async fn ensure_on(modem: &Modem) {
    if !modem.radio_allowed() {
        return;
    }
    let Ok(guard) = modem.radio_guard("data.watchdog", Duration::from_secs(20)).await else { return };
    match modem.radio_on(&guard).await {
        Ok(RadioOn::On) => tracing::info!("the radio was off; switched on"),
        Ok(RadioOn::Disabled) => {}
        Err(e) => tracing::warn!(error = %e, "the radio did not come on"),
    }
}

/// The radio off and on (the watchdog's step for a dead downlink): AT+SFUN=5, then the modem service's bring-up,
/// which waits for `+CFUN: 1`. The modem builds its packet domain again. Under the radio lock; only for a radio that
/// is on and allowed, so it never switches on a radio somebody switched off.
pub(crate) async fn cycle(at: &At, modem: &Modem) -> bool {
    if !modem.radio_allowed() {
        return false;
    }
    let Ok(guard) = modem.radio_guard("data.watchdog", Duration::from_secs(20)).await else {
        tracing::warn!("the radio is busy; the radio off-and-on step is skipped");
        return false;
    };
    if cfun(at).await != Some(1) {
        return false;
    }
    let _ = at.send("AT+SFUN=5", SendOpts::control(Duration::from_secs(10))).await;
    tokio::time::sleep(Duration::from_secs(2)).await;
    match modem.radio_on(&guard).await {
        Ok(RadioOn::On) => true,
        Ok(RadioOn::Disabled) => false,
        Err(e) => {
            tracing::warn!(error = %e, "the radio did not come back");
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registration_stat() {
        assert_eq!(reg_stat_of("2,1,\"1A2B\",\"01A2B3C4\",7"), Some(1));
        assert_eq!(reg_stat_of("0,5"), Some(5));
        assert_eq!(reg_stat_of("1"), None);
        assert!(registered(Some(5)) && !registered(Some(2)) && !registered(None));
    }
}
