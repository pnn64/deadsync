use super::{ButtonLight, CabinetLight, Player, State};
use hidapi::{HidApi, HidDevice};
use log::{debug, warn};
use std::time::{Duration, Instant};

const VENDOR_ID: u16 = 0xbeef;
const PRODUCT_ID: u16 = 0x5730;
const LIGHTING_INTERFACE: i32 = 1;
const REOPEN_INTERVAL: Duration = Duration::from_millis(1500);

const REPORT_ID: u8 = 0;
const REPORT_SIZE: usize = 9;
const LIGHTS_OFFSET: usize = 2;
const BLUE_LED_OFFSET: usize = 6;
const KB_ENABLE_OFFSET: usize = 7;
const ON: u8 = 0xff;

const P1_MENU: u8 = 2;
const P2_MENU: u8 = 3;
const MARQUEE_P2_LOWER: u8 = 4;
const MARQUEE_P2_UPPER: u8 = 5;
const MARQUEE_P1_LOWER: u8 = 6;
const MARQUEE_P1_UPPER: u8 = 7;

const P1_UP: u8 = 8;
const P1_DOWN: u8 = 9;
const P1_LEFT: u8 = 10;
const P1_RIGHT: u8 = 11;
const P1_PAD_ENABLE: u8 = 12;

const P2_UP: u8 = 16;
const P2_DOWN: u8 = 17;
const P2_LEFT: u8 = 18;
const P2_RIGHT: u8 = 19;
const P2_PAD_ENABLE: u8 = 20;

const NEONS: u8 = 24;

pub struct Driver {
    api: Option<HidApi>,
    device: Option<HidDevice>,
    last_open_attempt: Option<Instant>,
    last_report: [u8; REPORT_SIZE],
    warned_missing: bool,
}

impl Driver {
    pub fn new() -> Self {
        Self {
            api: None,
            device: None,
            last_open_attempt: None,
            last_report: [u8::MAX; REPORT_SIZE],
            warned_missing: false,
        }
    }

    pub fn set(&mut self, state: &State) {
        let report = build_report(state);
        if report == self.last_report {
            return;
        }
        self.ensure_device();
        let Some(device) = self.device.as_ref() else {
            return;
        };
        match device.write(&report) {
            Ok(n) if n == REPORT_SIZE => {
                self.last_report = report;
            }
            Ok(n) => {
                warn!("MinimaidHID lights short write: wrote {n} of {REPORT_SIZE} bytes");
                self.drop_device();
            }
            Err(e) => {
                warn!("MinimaidHID lights write failed: {e}");
                self.drop_device();
            }
        }
    }

    fn ensure_device(&mut self) {
        if self.device.is_some() {
            return;
        }
        let now = Instant::now();
        if self
            .last_open_attempt
            .is_some_and(|last| now.duration_since(last) < REOPEN_INTERVAL)
        {
            return;
        }
        self.last_open_attempt = Some(now);
        if self.api.is_none() {
            match HidApi::new() {
                Ok(api) => self.api = Some(api),
                Err(e) => {
                    warn!("MinimaidHID lights hidapi init failed: {e}");
                    return;
                }
            }
        }
        let Some(api) = self.api.as_mut() else {
            return;
        };
        if let Err(e) = api.refresh_devices() {
            warn!("MinimaidHID lights hidapi refresh failed: {e}");
            self.api = None;
            return;
        }
        let exact = api.device_list().find(|info| {
            info.vendor_id() == VENDOR_ID
                && info.product_id() == PRODUCT_ID
                && info.interface_number() == LIGHTING_INTERFACE
        });
        let fallback = || {
            api.device_list().find(|info| {
                info.vendor_id() == VENDOR_ID
                    && info.product_id() == PRODUCT_ID
                    && info.interface_number() < 0
            })
        };
        let Some(info) = exact.or_else(fallback) else {
            if !self.warned_missing {
                debug!(
                    "No MinimaidHID lights device {:04x}:{:04x} interface {} found",
                    VENDOR_ID, PRODUCT_ID, LIGHTING_INTERFACE
                );
                self.warned_missing = true;
            }
            return;
        };
        match info.open_device(api) {
            Ok(device) => {
                debug!(
                    "Opened MinimaidHID lights device {:04x}:{:04x} interface {}",
                    info.vendor_id(),
                    info.product_id(),
                    info.interface_number()
                );
                self.warned_missing = false;
                self.device = Some(device);
            }
            Err(e) => {
                warn!("MinimaidHID lights open failed: {e}");
            }
        }
    }

