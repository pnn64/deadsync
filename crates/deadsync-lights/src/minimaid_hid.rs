use super::State;
use hidapi::{HidApi, HidDevice};
use log::{debug, warn};
use std::time::{Duration, Instant};

const VENDOR_ID: u16 = 0xbeef;
const PRODUCT_ID: u16 = 0x5730;
const LIGHTING_INTERFACE: i32 = 1;
const REOPEN_INTERVAL: Duration = Duration::from_millis(1500);

const REPORT_ID: u8 = 0;
const REPORT_SIZE: usize = 9;
const ON: u8 = 0xff;

pub struct Driver {
    api: Option<HidApi>,
    device: Option<HidDevice>,
    last_open_attempt: Option<Instant>,
    last_report: [u8; REPORT_SIZE],
    warned_missing: bool,
}

impl Driver {
    pub const fn new() -> Self {
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
                    "No MinimaidHID lights device {VENDOR_ID:04x}:{PRODUCT_ID:04x} interface {LIGHTING_INTERFACE} found"
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
    let cabinet = state.cabinet;
    let neons = cabinet & 0x30 != 0;
    // Cabinet order is P2 lower, P2 upper, P1 lower, P1 upper in bits 4..7.
    let marquee = ((cabinet & 0x06) << 4) | ((cabinet & 0x01) << 7) | ((cabinet & 0x08) << 1);
    let menus = ((state.menu_buttons[0] & 0x10) >> 2) | ((state.menu_buttons[1] & 0x10) >> 1);
    let pads = u16::from_le_bytes(state.buttons);
    let pads = (pads & 0x0a0a) | ((pads & 0x0101) << 2) | ((pads & 0x0404) >> 2);
    [
        REPORT_ID,
        0,
        marquee | menus,
        (pads as u8) | 0x10,
        ((pads >> 8) as u8) | 0x10,
        neons as u8,
        if neons { ON } else { 0 },
        1,
        0,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ButtonLight, CabinetLight, Player};

    const LIGHTS_OFFSET: usize = 2;
    const BLUE_LED_OFFSET: usize = 6;
    const KB_ENABLE_OFFSET: usize = 7;
    const P1_MENU: u8 = 2;
    const P2_MENU: u8 = 3;
    const MARQUEE_P2_LOWER: u8 = 4;
    const MARQUEE_P1_LOWER: u8 = 6;
    const MARQUEE_P1_UPPER: u8 = 7;
    const P1_UP: u8 = 8;
    const P1_LEFT: u8 = 10;
    const P1_PAD_ENABLE: u8 = 12;
    const P2_DOWN: u8 = 17;
    const P2_RIGHT: u8 = 19;
    const P2_PAD_ENABLE: u8 = 20;
    const NEONS: u8 = 24;

    #[test]
    fn report_maps_ddr_lights_to_minimaid_bits() {
        let mut state = State::default();
        state.set_cabinet(CabinetLight::MarqueeUpperLeft, true);
        state.set_cabinet(CabinetLight::MarqueeLowerRight, true);
        state.set_cabinet(CabinetLight::BassRight, true);
        state.set_menu_button(Player::P1, ButtonLight::Start, true);
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

#[cfg(test)]
pub(super) fn performance_report(state: &State) -> [u8; REPORT_SIZE] {
    build_report(state)
}
