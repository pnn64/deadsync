use super::{ButtonLight, Player, State};
use hidapi::{HidApi, HidDevice};
use log::{debug, warn};
use std::time::{Duration, Instant};

const VENDOR_ID: u16 = 0x2e8a;
const P1_PRODUCT_ID: u16 = 0x10d9;
const P2_PRODUCT_ID: u16 = 0x10e9;
const LIGHTING_INTERFACE: i32 = 0x02;
const REOPEN_INTERVAL: Duration = Duration::from_millis(1500);

const DEVICE_COUNT: usize = 2;
const REPORT_ID: u8 = 0x01;
const LIGHT_COUNT: usize = 8;
const REPORT_SIZE: usize = LIGHT_COUNT + 1;
const ON: u8 = 0xff;

const BTN1: usize = 0;
const BTN2: usize = 1;
const BTN3: usize = 2;
const BTN4: usize = 3;

pub struct Driver {
    api: Option<HidApi>,
    devices: [DeviceSlot; DEVICE_COUNT],
    last_open_attempt: Option<Instant>,
}

impl Driver {
    pub fn new() -> Self {
        Self {
            api: None,
            devices: [
                DeviceSlot::new("P1", P1_PRODUCT_ID),
                DeviceSlot::new("P2", P2_PRODUCT_ID),
            ],
            last_open_attempt: None,
        }
    }

    pub fn set(&mut self, state: &State) {
        let reports = [
            build_player_report(state, Player::P1),
            build_player_report(state, Player::P2),
        ];
        if self
            .devices
            .iter()
            .zip(reports)
            .all(|(slot, report)| slot.last_report == report)
        {
            return;
        }
        self.ensure_devices();
        for (slot, report) in self.devices.iter_mut().zip(reports) {
            slot.write(report);
        }
    }

    fn ensure_devices(&mut self) {
        if self.devices.iter().all(|slot| slot.device.is_some()) {
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
                    warn!("STAC2 lights hidapi init failed: {e}");
                    return;
                }
            }
        }
        let Some(api) = self.api.as_mut() else {
            return;
        };
        if let Err(e) = api.refresh_devices() {
            warn!("STAC2 lights hidapi refresh failed: {e}");
            self.api = None;
            return;
        }
        for slot in &mut self.devices {
            if slot.device.is_none() {
                slot.open(api);
            }
        }
    }
}

struct DeviceSlot {
    name: &'static str,
    product_id: u16,
    device: Option<HidDevice>,
    last_report: [u8; REPORT_SIZE],
    warned_missing: bool,
}

impl DeviceSlot {
    const fn new(name: &'static str, product_id: u16) -> Self {
        Self {
            name,
            product_id,
            device: None,
            last_report: [u8::MAX; REPORT_SIZE],
            warned_missing: false,
        }
    }

    fn open(&mut self, api: &HidApi) {
        let exact = api.device_list().find(|info| {
            info.vendor_id() == VENDOR_ID
                && info.product_id() == self.product_id
                && info.interface_number() == LIGHTING_INTERFACE
        });
        let fallback = || {
            api.device_list().find(|info| {
                info.vendor_id() == VENDOR_ID
                    && info.product_id() == self.product_id
                    && info.interface_number() < 0
            })
        };
        let Some(info) = exact.or_else(fallback) else {
            if !self.warned_missing {
                debug!(
                    "No STAC2 {} lights device {:04x}:{:04x} interface {} found",
                    self.name, VENDOR_ID, self.product_id, LIGHTING_INTERFACE
                );
                self.warned_missing = true;
            }
            return;
        };
        match info.open_device(api) {
            Ok(device) => {
                debug!(
                    "Opened STAC2 {} lights device {:04x}:{:04x} interface {}",
                    self.name,
                    info.vendor_id(),
                    info.product_id(),
                    info.interface_number()
                );
                self.warned_missing = false;
                self.device = Some(device);
            }
            Err(e) => {
                warn!("STAC2 {} lights open failed: {e}", self.name);
            }
        }
    }

    fn write(&mut self, report: [u8; REPORT_SIZE]) {
        if report == self.last_report {
            return;
        }
        let Some(device) = self.device.as_ref() else {
            return;
        };
        match device.write(&report) {
            Ok(n) if n == REPORT_SIZE => {
                self.last_report = report;
            }
            Ok(n) => {
                warn!(
                    "STAC2 {} lights short write: wrote {n} of {REPORT_SIZE} bytes",
                    self.name
                );
                self.drop_device();
            }
            Err(e) => {
                warn!("STAC2 {} lights write failed: {e}", self.name);
                self.drop_device();
            }
        }
    }

    fn drop_device(&mut self) {
        self.device = None;
    }
}

fn build_player_report(state: &State, player: Player) -> [u8; REPORT_SIZE] {
    let mut report = [0u8; REPORT_SIZE];
    report[0] = REPORT_ID;
    set(&mut report, BTN1, state.button(player, ButtonLight::Up));
    set(&mut report, BTN2, state.button(player, ButtonLight::Down));
    set(&mut report, BTN3, state.button(player, ButtonLight::Left));
    set(&mut report, BTN4, state.button(player, ButtonLight::Right));
    report
}

fn set(report: &mut [u8; REPORT_SIZE], light_index: usize, on: bool) {
    report[light_index + 1] = if on { ON } else { 0 };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_maps_dance_buttons_to_stac2_order() {
        let mut state = State::default();
        state.set_button(Player::P1, ButtonLight::Up, true);
        state.set_button(Player::P1, ButtonLight::Left, true);
        state.set_button(Player::P1, ButtonLight::Start, true);
        state.set_button(Player::P2, ButtonLight::Down, true);
        state.set_button(Player::P2, ButtonLight::Right, true);

        let p1 = build_player_report(&state, Player::P1);
        assert_eq!(p1[0], REPORT_ID);
        assert_eq!(p1[BTN1 + 1], ON);
        assert_eq!(p1[BTN2 + 1], 0);
        assert_eq!(p1[BTN3 + 1], ON);
        assert_eq!(p1[BTN4 + 1], 0);
        assert!(p1[BTN4 + 2..].iter().all(|v| *v == 0));

        let p2 = build_player_report(&state, Player::P2);
        assert_eq!(p2[0], REPORT_ID);
        assert_eq!(p2[BTN1 + 1], 0);
        assert_eq!(p2[BTN2 + 1], ON);
        assert_eq!(p2[BTN3 + 1], 0);
        assert_eq!(p2[BTN4 + 1], ON);
        assert!(p2[BTN4 + 2..].iter().all(|v| *v == 0));
    }
}
