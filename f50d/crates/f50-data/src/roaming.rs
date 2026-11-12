//! Data roaming (FINDINGS 120): with it off, no data while the card in use is registered on a roaming network.
//!
//! No switch of the modem's own is known (on Android the decision is the framework's too), so this is a policy of
//! the dial and the watchdog: no context is activated, so no data leaves; the registration, IMS, SMS and emergency
//! calls are untouched. Allowed by default, per card (`roaming`, `roaming_internal`: [`crate::settings`]).
//!
//! "Roaming" is the network's word: `<stat>` of `AT+CEREG?`, and of `AT+C5GREG?` when LTE's says neither (5G SA).
//! 5 and 10 are roaming, 1 and 9 home (3GPP TS 27.007); 6-8 count as neither. Some SIMs report roaming at home (an
//! MVNO, national roaming): the setting is the user's answer to that, never a guess of ours. The card's own AT
//! channel answers for its card alone (FINDINGS 74).

use f50_at::At;
use serde::Serialize;

use crate::radio::reg_stat;

/// The network the card is registered on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Network {
    Home,
    Roaming,
    Unknown,
}

/// Home, roaming or unknown from `<stat>` values (an absent one says nothing).
pub fn classify(stats: &[Option<u8>]) -> Network {
    let mut r = Network::Unknown;
    for s in stats.iter().flatten() {
        match s {
            5 | 10 => return Network::Roaming,
            1 | 9 => r = Network::Home,
            _ => {}
        }
    }
    r
}

/// The card's network now: one question in the usual case (LTE, NSA), the 5G registration only when LTE's says
/// neither.
pub async fn network(at: &At) -> Network {
    let s4 = reg_stat(at, "AT+CEREG?").await;
    let s5 = if matches!(s4, Some(1 | 5 | 9 | 10)) { None } else { reg_stat(at, "AT+C5GREG?").await };
    classify(&[s4, s5])
}

/// May the card have data now?
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gate {
    Yes,
    /// Roaming with data roaming off: held back.
    Held,
    /// The registration could not be read: with data roaming off that is no dial either, but no hold.
    Unknown,
}

pub async fn gate(at: &At, allowed: bool) -> Gate {
    if allowed {
        return Gate::Yes;
    }
    match network(at).await {
        Network::Home => Gate::Yes,
        Network::Roaming => Gate::Held,
        Network::Unknown => Gate::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_networks_word() {
        assert_eq!(classify(&[Some(1), None]), Network::Home);
        assert_eq!(classify(&[Some(2), Some(5)]), Network::Roaming);
        assert_eq!(classify(&[Some(10)]), Network::Roaming);
        assert_eq!(classify(&[Some(9)]), Network::Home);
        assert_eq!(classify(&[Some(6), Some(8)]), Network::Unknown);
        assert_eq!(classify(&[None, None]), Network::Unknown);
    }
}
