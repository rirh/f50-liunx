//! What each LED shows, from the settings, the inputs and the moment: a pure function, so every rule is testable.

use super::settings::{LedSettings, ThermalLight};
use super::{LedInputs, NetworkLed};

/// The F50's LEDs (docs/research/FINDINGS.md 34).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Lamp {
    Red,
    Green,
    Blue,
    Wifi,
}

pub(crate) const LAMPS: [Lamp; 4] = [Lamp::Red, Lamp::Green, Lamp::Blue, Lamp::Wifi];
/// The siren's and the boot indicator's order: the PMIC's RGB LED one colour at a time (the white outshines the
/// others when mixed).
const SIREN: [Lamp; 3] = [Lamp::Red, Lamp::Green, Lamp::Blue];
/// ZTE lights the Wi-Fi sink at 48 of 127: a low-current sink.
pub(crate) const WIFI_LEVEL: u32 = 48;

impl Lamp {
    pub(crate) fn sysfs_name(self) -> &'static str {
        match self {
            Lamp::Red => "sc27xx:red",
            Lamp::Green => "sc27xx:green",
            Lamp::Blue => "sc27xx:blue",
            Lamp::Wifi => "keyboard-backlight",
        }
    }

    pub(crate) fn index(self) -> usize {
        match self {
            Lamp::Red => 0,
            Lamp::Green => 1,
            Lamp::Blue => 2,
            Lamp::Wifi => 3,
        }
    }
}

/// One LED's state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Out {
    Off,
    Steady(u32),
    /// The kernel's timer trigger, `ms` on and `ms` off.
    Blink {
        level: u32,
        ms: u32,
    },
}

/// `level` at `pct` %, never 0 for a lit lamp.
pub(crate) fn dim(level: u32, pct: u8) -> u32 {
    if pct >= 100 || level == 0 {
        return level;
    }
    ((level * u32::from(pct) + 50) / 100).max(1)
}

/// What the overlays are doing now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Overlay {
    None,
    /// The heat alarm as a siren, at this step.
    Siren(usize),
    /// The heat alarm as the red blinking.
    AlarmBlink,
    /// The boot indicator, at this step.
    Boot(usize),
}

impl Overlay {
    pub(crate) fn of(settings: &LedSettings, alarm: bool, boot: bool, phase: usize) -> Self {
        match (alarm, settings.thermal, boot) {
            (true, ThermalLight::Siren, _) => Overlay::Siren(phase),
            (true, ThermalLight::Blink, _) => Overlay::AlarmBlink,
            // the heat alarm goes first; switched off, it shows nothing and leaves the boot indicator running
            (_, _, true) => Overlay::Boot(phase),
            (_, _, false) => Overlay::None,
        }
    }
}

/// Everything one rendering depends on.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Scene<'a> {
    pub settings: &'a LedSettings,
    pub inputs: &'a LedInputs,
    /// Shown now: always without a timeout, else for the timeout after a key (in idle: only after a key).
    pub lit: bool,
    pub overlay: Overlay,
    /// Each LED's max_brightness.
    pub max: [u32; 4],
}

/// The network lamp's colour, steady or blinking.
fn network_colour(i: &LedInputs) -> Option<(Lamp, bool)> {
    match (i.network, i.data_up) {
        (NetworkLed::None, _) => Some((Lamp::Red, false)),
        (NetworkLed::Searching, _) => Some((Lamp::Red, true)),
        (NetworkLed::G5, true) => Some((Lamp::Green, false)),
        (NetworkLed::G2 | NetworkLed::G3 | NetworkLed::G4, true) => Some((Lamp::Blue, false)),
        (_, false) => None,
    }
}

