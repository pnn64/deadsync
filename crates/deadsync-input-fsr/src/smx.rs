//! StepManiaX FSR sensor monitor using the shared SmxManager.

use deadsync_input::fsr::{
    BackendKind, ButtonView, PAD_BUTTON_COUNT, PadDeviceId, PadView, SensorView, ThresholdKind,
};
use deadsync_smx::{self as smx, SensorTestData, SensorTestMode, SmxConfig};
use std::fmt::Write as _;
use std::time::SystemTime;

pub(super) const PANEL_SENSOR_COUNT: usize = 4;

pub(super) const MIN_FSR_THRESHOLD: u16 = 5;
pub(super) const MAX_FSR_THRESHOLD: u16 = 250;

/// Debounce edit bounds (microseconds). Firmware default is 4000us (4.0ms);
/// we never let the user drop below 0.5ms.
pub(super) const MIN_DEBOUNCE_US: u16 = 500;
pub(super) const MAX_DEBOUNCE_US: u16 = 25000;

// Pre-v5 load-cell pads: 8-bit thresholds (20-200) and raw live values that
// reach 500 (no >>2). We cap the display scale at 250 so the threshold band
// uses most of the bar's height (200 sits at 80%) instead of topping out
// midway; readings past 250 just show a full bar (far beyond "pressed").
pub(super) const MIN_LOADCELL_THRESHOLD: u16 = 20;
pub(super) const MAX_LOADCELL_THRESHOLD: u16 = 200;
pub(super) const FSR_VALUE_SCALE: u16 = 250;
pub(super) const LOADCELL_VALUE_SCALE: u16 = 250;

/// Panels exposed for config: (panel_index, label), in L/D/U/R order.
const VIEW_PANELS: [(usize, &str); PAD_BUTTON_COUNT] = [(3, "L"), (7, "D"), (1, "U"), (5, "R")];

