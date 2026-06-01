//! StepManiaX FSR sensor monitor using the shared SmxManager.

use crate::engine::input::fsr::{
    BackendKind, ButtonView, PAD_BUTTON_COUNT, PadDeviceId, PadView, SensorView,
};
use crate::engine::smx;
use rustmaniax_sdk::{ConfigFlags, SensorTestMode, SmxConfig};
use std::fmt::Write as _;
use std::time::SystemTime;

const PANEL_COUNT: usize = 9;
const PANEL_SENSOR_COUNT: usize = 4;

const MIN_FSR_THRESHOLD: u16 = 5;
const MAX_FSR_THRESHOLD: u16 = 250;

/// Panels exposed for config: (panel_index, label), in L/D/U/R order.
const VIEW_PANELS: [(usize, &str); PAD_BUTTON_COUNT] = [(3, "L"), (7, "D"), (1, "U"), (5, "R")];

pub struct Monitor {
    /// Whether the config screen has requested live reads (sensor test mode).
    read_active: bool,
}

impl Monitor {
    pub fn new() -> Self {
        Self { read_active: false }
    }

    /// Toggle live sensor reads (test mode) on all connected pads. Called by
    /// the config screen on enter/leave.
    pub fn set_active(&mut self, active: bool) {
        if self.read_active == active {
            return;
        }
        self.read_active = active;
        let mode = if active {
            SensorTestMode::CalibratedValues
        } else {
            SensorTestMode::Off
        };
        for pad in 0..2 {
            if smx::get_info(pad).connected {
                smx::set_test_mode(pad, mode);
            }
        }
    }

    /// Enumerate every connected FSR-capable StepManiaX pad as a `PadView`.
    pub fn poll_pads(&mut self) -> Vec<PadView> {
        let mut pads = Vec::new();
        for pad in 0..2 {
            let info = smx::get_info(pad);
            if !info.connected {
                continue;
            }
            let Some(config) = smx::get_config(pad) else {
                continue;
            };
            if !is_fsr(&config) {
                continue;
            }
            // Ensure test mode is on so sensor levels are populated.
            if !self.read_active {
                smx::set_test_mode(pad, SensorTestMode::CalibratedValues);
            }
            let test_data = smx::get_test_data(pad);
            let input_state = smx::manager().map_or(0, |m| m.get_input_state(pad));
            let device_name = format!(
                "StepManiaX P{} [{}]",
                if info.is_player2 { 2 } else { 1 },
                serial_prefix(&info.serial),
            );

            let buttons = std::array::from_fn(|i| {
                let (panel, label) = VIEW_PANELS[i];
                let settings = &config.panel_settings[panel];
                let sensors: Vec<SensorView> = (0..PANEL_SENSOR_COUNT)
                    .map(|s| {
                        let raw_value = test_data
                            .as_ref()
                            .filter(|d| d.have_data_from_panel[panel])
                            .map_or(0, |d| calibrate_fsr(d.sensor_level[panel][s]));
                        let raw_threshold = u16::from(settings.fsr_high_threshold[s]);
                        SensorView {
                            raw_value,
                            value_norm: normalize(raw_value, MAX_FSR_THRESHOLD),
                            raw_threshold,
                            threshold_norm: normalize(raw_threshold, MAX_FSR_THRESHOLD),
                            active: raw_value >= raw_threshold && raw_threshold > 0,
                        }
                    })
                    .collect();
                let aggregate_value = sensors.iter().map(|s| s.raw_value).max().unwrap_or(0);
                let aggregate_threshold =
                    sensors.iter().map(|s| s.raw_threshold).max().unwrap_or(0);
                ButtonView {
                    label,
                    sensors,
                    min_raw_threshold: MIN_FSR_THRESHOLD,
                    max_raw_threshold: MAX_FSR_THRESHOLD,
                    aggregate_value,
                    aggregate_threshold,
                    active: input_state & (1u16 << panel) != 0,
                }
            });

            pads.push(PadView {
                device_id: PadDeviceId {
                    backend: BackendKind::Smx,
                    index: pad,
                },
                device_name,
                is_player2: info.is_player2,
                buttons,
            });
        }
        pads
    }

