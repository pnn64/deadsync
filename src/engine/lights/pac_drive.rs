use super::{ButtonLight, CabinetLight, Player, State};
use hidapi::{HidApi, HidDevice};
use log::{debug, warn};
use std::time::{Duration, Instant};

const VENDOR_ID: u16 = 0xd209;
const PRODUCT_ID_BASE: u16 = 0x1500;
const PRODUCT_ID_COUNT: u16 = 8;
const LIGHTING_INTERFACE: i32 = 0;
const REOPEN_INTERVAL: Duration = Duration::from_millis(1500);

const REPORT_ID: u8 = 0x00;
const REPORT_SIZE: usize = 5;
const LED_LOW_BYTE: usize = 3;
const LED_HIGH_BYTE: usize = 4;

const LED01: u8 = 1;
const LED02: u8 = 2;
const LED03: u8 = 3;
const LED04: u8 = 4;
const LED05: u8 = 5;
const LED06: u8 = 6;
const LED07: u8 = 7;
const LED08: u8 = 8;
const LED09: u8 = 9;
const LED10: u8 = 10;
const LED11: u8 = 11;
const LED12: u8 = 12;
const LED13: u8 = 13;
const LED14: u8 = 14;
const LED15: u8 = 15;
const LED16: u8 = 16;

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
                warn!("PacDrive lights short write: wrote {n} of {REPORT_SIZE} bytes");
                self.drop_device();
            }
            Err(e) => {
                warn!("PacDrive lights write failed: {e}");
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
                    warn!("PacDrive lights hidapi init failed: {e}");
                    return;
                }
            }
        }
        let Some(api) = self.api.as_mut() else {
            return;
        };
        if let Err(e) = api.refresh_devices() {
            warn!("PacDrive lights hidapi refresh failed: {e}");
            self.api = None;
            return;
        }
        let exact = api.device_list().find(|info| {
            info.vendor_id() == VENDOR_ID
                && pac_drive_pid(info.product_id())
                && info.interface_number() == LIGHTING_INTERFACE
        });
        let fallback = || {
            api.device_list().find(|info| {
                info.vendor_id() == VENDOR_ID
                    && pac_drive_pid(info.product_id())
                    && info.interface_number() < 0
            })
        };
        let Some(info) = exact.or_else(fallback) else {
            if !self.warned_missing {
                debug!(
                    "No PacDrive lights device {:04x}:{} interface {} found",
                    VENDOR_ID,
                    product_range_text(),
                    LIGHTING_INTERFACE
                );
                self.warned_missing = true;
            }
            return;
        };
        match info.open_device(api) {
            Ok(device) => {
                debug!(
                    "Opened PacDrive lights device {:04x}:{:04x} interface {}",
                    info.vendor_id(),
                    info.product_id(),
                    info.interface_number()
                );
                self.warned_missing = false;
                self.device = Some(device);
            }
            Err(e) => {
                warn!("PacDrive lights open failed: {e}");
            }
        }
    }

    fn drop_device(&mut self) {
        self.device = None;
    }
}

const fn pac_drive_pid(pid: u16) -> bool {
    pid >= PRODUCT_ID_BASE && pid < PRODUCT_ID_BASE + PRODUCT_ID_COUNT
}

const fn product_range_text() -> &'static str {
    "1500-1507"
}

fn build_report(state: &State) -> [u8; REPORT_SIZE] {
    let mut report = [0u8; REPORT_SIZE];
    report[0] = REPORT_ID;

    set_led(
        &mut report,
        LED01,
        state.cabinet(CabinetLight::MarqueeUpperLeft),
    );
    set_led(
        &mut report,
        LED02,
        state.cabinet(CabinetLight::MarqueeUpperRight),
    );
    set_led(
        &mut report,
        LED03,
        state.cabinet(CabinetLight::MarqueeLowerLeft),
    );
    set_led(
        &mut report,
        LED04,
        state.cabinet(CabinetLight::MarqueeLowerRight),
    );
    let bass = state.cabinet(CabinetLight::BassLeft) || state.cabinet(CabinetLight::BassRight);
    set_led(&mut report, LED05, bass);

    set_led(
        &mut report,
        LED06,
        state.button(Player::P1, ButtonLight::Left),
    );
    set_led(
        &mut report,
        LED07,
        state.button(Player::P1, ButtonLight::Right),
    );
    set_led(
        &mut report,
        LED08,
        state.button(Player::P1, ButtonLight::Up),
    );
    set_led(
        &mut report,
        LED09,
        state.button(Player::P1, ButtonLight::Down),
    );
    set_led(
        &mut report,
        LED10,
        state.button(Player::P1, ButtonLight::Start),
    );

    set_led(
        &mut report,
        LED11,
        state.button(Player::P2, ButtonLight::Left),
    );
    set_led(
        &mut report,
        LED12,
        state.button(Player::P2, ButtonLight::Right),
    );
    set_led(
        &mut report,
        LED13,
        state.button(Player::P2, ButtonLight::Up),
    );
    set_led(
        &mut report,
        LED14,
        state.button(Player::P2, ButtonLight::Down),
    );
    set_led(
        &mut report,
        LED15,
        state.button(Player::P2, ButtonLight::Start),
    );
    set_led(&mut report, LED16, false);
    report
}

fn set_led(report: &mut [u8; REPORT_SIZE], led: u8, on: bool) {
    if !on {
        return;
    }
    if led >= LED09 {
        report[LED_LOW_BYTE] |= 1u8 << (led - LED09);
    } else {
        report[LED_HIGH_BYTE] |= 1u8 << (led - LED01);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_uses_sm5_pacdrive_order() {
        let mut state = State::default();
        state.set_button(Player::P1, ButtonLight::Left, true);
        state.set_button(Player::P1, ButtonLight::Up, true);
        state.set_button(Player::P1, ButtonLight::Start, true);
        state.set_button(Player::P2, ButtonLight::Right, true);
        state.set_button(Player::P2, ButtonLight::Down, true);
        state.set_cabinet(CabinetLight::MarqueeUpperLeft, true);
        state.set_cabinet(CabinetLight::MarqueeLowerRight, true);
        state.set_cabinet(CabinetLight::BassRight, true);

        let report = build_report(&state);
        assert_eq!(report[0], REPORT_ID);
        assert_eq!(report[1], 0);
        assert_eq!(report[2], 0);
        assert_eq!(
            report[LED_HIGH_BYTE],
            (1u8 << (LED01 - LED01))
                | (1u8 << (LED04 - LED01))
                | (1u8 << (LED05 - LED01))
                | (1u8 << (LED06 - LED01))
                | (1u8 << (LED08 - LED01))
        );
        assert_eq!(
            report[LED_LOW_BYTE],
            (1u8 << (LED10 - LED09)) | (1u8 << (LED12 - LED09)) | (1u8 << (LED14 - LED09))
        );
    }

    #[test]
    fn pid_range_matches_pacdrive_devices() {
        assert!(!pac_drive_pid(0x14ff));
        assert!(pac_drive_pid(0x1500));
        assert!(pac_drive_pid(0x1507));
        assert!(!pac_drive_pid(0x1508));
    }
}