/// Edge label for each of a panel's four FSR sensors, by firmware index.
pub(super) const SENSOR_EDGE_LABELS: [&str; PANEL_SENSOR_COUNT] = ["L", "R", "U", "D"];
/// Firmware-index order to display the sensors in so they read L, D, U, R.
pub(super) const SENSOR_DISPLAY_ORDER: [usize; PANEL_SENSOR_COUNT] = [0, 3, 2, 1];

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

    /// Enumerate every connected StepManiaX pad (FSR or load cell) as a `PadView`.
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
            let fsr = is_fsr(&config);
            // Ensure test mode is on so sensor levels are populated.
            if !self.read_active {
                smx::set_test_mode(pad, SensorTestMode::CalibratedValues);
            }
            let test_data = smx::get_test_data(pad);
            let input_state = smx::manager().map_or(0, |m| m.get_input_state(pad));
            // Player side is the slot (the SDK orders slot 0 = P1, slot 1 = P2
            // per the pad→player assignment), not the raw jumper bit.
            let device_name = format!(
                "StepManiaX P{} [{}]",
                if pad == 1 { 2 } else { 1 },
                smx::serial_prefix(&info.serial),
            );

            let buttons = std::array::from_fn(|i| {
                let (panel, label) = VIEW_PANELS[i];
                if fsr {
                    fsr_button(&config, panel, label, test_data.as_ref(), input_state)
                } else {
                    load_cell_button(&config, panel, label, test_data.as_ref(), input_state)
                }
            });

            // Read packed u16 fields by copy (no references into a packed struct).
            let max_tare = config.auto_calibration_max_tare;
            let debounce_us = config.panel_debounce_us;
            // Load-cell pads are Simple-only (no per-sensor config); they show
            // their four corner readings as separate bars but share one threshold.
            pads.push(PadView {
                device_id: PadDeviceId {
                    backend: BackendKind::Smx,
                    index: pad,
                },
                device_name,
                is_p2_side: pad == 1,
                buttons,
                supports_advanced: fsr,
                simple_per_sensor_bars: !fsr,
                supports_sensor_toggle: fsr,
                auto_recalibration: if fsr { Some(max_tare != 0) } else { None },
                debounce_micros: if fsr { Some(debounce_us) } else { None },
            });
        }
        pads
    }

    /// Set a panel threshold. FSR pads have one user-facing threshold (we write
    /// `high = value`, `low = value - 1`, so only `Press` is accepted) and allow
    /// per-sensor edits (`sensor = Some(i)`). Load-cell pads edit their press
    /// (high) and release (low) thresholds independently by `kind` (`sensor` is
    /// ignored); keeping release below press is the caller's job.
    pub fn set_threshold(
        &mut self,
        device: PadDeviceId,
        button: usize,
        sensor: Option<usize>,
        kind: ThresholdKind,
        value: u16,
    ) -> bool {
        if device.backend != BackendKind::Smx || button >= PAD_BUTTON_COUNT {
            return false;
        }
        let pad = device.index;
        let info = smx::get_info(pad);
        if !info.connected {
            log::trace!("SMX: set_threshold pad {pad} rejected (not connected)");
            return false;
        }
        let Some(mut config) = smx::get_config(pad) else {
            log::trace!("SMX: set_threshold pad {pad} rejected (config unavailable)");
            return false;
        };
        let (panel, _) = VIEW_PANELS[button];
        let fsr = is_fsr(&config);
        let settings = &mut config.panel_settings[panel];

        if fsr {
            if info.firmware_version < 5
                || kind != ThresholdKind::Press
                || !(MIN_FSR_THRESHOLD..=MAX_FSR_THRESHOLD).contains(&value)
            {
                log::trace!(
                    "SMX: set_threshold pad {pad} panel {panel} rejected (fsr, fw {}, kind {kind:?}, value {value} not in {MIN_FSR_THRESHOLD}..={MAX_FSR_THRESHOLD})",
                    info.firmware_version
                );
                return false;
            }
            let high = value as u8;
            let low = high.saturating_sub(1);
            match sensor {
                Some(s) if s < PANEL_SENSOR_COUNT => {
                    settings.fsr_high_threshold[s] = high;
                    settings.fsr_low_threshold[s] = low;
                }
                Some(s) => {
                    log::trace!("SMX: set_threshold pad {pad} rejected (sensor {s} out of range)");
                    return false;
                }
                None => {
                    for s in 0..PANEL_SENSOR_COUNT {
                        settings.fsr_high_threshold[s] = high;
                        settings.fsr_low_threshold[s] = low;
                    }
                }
            }
        } else {
            // Load cell: per-panel press (high) / release (low) pair; the
            // sensor index is ignored.
            if !(MIN_LOADCELL_THRESHOLD..=MAX_LOADCELL_THRESHOLD).contains(&value) {
                log::trace!(
                    "SMX: set_threshold pad {pad} panel {panel} rejected (load-cell, value {value} not in {MIN_LOADCELL_THRESHOLD}..={MAX_LOADCELL_THRESHOLD})"
                );
                return false;
            }
            match kind {
                ThresholdKind::Press => settings.load_cell_high_threshold = value as u8,
                ThresholdKind::Release => settings.load_cell_low_threshold = value as u8,
            }
        }
        smx::set_config(pad, config);
        log::trace!(
            "SMX: set_threshold pad {pad} panel {panel} sensor {sensor:?} {kind:?} -> {value} ({})",
            if fsr { "fsr" } else { "load-cell" }
        );
        true
    }

    /// Enable/disable one sensor of a panel via the `enabled_sensors` bitmask.
    pub fn set_sensor_enabled(
        &mut self,
        device: PadDeviceId,
        button: usize,
        sensor: usize,
        enabled: bool,
    ) -> bool {
        if device.backend != BackendKind::Smx
            || button >= PAD_BUTTON_COUNT
            || sensor >= PANEL_SENSOR_COUNT
        {
            return false;
        }
        let pad = device.index;
        let Some(mut config) = fsr_config_for_edit(pad, "set_sensor_enabled") else {
            return false;
        };
        let (panel, _) = VIEW_PANELS[button];
        let (byte, mask) = smx::enabled_bit(panel, sensor);
        if enabled {
            config.enabled_sensors[byte] |= mask;
        } else {
            config.enabled_sensors[byte] &= !mask;
        }
        smx::set_config(pad, config);
        log::trace!("SMX: set_sensor_enabled pad {pad} panel {panel} sensor {sensor} -> {enabled}");
        true
    }

    /// Turn auto-recalibration on (max tare `0xFFFF`) or off (max tare `0`),
    /// matching how other StepManiaX SDK forks toggle it.
    pub fn set_auto_recalibration(&mut self, device: PadDeviceId, enabled: bool) -> bool {
        if device.backend != BackendKind::Smx {
            return false;
        }
        let pad = device.index;
        let Some(mut config) = fsr_config_for_edit(pad, "set_auto_recalibration") else {
            return false;
        };
        config.auto_calibration_max_tare = if enabled { 0xFFFF } else { 0 };
        smx::set_config(pad, config);
        log::trace!("SMX: set_auto_recalibration pad {pad} -> {enabled}");
        true
    }

    /// Set the per-panel debounce time in microseconds.
    pub fn set_debounce_micros(&mut self, device: PadDeviceId, micros: u16) -> bool {
        if device.backend != BackendKind::Smx
            || !(MIN_DEBOUNCE_US..=MAX_DEBOUNCE_US).contains(&micros)
        {
            return false;
        }
        let pad = device.index;
        let Some(mut config) = fsr_config_for_edit(pad, "set_debounce_micros") else {
            return false;
        };
        config.panel_debounce_us = micros;
        smx::set_config(pad, config);
        log::trace!("SMX: set_debounce_micros pad {pad} -> {micros}us");
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
                "Pad {pad}: P{} (jumper P{}) fw={} serial={}",
                if pad == 1 { 2 } else { 1 },
                if info.is_player2 { 2 } else { 1 },
                info.firmware_version,
                info.serial
            );
            if let Some(config) = smx::get_config(pad) {
                let _ = writeln!(out, "  is_fsr: {}", is_fsr(&config));
                for panel in 0..smx::PANEL_COUNT {
                    let s = &config.panel_settings[panel];
                    let _ = writeln!(
                        out,
                        "  panel {panel} fsr_low: [{}, {}, {}, {}]",
                        s.fsr_low_threshold[0],
                        s.fsr_low_threshold[1],
                        s.fsr_low_threshold[2],
                        s.fsr_low_threshold[3]
                    );
                }
            }
            if let Some(data) = smx::get_test_data(pad) {
                for panel in 0..smx::PANEL_COUNT {
                    if !data.have_data_from_panel[panel] {
                        let _ = writeln!(out, "  panel {panel} sensors: [no data]");
                        continue;
                    }
                    let _ = writeln!(
                        out,
                        "  panel {panel} sensors: [{}, {}, {}, {}]",
                        data.sensor_level[panel][0],
                        data.sensor_level[panel][1],
                        data.sensor_level[panel][2],
                        data.sensor_level[panel][3]
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

/// Build an FSR panel's button view: four edge sensors (displayed L,D,U,R),
/// each with its own threshold and enable bit.
fn fsr_button(
    config: &SmxConfig,
    panel: usize,
    label: &'static str,
    test_data: Option<&SensorTestData>,
    input_state: u16,
) -> ButtonView {
    let settings = &config.panel_settings[panel];
    let sensors: Vec<SensorView> = SENSOR_DISPLAY_ORDER
        .iter()
        .map(|&s| {
            let raw_value = test_data
                .filter(|d| d.have_data_from_panel[panel])
                .map_or(0, |d| calibrate_fsr(d.sensor_level[panel][s]));
            let raw_threshold = u16::from(settings.fsr_high_threshold[s]);
            SensorView {
                firmware_index: s,
                label: Some(SENSOR_EDGE_LABELS[s]),
                raw_value,
                value_norm: normalize(raw_value, FSR_VALUE_SCALE),
                raw_threshold,
                threshold_norm: normalize(raw_threshold, FSR_VALUE_SCALE),
                active: raw_value >= raw_threshold && raw_threshold > 0,
                enabled: sensor_enabled(config, panel, s),
            }
        })
        .collect();
    let aggregate_value = sensors.iter().map(|s| s.raw_value).max().unwrap_or(0);
    let aggregate_threshold = sensors.iter().map(|s| s.raw_threshold).max().unwrap_or(0);
    ButtonView {
        label,
        sensors,
        min_raw_threshold: MIN_FSR_THRESHOLD,
        max_raw_threshold: MAX_FSR_THRESHOLD,
        aggregate_value,
        aggregate_threshold,
        active: input_state & (1u16 << panel) != 0,
        value_scale: FSR_VALUE_SCALE,
        release_threshold: None,
    }
}

/// Build a load-cell panel's button view: four corner readings (numbered 1-4)
/// that all share the panel's press/release threshold pair. The panel-level
/// `active` comes from the firmware's input state (its own hysteresis), which
/// is also what the Simple view colors the bars with; the per-sensor flag is
/// just the instantaneous press-threshold comparison.
fn load_cell_button(
    config: &SmxConfig,
    panel: usize,
    label: &'static str,
    test_data: Option<&SensorTestData>,
    input_state: u16,
) -> ButtonView {
    let settings = &config.panel_settings[panel];
    let threshold = u16::from(settings.load_cell_high_threshold);
    let sensors: Vec<SensorView> = (0..PANEL_SENSOR_COUNT)
        .map(|s| {
            let raw_value = test_data
                .filter(|d| d.have_data_from_panel[panel])
                .map_or(0, |d| calibrate_load_cell(d.sensor_level[panel][s]));
            SensorView {
                firmware_index: s,
                label: None, // corners, not edges -> numbered 1-4 in the UI
                raw_value,
                value_norm: normalize(raw_value, LOADCELL_VALUE_SCALE),
                raw_threshold: threshold,
                threshold_norm: normalize(threshold, LOADCELL_VALUE_SCALE),
                active: raw_value >= threshold && threshold > 0,
                enabled: true,
            }
        })
        .collect();
    let aggregate_value = sensors.iter().map(|s| s.raw_value).max().unwrap_or(0);
    ButtonView {
        label,
        sensors,
        min_raw_threshold: MIN_LOADCELL_THRESHOLD,
        max_raw_threshold: MAX_LOADCELL_THRESHOLD,
        aggregate_value,
        aggregate_threshold: threshold,
        active: input_state & (1u16 << panel) != 0,
        value_scale: LOADCELL_VALUE_SCALE,
        release_threshold: Some(u16::from(settings.load_cell_low_threshold)),
    }
}

fn sensor_enabled(config: &SmxConfig, panel: usize, sensor: usize) -> bool {
    let (byte, mask) = smx::enabled_bit(panel, sensor);
    config.enabled_sensors[byte] & mask != 0
}

/// Fetch a firmware-5+ FSR pad's config for an FSR-only edit (per-sensor toggle,
/// auto-recalibration, debounce). Returns `None` — logging why — for a
/// disconnected pad, pre-v5 firmware, an unavailable config, or a load-cell pad.
/// `op` names the calling edit for the log line.
fn fsr_config_for_edit(pad: usize, op: &str) -> Option<SmxConfig> {
    let info = smx::get_info(pad);
    if !info.connected || info.firmware_version < 5 {
        log::trace!(
            "SMX: {op} pad {pad} rejected (connected={}, fw {})",
            info.connected,
            info.firmware_version
        );
        return None;
    }
    let Some(config) = smx::get_config(pad) else {
        log::trace!("SMX: {op} pad {pad} rejected (config unavailable)");
        return None;
    };
    if !is_fsr(&config) {
        log::trace!("SMX: {op} pad {pad} rejected (load-cell pad)");
        return None;
    }
    Some(config)
}

fn is_fsr(config: &SmxConfig) -> bool {
    deadsync_smx::is_fsr(config)
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

/// Load-cell readings are used unscaled (no `>>2`), clamping noise at/below
/// zero to zero and capping at the display scale.
fn calibrate_load_cell(value: i16) -> u16 {
    if value <= 0 {
        return 0;
    }
    (value as u16).min(LOADCELL_VALUE_SCALE)
}

/// Sticky pressed state with the firmware's hysteresis: turns on at/above the
/// press (`high`) threshold and off only below the release (`low`) threshold;
/// in between it keeps its previous state.
pub(super) fn hysteresis_active(was: bool, value: u16, low: u16, high: u16) -> bool {
    if high == 0 {
        false
    } else if value >= high {
        true
    } else if value < low {
        false
    } else {
        was
    }
}

fn normalize(value: u16, max: u16) -> f32 {
    if max == 0 {
        return 0.0;
    }
    (value as f32 / max as f32).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::hysteresis_active;

    #[test]
    fn hysteresis_holds_between_release_and_press() {
        // Rising: stays off through the band, on at/above press.
        assert!(!hysteresis_active(false, 69, 70, 80));
        assert!(!hysteresis_active(false, 75, 70, 80));
        assert!(hysteresis_active(false, 80, 70, 80));
        // Falling: stays on through the band, off only below release.
        assert!(hysteresis_active(true, 79, 70, 80));
        assert!(hysteresis_active(true, 70, 70, 80));
        assert!(!hysteresis_active(true, 69, 70, 80));
        // An unset press threshold never reads as pressed.
        assert!(!hysteresis_active(true, 100, 0, 0));
    }
}
