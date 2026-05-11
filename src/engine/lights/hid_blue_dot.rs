use super::{ButtonLight, CabinetLight, Player, State};
use hidapi::{HidApi, HidDevice};
use log::{debug, warn};
use std::time::{Duration, Instant};

const VENDOR_ID: u16 = 0x04bd;
const PRODUCT_ID: u16 = 0x00bd;
const REOPEN_INTERVAL: Duration = Duration::from_millis(1500);

const REPORT_SIZE: usize = 3;
const REPORT_ID: u8 = 0x00;
const CABINET_REPORT: u8 = 0x01;
const PAD_REPORT: u8 = 0x02;

const CAB_MARQUEE_UPPER_LEFT: u8 = 0;
const CAB_MARQUEE_UPPER_RIGHT: u8 = 1;
const CAB_MARQUEE_LOWER_LEFT: u8 = 2;
const CAB_MARQUEE_LOWER_RIGHT: u8 = 3;
const CAB_P1_START: u8 = 4;
const CAB_P2_START: u8 = 5;
const CAB_BASS: u8 = 6;

const PAD_P1_UP: u8 = 0;
const PAD_P1_DOWN: u8 = 1;
const PAD_P1_LEFT: u8 = 2;
const PAD_P1_RIGHT: u8 = 3;
const PAD_P2_UP: u8 = 4;
const PAD_P2_DOWN: u8 = 5;
const PAD_P2_LEFT: u8 = 6;
const PAD_P2_RIGHT: u8 = 7;

pub struct Driver {
    api: Option<HidApi>,
    device: Option<HidDevice>,
    last_open_attempt: Option<Instant>,
    last_cabinet_report: [u8; REPORT_SIZE],
    last_pad_report: [u8; REPORT_SIZE],
    warned_missing: bool,
}

impl Driver {
    pub fn new() -> Self {
        Self {
            api: None,
            device: None,
            last_open_attempt: None,
            last_cabinet_report: [u8::MAX; REPORT_SIZE],
            last_pad_report: [u8::MAX; REPORT_SIZE],
            warned_missing: false,
        }
    }

    pub fn set(&mut self, state: &State) {
        let cabinet = build_cabinet_report(state);
        let pad = build_pad_report(state);
        if cabinet == self.last_cabinet_report && pad == self.last_pad_report {
            return;
        }
        self.ensure_device();
        let Some(device) = self.device.as_ref() else {
            return;
        };
        if let Err(e) = device.write(&cabinet) {
            warn!("HidBlueDot cabinet lights write failed: {e}");
            self.drop_device();
            return;
        }
        if let Err(e) = device.write(&pad) {
            warn!("HidBlueDot pad lights write failed: {e}");
            self.drop_device();
            return;
        }
        self.last_cabinet_report = cabinet;
        self.last_pad_report = pad;
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
                    warn!("HidBlueDot lights hidapi init failed: {e}");
                    return;
                }
            }
        }
        let Some(api) = self.api.as_mut() else {
            return;
        };
        if let Err(e) = api.refresh_devices() {
            warn!("HidBlueDot lights hidapi refresh failed: {e}");
            self.api = None;
            return;
        }
        let Some(info) = api
            .device_list()
            .find(|info| info.vendor_id() == VENDOR_ID && info.product_id() == PRODUCT_ID)
        else {
            if !self.warned_missing {
                debug!("No HidBlueDot lights device found");
                self.warned_missing = true;
            }
            return;
        };
        match info.open_device(api) {
            Ok(device) => {
                debug!(
                    "Opened HidBlueDot lights device {:04x}:{:04x} interface {}",
                    info.vendor_id(),
                    info.product_id(),
                    info.interface_number()
                );
                self.warned_missing = false;
                self.device = Some(device);
            }
            Err(e) => {
                warn!("HidBlueDot lights open failed: {e}");
            }
        }
    }

    fn drop_device(&mut self) {
        self.device = None;
    }
}

fn build_cabinet_report(state: &State) -> [u8; REPORT_SIZE] {
    let mut bits = 0u8;
    set_bit(
        &mut bits,
        CAB_MARQUEE_UPPER_LEFT,
        state.cabinet(CabinetLight::MarqueeUpperLeft),
    );
    set_bit(
        &mut bits,
        CAB_MARQUEE_UPPER_RIGHT,
        state.cabinet(CabinetLight::MarqueeUpperRight),
    );
    set_bit(
        &mut bits,
        CAB_MARQUEE_LOWER_LEFT,
        state.cabinet(CabinetLight::MarqueeLowerLeft),
    );
    set_bit(
        &mut bits,
        CAB_MARQUEE_LOWER_RIGHT,
        state.cabinet(CabinetLight::MarqueeLowerRight),
    );
    set_bit(
        &mut bits,
        CAB_P1_START,
        state.button(Player::P1, ButtonLight::Start),
    );
    set_bit(
        &mut bits,
        CAB_P2_START,
        state.button(Player::P2, ButtonLight::Start),
    );
    set_bit(
        &mut bits,
        CAB_BASS,
        state.cabinet(CabinetLight::BassLeft) || state.cabinet(CabinetLight::BassRight),
    );
    [REPORT_ID, CABINET_REPORT, bits]
}

fn build_pad_report(state: &State) -> [u8; REPORT_SIZE] {
    let mut bits = 0u8;
    set_bit(
        &mut bits,
        PAD_P1_UP,
        state.button(Player::P1, ButtonLight::Up),
    );
    set_bit(
        &mut bits,
        PAD_P1_DOWN,
        state.button(Player::P1, ButtonLight::Down),
    );
    set_bit(
        &mut bits,
        PAD_P1_LEFT,
        state.button(Player::P1, ButtonLight::Left),
    );
    set_bit(
        &mut bits,
        PAD_P1_RIGHT,
        state.button(Player::P1, ButtonLight::Right),
    );
    set_bit(
        &mut bits,
        PAD_P2_UP,
        state.button(Player::P2, ButtonLight::Up),
    );
    set_bit(
        &mut bits,
        PAD_P2_DOWN,
        state.button(Player::P2, ButtonLight::Down),
    );
    set_bit(
        &mut bits,
        PAD_P2_LEFT,
        state.button(Player::P2, ButtonLight::Left),
    );
    set_bit(
        &mut bits,
        PAD_P2_RIGHT,
        state.button(Player::P2, ButtonLight::Right),
    );
    [REPORT_ID, PAD_REPORT, bits]
}

fn set_bit(bits: &mut u8, bit: u8, on: bool) {
    if on {
        *bits |= 1u8 << bit;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_use_hid_blue_dot_bit_order() {
        let mut state = State::default();
        state.set_cabinet(CabinetLight::MarqueeUpperLeft, true);
        state.set_cabinet(CabinetLight::BassRight, true);
        state.set_button(Player::P1, ButtonLight::Left, true);
        state.set_button(Player::P1, ButtonLight::Start, true);
        state.set_button(Player::P2, ButtonLight::Down, true);

        assert_eq!(
            build_cabinet_report(&state),
            [
                REPORT_ID,
                CABINET_REPORT,
                (1u8 << CAB_MARQUEE_UPPER_LEFT) | (1u8 << CAB_P1_START) | (1u8 << CAB_BASS)
            ]
        );
        assert_eq!(
            build_pad_report(&state),
            [
                REPORT_ID,
                PAD_REPORT,
                (1u8 << PAD_P1_LEFT) | (1u8 << PAD_P2_DOWN)
            ]
        );
    }
}
