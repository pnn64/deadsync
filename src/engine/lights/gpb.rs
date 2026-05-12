use super::{ButtonLight, Player, State};
use hidapi::{HidApi, HidDevice};
use log::{debug, warn};
use std::time::{Duration, Instant};

const VENDOR_ID: u16 = 0x04d8;
const PRODUCT_ID: u16 = 0xea6a;
const LIGHTING_INTERFACE: i32 = 0x01;
const REOPEN_INTERVAL: Duration = Duration::from_millis(1500);

const REPORT_ID: u8 = 0x01;
const REPORT_SIZE: usize = 17;

const BTN01: usize = 1;
const BTN02: usize = 2;
const BTN03: usize = 3;
const BTN04: usize = 4;
const BTN05: usize = 5;
const BTN06: usize = 6;
const BTN07: usize = 7;
const BTN08: usize = 8;
const BTN09: usize = 9;
const BTN10: usize = 10;
const BTN11: usize = 11;
const BTN12: usize = 12;
const BTN13: usize = 13;
const BTN14: usize = 14;
const BTN15: usize = 15;
const BTN16: usize = 16;

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
                warn!("GPB lights short write: wrote {n} of {REPORT_SIZE} bytes");
                self.drop_device();
            }
            Err(e) => {
                warn!("GPB lights write failed: {e}");
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
                    warn!("GPB lights hidapi init failed: {e}");
                    return;
                }
            }
        }
        let Some(api) = self.api.as_mut() else {
            return;
        };
        if let Err(e) = api.refresh_devices() {
            warn!("GPB lights hidapi refresh failed: {e}");
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
                    "No GPB lights device {:04x}:{:04x} interface {} found",
                    VENDOR_ID, PRODUCT_ID, LIGHTING_INTERFACE
                );
                self.warned_missing = true;
            }
            return;
        };
        match info.open_device(api) {
            Ok(device) => {
                debug!(
                    "Opened GPB lights device {:04x}:{:04x} interface {}",
                    info.vendor_id(),
                    info.product_id(),
                    info.interface_number()
                );
                self.warned_missing = false;
                self.device = Some(device);
            }
            Err(e) => {
                warn!("GPB lights open failed: {e}");
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
        BTN01,
        state.button(Player::P1, ButtonLight::Select),
    );
    set(
        &mut report,
        BTN02,
        state.button(Player::P1, ButtonLight::Left),
    );
    set(
        &mut report,
        BTN03,
        state.button(Player::P1, ButtonLight::Right),
    );
    set(
        &mut report,
        BTN04,
        state.button(Player::P1, ButtonLight::Start),
    );

    set(
        &mut report,
        BTN05,
        state.button(Player::P2, ButtonLight::Select),
    );
    set(
        &mut report,
        BTN06,
        state.button(Player::P2, ButtonLight::Left),
    );
    set(
        &mut report,
        BTN07,
        state.button(Player::P2, ButtonLight::Right),
    );
    set(
        &mut report,
        BTN08,
        state.button(Player::P2, ButtonLight::Start),
    );

    set(
        &mut report,
        BTN09,
        state.button(Player::P1, ButtonLight::Up),
    );
    set(
        &mut report,
        BTN10,
        state.button(Player::P1, ButtonLight::Down),
    );
    set(
        &mut report,
        BTN11,
        state.button(Player::P2, ButtonLight::Up),
    );
    set(
        &mut report,
        BTN12,
        state.button(Player::P2, ButtonLight::Down),
    );
    for light in [BTN13, BTN14, BTN15, BTN16] {
        set(&mut report, light, false);
    }
    report
}

fn set(report: &mut [u8; REPORT_SIZE], light_index: usize, on: bool) {
    report[light_index] = u8::from(on);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_maps_buttons_to_gpb_order() {
        let mut state = State::default();
        state.set_button(Player::P1, ButtonLight::Select, true);
        state.set_button(Player::P1, ButtonLight::Right, true);
        state.set_button(Player::P1, ButtonLight::Up, true);
        state.set_button(Player::P2, ButtonLight::Left, true);
        state.set_button(Player::P2, ButtonLight::Start, true);
        state.set_button(Player::P2, ButtonLight::Down, true);

        let report = build_report(&state);
        assert_eq!(report[0], REPORT_ID);
        assert_eq!(report[BTN01], 1);
        assert_eq!(report[BTN02], 0);
        assert_eq!(report[BTN03], 1);
        assert_eq!(report[BTN04], 0);
        assert_eq!(report[BTN05], 0);
        assert_eq!(report[BTN06], 1);
        assert_eq!(report[BTN08], 1);
        assert_eq!(report[BTN09], 1);
        assert_eq!(report[BTN10], 0);
        assert_eq!(report[BTN11], 0);
        assert_eq!(report[BTN12], 1);
        assert_eq!(report[BTN13], 0);
        assert_eq!(report[BTN14], 0);
        assert_eq!(report[BTN15], 0);
        assert_eq!(report[BTN16], 0);
    }
}