    fn drop_device(&mut self) {
        self.device = None;
    }
}

fn build_report(state: &State) -> [u8; REPORT_SIZE] {
    let mut report = [0u8; REPORT_SIZE];
    let mut lights = 0u32;
    set_bit(
        &mut lights,
        MARQUEE_P1_LOWER,
        state.cabinet(CabinetLight::MarqueeLowerLeft),
    );
    set_bit(
        &mut lights,
        MARQUEE_P2_LOWER,
        state.cabinet(CabinetLight::MarqueeLowerRight),
    );
    set_bit(
        &mut lights,
        MARQUEE_P1_UPPER,
        state.cabinet(CabinetLight::MarqueeUpperLeft),
    );
    set_bit(
        &mut lights,
        MARQUEE_P2_UPPER,
        state.cabinet(CabinetLight::MarqueeUpperRight),
    );
    let neons = state.cabinet(CabinetLight::BassLeft) || state.cabinet(CabinetLight::BassRight);
    set_bit(&mut lights, NEONS, neons);

    set_bit(
        &mut lights,
        P1_MENU,
        state.button(Player::P1, ButtonLight::Start),
    );
    set_bit(
        &mut lights,
        P2_MENU,
        state.button(Player::P2, ButtonLight::Start),
    );
    set_player(&mut lights, Player::P1, state);
    set_player(&mut lights, Player::P2, state);
    set_bit(&mut lights, P1_PAD_ENABLE, true);
    set_bit(&mut lights, P2_PAD_ENABLE, true);

    report[0] = REPORT_ID;
    report[LIGHTS_OFFSET..LIGHTS_OFFSET + 4].copy_from_slice(&lights.to_le_bytes());
    report[BLUE_LED_OFFSET] = if neons { ON } else { 0 };
    report[KB_ENABLE_OFFSET] = 1;
    report
}

fn set_player(lights: &mut u32, player: Player, state: &State) {
    let (up, down, left, right) = match player {
        Player::P1 => (P1_UP, P1_DOWN, P1_LEFT, P1_RIGHT),
        Player::P2 => (P2_UP, P2_DOWN, P2_LEFT, P2_RIGHT),
    };
    set_bit(lights, up, state.button(player, ButtonLight::Up));
    set_bit(lights, down, state.button(player, ButtonLight::Down));
    set_bit(lights, left, state.button(player, ButtonLight::Left));
    set_bit(lights, right, state.button(player, ButtonLight::Right));
}

fn set_bit(bits: &mut u32, bit: u8, on: bool) {
    if on {
        *bits |= 1u32 << bit;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_maps_ddr_lights_to_minimaid_bits() {
        let mut state = State::default();
        state.set_cabinet(CabinetLight::MarqueeUpperLeft, true);
        state.set_cabinet(CabinetLight::MarqueeLowerRight, true);
        state.set_cabinet(CabinetLight::BassRight, true);
        state.set_button(Player::P1, ButtonLight::Start, true);
        state.set_button(Player::P1, ButtonLight::Up, true);
        state.set_button(Player::P1, ButtonLight::Left, true);
        state.set_button(Player::P2, ButtonLight::Down, true);
        state.set_button(Player::P2, ButtonLight::Right, true);

        let report = build_report(&state);
        let lights = u32::from_le_bytes(
            report[LIGHTS_OFFSET..LIGHTS_OFFSET + 4]
                .try_into()
                .expect("Minimaid light report stores exactly 4 light bytes"),
        );

        assert_eq!(report[0], REPORT_ID);
        assert_eq!(report[1], 0);
        assert_eq!(report[BLUE_LED_OFFSET], ON);
        assert_eq!(report[KB_ENABLE_OFFSET], 1);
        assert_eq!(report[8], 0);
        assert!(bit(lights, MARQUEE_P1_UPPER));
        assert!(bit(lights, MARQUEE_P2_LOWER));
        assert!(bit(lights, NEONS));
        assert!(bit(lights, P1_MENU));
        assert!(bit(lights, P1_UP));
        assert!(bit(lights, P1_LEFT));
        assert!(bit(lights, P2_DOWN));
        assert!(bit(lights, P2_RIGHT));
        assert!(bit(lights, P1_PAD_ENABLE));
        assert!(bit(lights, P2_PAD_ENABLE));
        assert!(!bit(lights, MARQUEE_P1_LOWER));
        assert!(!bit(lights, P2_MENU));
    }

    fn bit(bits: u32, bit: u8) -> bool {
        bits & (1u32 << bit) != 0
    }
}
