use super::{ButtonLight, CabinetLight, PacDriveLightOrdering, Player, State};
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
    ordering: PacDriveLightOrdering,
    last_open_attempt: Option<Instant>,
    last_report: [u8; REPORT_SIZE],
    warned_missing: bool,
}

impl Driver {
    pub fn new(ordering: PacDriveLightOrdering) -> Self {
        Self {
            api: None,
            device: None,
            ordering,
            last_open_attempt: None,
            last_report: [u8::MAX; REPORT_SIZE],
            warned_missing: false,
        }
    }

    pub fn set(&mut self, state: &State) {
        let report = build_report(state, self.ordering);
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

fn build_report(state: &State, ordering: PacDriveLightOrdering) -> [u8; REPORT_SIZE] {
    let mut report = [0u8; REPORT_SIZE];
    report[0] = REPORT_ID;

    match ordering {
        PacDriveLightOrdering::OpenItg => build_openitg_report(state, &mut report),
        PacDriveLightOrdering::Sm5 => build_sm5_report(state, &mut report),
    }
    report
}

fn build_openitg_report(state: &State, report: &mut [u8; REPORT_SIZE]) {
    set_led(report, LED01, state.button(Player::P1, ButtonLight::Left));
    set_led(report, LED02, state.button(Player::P1, ButtonLight::Right));
    set_led(report, LED03, state.button(Player::P1, ButtonLight::Up));
    set_led(report, LED04, state.button(Player::P1, ButtonLight::Down));
    set_led(report, LED05, state.button(Player::P2, ButtonLight::Left));
    set_led(report, LED06, state.button(Player::P2, ButtonLight::Right));
    set_led(report, LED07, state.button(Player::P2, ButtonLight::Up));
    set_led(report, LED08, state.button(Player::P2, ButtonLight::Down));
    set_led(report, LED09, state.cabinet(CabinetLight::MarqueeUpperLeft));
    set_led(
        report,
        LED10,
        state.cabinet(CabinetLight::MarqueeUpperRight),
    );
    set_led(report, LED11, state.cabinet(CabinetLight::MarqueeLowerLeft));
    set_led(
        report,
        LED12,
        state.cabinet(CabinetLight::MarqueeLowerRight),
    );
    set_led(
        report,
        LED13,
        state.menu_button(Player::P1, ButtonLight::Start),
    );
    set_led(
        report,
        LED14,
        state.menu_button(Player::P2, ButtonLight::Start),
    );
    let bass = state.cabinet(CabinetLight::BassLeft) || state.cabinet(CabinetLight::BassRight);
    set_led(report, LED15, bass);
    set_led(report, LED16, bass);
}

fn build_sm5_report(state: &State, report: &mut [u8; REPORT_SIZE]) {
    set_led(report, LED01, state.cabinet(CabinetLight::MarqueeUpperLeft));
    set_led(
        report,
        LED02,
        state.cabinet(CabinetLight::MarqueeUpperRight),
    );
    set_led(report, LED03, state.cabinet(CabinetLight::MarqueeLowerLeft));
    set_led(
        report,
        LED04,
        state.cabinet(CabinetLight::MarqueeLowerRight),
    );
    let bass = state.cabinet(CabinetLight::BassLeft) || state.cabinet(CabinetLight::BassRight);
    set_led(report, LED05, bass);

    set_led(report, LED06, state.button(Player::P1, ButtonLight::Left));
    set_led(report, LED07, state.button(Player::P1, ButtonLight::Right));
    set_led(report, LED08, state.button(Player::P1, ButtonLight::Up));
    set_led(report, LED09, state.button(Player::P1, ButtonLight::Down));
    set_led(
        report,
        LED10,
        state.menu_button(Player::P1, ButtonLight::Start),
    );

    set_led(report, LED11, state.button(Player::P2, ButtonLight::Left));
    set_led(report, LED12, state.button(Player::P2, ButtonLight::Right));
    set_led(report, LED13, state.button(Player::P2, ButtonLight::Up));
    set_led(report, LED14, state.button(Player::P2, ButtonLight::Down));
    set_led(
        report,
        LED15,
        state.menu_button(Player::P2, ButtonLight::Start),
    );
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

    fn expected_report(leds: &[u8]) -> [u8; REPORT_SIZE] {
        let mut report = [0; REPORT_SIZE];
        for led in leds {
            set_led(&mut report, *led, true);
        }
        report
    }

    fn assert_light_led(
        ordering: PacDriveLightOrdering,
        configure: impl FnOnce(&mut State),
        leds: &[u8],
    ) {
        let mut state = State::default();
        configure(&mut state);
        assert_eq!(build_report(&state, ordering), expected_report(leds));
    }

    #[test]
    fn report_uses_openitg_pacdrive_order() {
        let ordering = PacDriveLightOrdering::OpenItg;
        assert_light_led(
            ordering,
            |s| s.set_button(Player::P1, ButtonLight::Left, true),
            &[LED01],
        );
        assert_light_led(
            ordering,
            |s| s.set_button(Player::P1, ButtonLight::Right, true),
            &[LED02],
        );
        assert_light_led(
            ordering,
            |s| s.set_button(Player::P1, ButtonLight::Up, true),
            &[LED03],
        );
        assert_light_led(
            ordering,
            |s| s.set_button(Player::P1, ButtonLight::Down, true),
            &[LED04],
        );
        assert_light_led(
            ordering,
            |s| s.set_button(Player::P2, ButtonLight::Left, true),
            &[LED05],
        );
        assert_light_led(
            ordering,
            |s| s.set_button(Player::P2, ButtonLight::Right, true),
            &[LED06],
        );
        assert_light_led(
            ordering,
            |s| s.set_button(Player::P2, ButtonLight::Up, true),
            &[LED07],
        );
        assert_light_led(
            ordering,
            |s| s.set_button(Player::P2, ButtonLight::Down, true),
            &[LED08],
        );
        assert_light_led(
            ordering,
            |s| s.set_cabinet(CabinetLight::MarqueeUpperLeft, true),
            &[LED09],
        );
        assert_light_led(
            ordering,
            |s| s.set_cabinet(CabinetLight::MarqueeUpperRight, true),
            &[LED10],
        );
        assert_light_led(
            ordering,
            |s| s.set_cabinet(CabinetLight::MarqueeLowerLeft, true),
            &[LED11],
        );
        assert_light_led(
            ordering,
            |s| s.set_cabinet(CabinetLight::MarqueeLowerRight, true),
            &[LED12],
        );
        assert_light_led(
            ordering,
            |s| s.set_menu_button(Player::P1, ButtonLight::Start, true),
            &[LED13],
        );
        assert_light_led(
            ordering,
            |s| s.set_menu_button(Player::P2, ButtonLight::Start, true),
            &[LED14],
        );
        assert_light_led(
            ordering,
            |s| s.set_cabinet(CabinetLight::BassLeft, true),
            &[LED15, LED16],
        );
        assert_light_led(
            ordering,
            |s| s.set_cabinet(CabinetLight::BassRight, true),
            &[LED15, LED16],
        );
        assert_light_led(
            ordering,
            |s| s.set_any_button(Player::P1, ButtonLight::Select, true),
            &[],
        );
        assert_light_led(
            ordering,
            |s| s.set_any_button(Player::P2, ButtonLight::Select, true),
            &[],
        );
    }

    #[test]
    fn report_retains_sm5_pacdrive_order() {
        let ordering = PacDriveLightOrdering::Sm5;
        assert_light_led(
            ordering,
            |s| s.set_cabinet(CabinetLight::MarqueeUpperLeft, true),
            &[LED01],
        );
        assert_light_led(
            ordering,
            |s| s.set_cabinet(CabinetLight::MarqueeUpperRight, true),
            &[LED02],
        );
        assert_light_led(
            ordering,
            |s| s.set_cabinet(CabinetLight::MarqueeLowerLeft, true),
            &[LED03],
        );
        assert_light_led(
            ordering,
            |s| s.set_cabinet(CabinetLight::MarqueeLowerRight, true),
            &[LED04],
        );
        assert_light_led(
            ordering,
            |s| s.set_cabinet(CabinetLight::BassLeft, true),
            &[LED05],
        );
        assert_light_led(
            ordering,
            |s| s.set_cabinet(CabinetLight::BassRight, true),
            &[LED05],
        );
        assert_light_led(
            ordering,
            |s| s.set_button(Player::P1, ButtonLight::Left, true),
            &[LED06],
        );
        assert_light_led(
            ordering,
            |s| s.set_button(Player::P1, ButtonLight::Right, true),
            &[LED07],
        );
        assert_light_led(
            ordering,
            |s| s.set_button(Player::P1, ButtonLight::Up, true),
            &[LED08],
        );
        assert_light_led(
            ordering,
            |s| s.set_button(Player::P1, ButtonLight::Down, true),
            &[LED09],
        );
        assert_light_led(
            ordering,
            |s| s.set_menu_button(Player::P1, ButtonLight::Start, true),
            &[LED10],
        );
        assert_light_led(
            ordering,
            |s| s.set_button(Player::P2, ButtonLight::Left, true),
            &[LED11],
        );
        assert_light_led(
            ordering,
            |s| s.set_button(Player::P2, ButtonLight::Right, true),
            &[LED12],
        );
        assert_light_led(
            ordering,
            |s| s.set_button(Player::P2, ButtonLight::Up, true),
            &[LED13],
        );
        assert_light_led(
            ordering,
            |s| s.set_button(Player::P2, ButtonLight::Down, true),
            &[LED14],
        );
        assert_light_led(
            ordering,
            |s| s.set_menu_button(Player::P2, ButtonLight::Start, true),
            &[LED15],
        );
        assert_light_led(
            ordering,
            |s| s.set_any_button(Player::P1, ButtonLight::Select, true),
            &[],
        );
        assert_light_led(
            ordering,
            |s| s.set_any_button(Player::P2, ButtonLight::Select, true),
            &[],
        );
    }

    #[test]
    fn report_byte_swaps_physical_led_halves_like_itgmania() {
        assert_eq!(expected_report(&[LED01]), [0, 0, 0, 0, 0b0000_0001]);
        assert_eq!(expected_report(&[LED08]), [0, 0, 0, 0, 0b1000_0000]);
        assert_eq!(expected_report(&[LED09]), [0, 0, 0, 0b0000_0001, 0]);
        assert_eq!(expected_report(&[LED16]), [0, 0, 0, 0b1000_0000, 0]);
    }

    #[test]
    fn pid_range_matches_pacdrive_devices() {
        assert!(!pac_drive_pid(0x14ff));
        assert!(pac_drive_pid(0x1500));
        assert!(pac_drive_pid(0x1507));
        assert!(!pac_drive_pid(0x1508));
    }
}
