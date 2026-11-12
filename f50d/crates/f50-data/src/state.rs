//! What the data service knows about the bearer now: the phase of the connection, why it is held, the address it
//! was given. Written by the dial and the teardown, read by the watchdog and `f50.data get`. Behind a std mutex that
//! is never held across an `.await`.

use std::time::Instant;

use f50_modem::Slot;
use serde::Serialize;

use crate::dial::Bearer;

/// Where the connection is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Phase {
    /// No connection, and none asked for (netifd has not set the interface up, or took it down).
    #[default]
    Down,
    /// netifd asked for it; the dial runs.
    Connecting,
    /// The bearer is up and netifd has it.
    Connected,
    /// Not dialled on purpose (see [`Hold`]).
    Held,
    /// The last dial failed; netifd tries again.
    Failed,
}

/// Why data is held back.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Hold {
    /// The card is registered as roaming and data roaming is off (FINDINGS 120).
    Roaming,
    /// No card in the slot in use (FINDINGS 186): the interface is unavailable until one answers.
    SimMissing,
    /// The radio is off (flight mode, or switched off for this card: FINDINGS 185); the modem service owns it.
    RadioOff,
}

#[derive(Debug, Default)]
pub(crate) struct State {
    pub phase: Phase,
    pub hold: Option<Hold>,
    /// netifd's interface the session is for (the `config` of the last setup).
    pub config: Option<String>,
    pub bearer: Option<Bearer>,
    pub slot: Option<Slot>,
    /// Relay mode was chosen for this session.
    pub relay: bool,
    pub since: Option<Instant>,
    /// The reason of the last failed dial (sanitised, never a secret).
    pub last_error: Option<String>,
}

impl State {
    pub fn connecting(&mut self, config: &str) {
        self.phase = Phase::Connecting;
        self.config = Some(config.to_owned());
        self.since = Some(Instant::now());
    }

    pub fn connected(&mut self, bearer: Bearer, slot: Slot, relay: bool) {
        self.phase = Phase::Connected;
        self.hold = None;
        self.bearer = Some(bearer);
        self.slot = Some(slot);
        self.relay = relay;
        self.since = Some(Instant::now());
        self.last_error = None;
    }

    pub fn held(&mut self, hold: Hold) {
        self.phase = Phase::Held;
        self.hold = Some(hold);
        self.bearer = None;
    }

    pub fn failed(&mut self, why: &str) {
        self.phase = Phase::Failed;
        self.hold = None;
        self.bearer = None;
        self.last_error = Some(why.to_owned());
    }

    /// The teardown: nothing is up. A hold for a missing card outlives it (the interface stays unavailable until
    /// the watchdog sees a card); the others end with it.
    pub fn down(&mut self) {
        self.phase = Phase::Down;
        self.bearer = None;
        self.relay = false;
        self.since = None;
        if self.hold != Some(Hold::SimMissing) {
            self.hold = None;
        }
    }
}