    /// Set a panel threshold for one or all of its sensors. SMX stores both a
    /// low and high FSR threshold; we write `high = value`, `low = value - 1`.
    pub fn set_threshold(
        &mut self,
        device: PadDeviceId,
        button: usize,
        sensor: Option<usize>,
        value: u16,
    ) -> bool {
        if device.backend != BackendKind::Smx
            || button >= PAD_BUTTON_COUNT
            || !(MIN_FSR_THRESHOLD..=MAX_FSR_THRESHOLD).contains(&value)
        {
            return false;
        }
        let pad = device.index;
        let info = smx::get_info(pad);
        if !info.connected || info.firmware_version < 5 {
            return false;
        }
        let Some(mut config) = smx::get_config(pad) else {
            return false;
        };
        if !is_fsr(&config) {
            return false;
        }
        let (panel, _) = VIEW_PANELS[button];
        let high = value as u8;
        let low = high.saturating_sub(1);
        let settings = &mut config.panel_settings[panel];
        match sensor {
            Some(s) if s < PANEL_SENSOR_COUNT => {
                settings.fsr_high_threshold[s] = high;
                settings.fsr_low_threshold[s] = low;
            }
            Some(_) => return false,
            None => {
                for s in 0..PANEL_SENSOR_COUNT {
                    settings.fsr_high_threshold[s] = high;
                    settings.fsr_low_threshold[s] = low;
                }
            }
        }
        smx::set_config(pad, config);
        true
    }

    pub fn debug_dump(&mut self) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "DeadSync StepManiaX FSR debug dump");
        let _ = writeln!(out, "generated: {:?}", SystemTime::now());
        let _ = writeln!(out);

        for pad in 0..2 {
            let info = smx::get_info(pad);
            if !info.connected {
                let _ = writeln!(out, "Pad {pad}: not connected");
                continue;
            }
            let _ = writeln!(
                out,
                "Pad {pad}: P{} fw={} serial={}",
                if info.is_player2 { 2 } else { 1 },
                info.firmware_version,
                info.serial
            );
            if let Some(config) = smx::get_config(pad) {
                let _ = writeln!(out, "  is_fsr: {}", is_fsr(&config));
                for panel in 0..PANEL_COUNT {
                    let s = &config.panel_settings[panel];
                    let _ = writeln!(
                        out,
                        "  panel {panel} fsr_low: [{}, {}, {}, {}]",
                        s.fsr_low_threshold[0], s.fsr_low_threshold[1],
                        s.fsr_low_threshold[2], s.fsr_low_threshold[3]
                    );
                }
            }
            if let Some(data) = smx::get_test_data(pad) {
                for panel in 0..PANEL_COUNT {
                    if !data.have_data_from_panel[panel] {
                        let _ = writeln!(out, "  panel {panel} sensors: [no data]");
                        continue;
                    }
                    let _ = writeln!(
                        out,
                        "  panel {panel} sensors: [{}, {}, {}, {}]",
                        data.sensor_level[panel][0], data.sensor_level[panel][1],
                        data.sensor_level[panel][2], data.sensor_level[panel][3]
                    );
                }
            }
            let _ = writeln!(out);
        }
        out
    }

}

impl Drop for Monitor {
    fn drop(&mut self) {
        if self.read_active {
            for pad in 0..2 {
                if smx::get_info(pad).connected {
                    smx::set_test_mode(pad, SensorTestMode::Off);
                }
            }
        }
    }
}

/// First 4 hex chars of a serial, for compact pad labels (e.g. `40ea`).
fn serial_prefix(serial: &str) -> String {
    if serial.is_empty() {
        "?".to_owned()
    } else {
        serial.chars().take(4).collect()
    }
}

fn is_fsr(config: &SmxConfig) -> bool {
    config.master_version >= 4 && ConfigFlags::from_bits_truncate(config.flags).contains(ConfigFlags::FSR)
}

/// Scale a raw calibrated FSR sensor reading to the 0-250 range used by the
/// FSR thresholds, matching the official SMX config tool: clamp noise to zero,
/// then `value >> 2` (divide by 4).
fn calibrate_fsr(value: i16) -> u16 {
    if value <= 0 {
        return 0;
    }
    (value >> 2) as u16
}

fn normalize(value: u16, max: u16) -> f32 {
    if max == 0 {
        return 0.0;
    }
    (value as f32 / max as f32).clamp(0.0, 1.0)
}
