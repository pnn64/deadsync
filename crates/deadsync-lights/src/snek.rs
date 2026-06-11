use super::{ButtonLight, CabinetLight, Player, State};
use hidapi::{HidApi, HidDevice};
use log::{debug, warn};
use std::time::{Duration, Instant};

const VENDOR_ID: u16 = 0x2e8a;
const PRODUCT_ID: u16 = 0x10a8;
const LIGHTING_INTERFACE: i32 = 0x02;
const REOPEN_INTERVAL: Duration = Duration::from_millis(1500);

const REPORT_ID: u8 = 0x01;
const LIGHT_COUNT: usize = 32;
const REPORT_SIZE: usize = LIGHT_COUNT + 1;
const ON: u8 = 0xff;

const P1_START: usize = 2;
const P2_START: usize = 3;
const MARQUEE_LOWER_RIGHT: usize = 4;
const MARQUEE_UPPER_RIGHT: usize = 5;
const MARQUEE_LOWER_LEFT: usize = 6;
const MARQUEE_UPPER_LEFT: usize = 7;
const NEON: usize = 8;
const P1_UP: usize = 16;
const P1_DOWN: usize = 17;
const P1_LEFT: usize = 18;
const P1_RIGHT: usize = 19;
const P2_UP: usize = 24;
const P2_DOWN: usize = 25;
const P2_LEFT: usize = 26;
const P2_RIGHT: usize = 27;

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
            Ok(_) => {
                self.last_report = report;
            }
            Err(e) => {
                warn!("Snekboard lights write failed: {e}");
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
                    warn!("Snekboard lights hidapi init failed: {e}");
                    return;
                }
            }
        }
        let Some(api) = self.api.as_mut() else {
            return;
        };
        if let Err(e) = api.refresh_devices() {
            warn!("Snekboard lights hidapi refresh failed: {e}");
            self.api = None;
            return;
        }
        let exact = api.device_list().find(|info| {
            info.vendor_id() == VENDOR_ID
                && info.product_id() == PRODUCT_ID
                && info.interface_number() == LIGHTING_INTERFACE
        });
        let fallback = || {
            api.device_list()
                .find(|info| info.vendor_id() == VENDOR_ID && info.product_id() == PRODUCT_ID)
        };
        let Some(info) = exact.or_else(fallback) else {
            if !self.warned_missing {
                debug!("No Snekboard/Litboard lights device found");
                self.warned_missing = true;
            }
            return;
        };
        match info.open_device(api) {
            Ok(device) => {
                debug!(
                    "Opened Snekboard/Litboard lights device {:04x}:{:04x} interface {}",
                    info.vendor_id(),
                    info.product_id(),
                    info.interface_number()
                );
                self.warned_missing = false;
                self.device = Some(device);
            }
            Err(e) => {
                warn!("Snekboard lights open failed: {e}");
            }
        }
    }

    fn drop_device(&mut self) {
        self.device = None;
    }
}

fn build_report(state: &State) -> [u8; REPORT_SIZE] {
    let mut report = [0u8; REPORT_SIZE];
    report[0] = REPORT_ID;
    set(
        &mut report,
        MARQUEE_UPPER_LEFT,
        state.cabinet(CabinetLight::MarqueeUpperLeft),
    );
    set(
        &mut report,
        MARQUEE_UPPER_RIGHT,
        state.cabinet(CabinetLight::MarqueeUpperRight),
    );
    set(
        &mut report,
        MARQUEE_LOWER_LEFT,
        state.cabinet(CabinetLight::MarqueeLowerLeft),
    );
    set(
        &mut report,
        MARQUEE_LOWER_RIGHT,
        state.cabinet(CabinetLight::MarqueeLowerRight),
    );
    set(
        &mut report,
        NEON,
        state.cabinet(CabinetLight::BassLeft) || state.cabinet(CabinetLight::BassRight),
    );

    set(
        &mut report,
        P1_START,
        state.menu_button(Player::P1, ButtonLight::Start),
    );
    set(
        &mut report,
        P2_START,
        state.menu_button(Player::P2, ButtonLight::Start),
    );
    set(
        &mut report,
        P1_UP,
        state.button(Player::P1, ButtonLight::Up),
    );
    set(
        &mut report,
        P1_DOWN,
        state.button(Player::P1, ButtonLight::Down),
    );
    set(
        &mut report,
        P1_LEFT,
        state.button(Player::P1, ButtonLight::Left),
    );
    set(
        &mut report,
        P1_RIGHT,
        state.button(Player::P1, ButtonLight::Right),
    );
    set(
        &mut report,
        P2_UP,
        state.button(Player::P2, ButtonLight::Up),
    );
    set(
        &mut report,
        P2_DOWN,
        state.button(Player::P2, ButtonLight::Down),
    );
    set(
        &mut report,
        P2_LEFT,
        state.button(Player::P2, ButtonLight::Left),
    );
    set(
        &mut report,
        P2_RIGHT,
        state.button(Player::P2, ButtonLight::Right),
    );
    report
}

fn set(report: &mut [u8; REPORT_SIZE], light_index: usize, on: bool) {
    report[light_index + 1] = if on { ON } else { 0 };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_maps_dance_lights_to_snek_connectors() {
        let mut state = State::default();
        state.set_cabinet(CabinetLight::MarqueeUpperLeft, true);
        state.set_cabinet(CabinetLight::BassRight, true);
        state.set_button(Player::P1, ButtonLight::Left, true);
        state.set_menu_button(Player::P2, ButtonLight::Start, true);

        let report = build_report(&state);
        assert_eq!(report[0], REPORT_ID);
        assert_eq!(report[MARQUEE_UPPER_LEFT + 1], ON);
        assert_eq!(report[NEON + 1], ON);
        assert_eq!(report[P1_LEFT + 1], ON);
        assert_eq!(report[P2_START + 1], ON);
        assert_eq!(report[P1_RIGHT + 1], 0);
    }
}