pub(crate) fn render(s: &Scene<'_>) -> [Out; 4] {
    let mut out = [Out::Off; 4];
    let max = |l: Lamp| s.max[l.index()];
    // the lamps as usual
    if s.lit
        && s.settings.network
        && let Some((l, blink)) = network_colour(s.inputs)
    {
        let level = dim(max(l), s.settings.network_brightness);
        out[l.index()] = if blink { Out::Blink { level, ms: 500 } } else { Out::Steady(level) };
    }
    if s.lit && s.settings.wifi && s.inputs.wifi_on {
        out[Lamp::Wifi.index()] = Out::Steady(dim(WIFI_LEVEL.min(max(Lamp::Wifi)), s.settings.wifi_brightness));
    }
    // the overlays own the RGB LED: the alarm at full brightness whatever the switch, the boot indicator as the
    // network lamp's switch and brightness say
    match s.overlay {
        Overlay::None => {}
        Overlay::Siren(phase) => {
            let on = SIREN[phase % SIREN.len()];
            for l in SIREN {
                out[l.index()] = if l == on { Out::Steady(max(l)) } else { Out::Off };
            }
        }
        Overlay::AlarmBlink => {
            for l in SIREN {
                out[l.index()] = Out::Off;
            }
            out[Lamp::Red.index()] = Out::Blink { level: max(Lamp::Red), ms: 200 };
        }
        Overlay::Boot(phase) => {
            let on = SIREN[phase % SIREN.len()];
            for l in SIREN {
                out[l.index()] = if l == on && s.settings.network {
                    Out::Steady(dim(max(l), s.settings.network_brightness))
                } else {
                    Out::Off
                };
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAX: [u32; 4] = [255, 255, 255, 127];

    fn scene<'a>(s: &'a LedSettings, i: &'a LedInputs, lit: bool, overlay: Overlay) -> [Out; 4] {
        render(&Scene { settings: s, inputs: i, lit, overlay, max: MAX })
    }

    #[test]
    fn network_and_wifi_as_zte() {
        let s = LedSettings::default();
        let mut i = LedInputs { network: NetworkLed::G4, data_up: true, wifi_on: true, ..LedInputs::default() };
        assert_eq!(scene(&s, &i, true, Overlay::None), [Out::Off, Out::Off, Out::Steady(255), Out::Steady(48)]);
        i.network = NetworkLed::G5;
        assert_eq!(scene(&s, &i, true, Overlay::None)[1], Out::Steady(255));
        i.data_up = false;
        assert_eq!(scene(&s, &i, true, Overlay::None)[..3], [Out::Off; 3]);
        i.network = NetworkLed::None;
        assert_eq!(scene(&s, &i, true, Overlay::None)[0], Out::Steady(255));
        assert_eq!(scene(&s, &i, false, Overlay::None), [Out::Off; 4], "dark past the timeout");
    }

    #[test]
    fn switches_and_brightness() {
        let s = LedSettings { network: false, wifi_brightness: 50, ..LedSettings::default() };
        let i = LedInputs { network: NetworkLed::G4, data_up: true, wifi_on: true, ..LedInputs::default() };
        assert_eq!(scene(&s, &i, true, Overlay::None), [Out::Off, Out::Off, Out::Off, Out::Steady(24)]);
        assert_eq!(dim(48, 1), 1);
        // the alarm shows on a lamp switched off, at full brightness
        assert_eq!(scene(&s, &i, false, Overlay::Siren(1))[..3], [Out::Off, Out::Steady(255), Out::Off]);
        assert_eq!(scene(&s, &i, true, Overlay::AlarmBlink)[0], Out::Blink { level: 255, ms: 200 });
        // the boot indicator does not
        assert_eq!(scene(&s, &i, true, Overlay::Boot(0))[..3], [Out::Off; 3]);
    }

    #[test]
    fn overlay_priority() {
        let mut s = LedSettings::default();
        assert_eq!(Overlay::of(&s, true, true, 2), Overlay::Siren(2));
        assert_eq!(Overlay::of(&s, false, true, 2), Overlay::Boot(2));
        s.thermal = ThermalLight::Off;
        assert_eq!(Overlay::of(&s, true, true, 2), Overlay::Boot(2));
        assert_eq!(Overlay::of(&s, true, false, 2), Overlay::None);
    }
}
