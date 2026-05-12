use super::{ButtonLight, CabinetLight, Player, State};
use hidapi::{HidApi, HidDevice};
use log::{debug, warn};
use std::time::{Duration, Instant};

const VENDOR_ID: u16 = 0x0547;
const PRODUCT_ID: u16 = 0x1337;
const REOPEN_INTERVAL: Duration = Duration::from_millis(1500);

const REPORT_ID: u8 = 0x02;
const REPORT_SIZE: usize = 18;
const ON: u8 = 0xff;

const P1_UL: usize = 1;
const P1_UR: usize = 2;
const P1_CN: usize = 3;
const P1_LL: usize = 4;
const P1_LR: usize = 5;
const P2_UL: usize = 6;
const P2_UR: usize = 7;
const P2_CN: usize = 8;
const P2_LL: usize = 9;
const P2_LR: usize = 10;
const NEON: usize = 11;
const MARQUEE_UPPER_LEFT: usize = 12;
const MARQUEE_UPPER_RIGHT: usize = 13;
const MARQUEE_LOWER_LEFT: usize = 14;
const MARQUEE_LOWER_RIGHT: usize = 15;
const COIN_PULSE: usize = 16;
const LED: usize = 17;

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
                warn!("Fusion lights short write: wrote {n} of {REPORT_SIZE} bytes");
                self.drop_device();
            }
            Err(e) => {
                warn!("Fusion lights write failed: {e}");
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
                    warn!("Fusion lights hidapi init failed: {e}");
                    return;
                }
            }
        }
        let Some(api) = self.api.as_mut() else {
            return;
        };
        if let Err(e) = api.refresh_devices() {
            warn!("Fusion lights hidapi refresh failed: {e}");
            self.api = None;
            return;
        }
        let Some(info) = api
            .device_list()
            .find(|info| info.vendor_id() == VENDOR_ID && info.product_id() == PRODUCT_ID)
        else {
            if !self.warned_missing {
                debug!(
                    "No Fusion lights device {:04x}:{:04x} found",
                    VENDOR_ID, PRODUCT_ID
                );
                self.warned_missing = true;
            }
            return;
        };
        match info.open_device(api) {
            Ok(device) => {
                debug!(
                    "Opened Fusion lights device {:04x}:{:04x} interface {}",
                    info.vendor_id(),
                    info.product_id(),
                    info.interface_number()
                );
                self.warned_missing = false;
                self.device = Some(device);
            }
            Err(e) => {
                warn!("Fusion lights open failed: {e}");
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

    let neon = state.cabinet(CabinetLight::BassLeft) || state.cabinet(CabinetLight::BassRight);
    set(&mut report, NEON, neon);
    set(&mut report, LED, neon);
    set(&mut report, COIN_PULSE, false);

    set_player(
        &mut report,
        Player::P1,
        P1_UL,
        P1_UR,
        P1_CN,
        P1_LL,
        P1_LR,
        state,
    );
    set_player(
        &mut report,
        Player::P2,
        P2_UL,
        P2_UR,
        P2_CN,
        P2_LL,
        P2_LR,
        state,
    );
    report
}

fn set_player(
    report: &mut [u8; REPORT_SIZE],
    player: Player,
    up_left: usize,
    up_right: usize,
    center: usize,
    lower_left: usize,
    lower_right: usize,
    state: &State,
) {
    set(report, up_left, state.button(player, ButtonLight::Up));
    set(report, up_right, state.button(player, ButtonLight::Down));
    set(report, center, state.button(player, ButtonLight::Left));
    set(report, lower_left, state.button(player, ButtonLight::Right));
    set(report, lower_right, false);
}

fn set(report: &mut [u8; REPORT_SIZE], light_index: usize, on: bool) {
    report[light_index] = if on { ON } else { 0 };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_maps_dance_lights_to_fusion_order() {
        let mut state = State::default();
        state.set_cabinet(CabinetLight::MarqueeUpperLeft, true);
        state.set_cabinet(CabinetLight::MarqueeLowerRight, true);
        state.set_cabinet(CabinetLight::BassLeft, true);
        state.set_button(Player::P1, ButtonLight::Up, true);
        state.set_button(Player::P1, ButtonLight::Left, true);
        state.set_button(Player::P1, ButtonLight::Start, true);
        state.set_button(Player::P2, ButtonLight::Down, true);
        state.set_button(Player::P2, ButtonLight::Right, true);

        let report = build_report(&state);
        assert_eq!(report[0], REPORT_ID);
        assert_eq!(report[MARQUEE_UPPER_LEFT], ON);
        assert_eq!(report[MARQUEE_LOWER_RIGHT], ON);
        assert_eq!(report[NEON], ON);
        assert_eq!(report[LED], ON);
        assert_eq!(report[P1_UL], ON);
        assert_eq!(report[P1_CN], ON);
        assert_eq!(report[P1_LL], 0);
        assert_eq!(report[P1_LR], 0);
        assert_eq!(report[P2_UR], ON);
        assert_eq!(report[P2_LL], ON);
        assert_eq!(report[P2_LR], 0);
        assert_eq!(report[COIN_PULSE], 0);
    }
}
