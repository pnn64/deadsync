//! Synthetic SMX pads for UI development, enabled with `DEADSYNC_MOCK_PADS`.
//!
//! Set the variable to a comma-separated list of pad kinds, e.g.
//! `DEADSYNC_MOCK_PADS=loadcell` or `DEADSYNC_MOCK_PADS=loadcell,fsr` (any
//! other non-empty value means one load-cell pad). Each kind becomes one fake
//! pad on the Configure Pads screen, with animated sensor readings and
//! thresholds edited in memory, so the whole editor can be exercised without
//! hardware. While the variable is set, `engine::smx::init` refuses to start
//! the SDK, so native SMX is fully off and nothing is written over USB.

use super::smx::{
    FSR_VALUE_SCALE, LOADCELL_VALUE_SCALE, MAX_DEBOUNCE_US, MAX_FSR_THRESHOLD,
    MAX_LOADCELL_THRESHOLD, MIN_DEBOUNCE_US, MIN_FSR_THRESHOLD, MIN_LOADCELL_THRESHOLD,
    PANEL_SENSOR_COUNT, SENSOR_DISPLAY_ORDER, SENSOR_EDGE_LABELS,
};
use deadsync_input::fsr::{
    BackendKind, ButtonView, PAD_BUTTON_COUNT, PAD_BUTTON_LABELS, PadDeviceId, PadView, SensorView,
    ThresholdKind,
};
use std::time::Instant;

/// Starting thresholds: the Low preset's load-cell pair and Medium's FSR value.
const INIT_LOADCELL_PRESS: u16 = 80;
const INIT_LOADCELL_RELEASE: u16 = 70;
const INIT_FSR_THRESHOLD: u16 = 175;
const INIT_DEBOUNCE_US: u16 = 4000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MockKind {
    LoadCell,
    Fsr,
}

struct MockPad {
    kind: MockKind,
    /// Per-button per-sensor press (high) thresholds. Load-cell buttons keep
    /// all four in lockstep (one threshold per panel).
    press: [[u16; PANEL_SENSOR_COUNT]; PAD_BUTTON_COUNT],
    /// Per-button release (low) thresholds (load cell only).
    release: [u16; PAD_BUTTON_COUNT],
    /// Per-sensor enable flags (FSR only).
    enabled: [[bool; PANEL_SENSOR_COUNT]; PAD_BUTTON_COUNT],
    auto_recal: bool,
    debounce_us: u16,
}

impl MockPad {
    fn new(kind: MockKind) -> Self {
        let press = match kind {
            MockKind::LoadCell => INIT_LOADCELL_PRESS,
            MockKind::Fsr => INIT_FSR_THRESHOLD,
        };
        Self {
            kind,
            press: [[press; PANEL_SENSOR_COUNT]; PAD_BUTTON_COUNT],
            release: [INIT_LOADCELL_RELEASE; PAD_BUTTON_COUNT],
            enabled: [[true; PANEL_SENSOR_COUNT]; PAD_BUTTON_COUNT],
            auto_recal: true,
            debounce_us: INIT_DEBOUNCE_US,
        }
    }

    fn threshold_range(&self) -> (u16, u16) {
        match self.kind {
            MockKind::LoadCell => (MIN_LOADCELL_THRESHOLD, MAX_LOADCELL_THRESHOLD),
            MockKind::Fsr => (MIN_FSR_THRESHOLD, MAX_FSR_THRESHOLD),
        }
    }
}

pub struct Monitor {
    pads: Vec<MockPad>,
    started: Instant,
}

impl Monitor {
    /// Build the mock monitor from `DEADSYNC_MOCK_PADS`, or `None` if unset.
    pub fn from_env() -> Option<Self> {
        deadsync_smx::mock_pads_env().map(|spec| Self::from_spec(&spec))
    }

    fn from_spec(spec: &str) -> Self {
        let mut kinds: Vec<MockKind> = spec
            .split(',')
            .filter_map(|s| match s.trim().to_ascii_lowercase().as_str() {
                "loadcell" | "load-cell" | "load_cell" => Some(MockKind::LoadCell),
                "fsr" => Some(MockKind::Fsr),
                _ => None,
            })
            .take(2)
            .collect();
        // Any other non-empty value (e.g. "1") still turns the mock on.
        if kinds.is_empty() {
            kinds.push(MockKind::LoadCell);
        }
        log::info!("SMX mock: providing {} fake pad(s): {kinds:?}", kinds.len());
        Self {
            pads: kinds.into_iter().map(MockPad::new).collect(),
            started: Instant::now(),
        }
    }

    pub fn poll_pads(&mut self) -> Vec<PadView> {
        let t = self.started.elapsed().as_secs_f32();
        self.pads
            .iter()
            .enumerate()
            .map(|(i, pad)| {
                let buttons = std::array::from_fn(|b| match pad.kind {
                    MockKind::LoadCell => load_cell_button(pad, i, b, t),
                    MockKind::Fsr => fsr_button(pad, i, b, t),
                });
                let fsr = pad.kind == MockKind::Fsr;
                PadView {
                    device_id: PadDeviceId {
                        backend: BackendKind::Smx,
                        index: i,
                    },
                    device_name: format!("StepManiaX P{} [MOCK]", i + 1),
                    is_p2_side: i == 1,
                    buttons,
                    supports_advanced: fsr,
                    simple_per_sensor_bars: !fsr,
                    supports_sensor_toggle: fsr,
                    auto_recalibration: fsr.then_some(pad.auto_recal),
                    debounce_micros: fsr.then_some(pad.debounce_us),
                }
            })
            .collect()
    }

    pub fn set_threshold(
        &mut self,
        device: PadDeviceId,
        button: usize,
        sensor: Option<usize>,
        kind: ThresholdKind,
        value: u16,
    ) -> bool {
        let Some(pad) = self.pad_mut(device, button) else {
            return false;
        };
        let (min, max) = pad.threshold_range();
        if !(min..=max).contains(&value) {
            return false;
        }
        match (pad.kind, kind) {
            (MockKind::LoadCell, ThresholdKind::Press) => {
                pad.press[button] = [value; PANEL_SENSOR_COUNT];
            }
            (MockKind::LoadCell, ThresholdKind::Release) => pad.release[button] = value,
            (MockKind::Fsr, ThresholdKind::Press) => match sensor {
                Some(s) if s < PANEL_SENSOR_COUNT => pad.press[button][s] = value,
                Some(_) => return false,
                None => pad.press[button] = [value; PANEL_SENSOR_COUNT],
            },
            (MockKind::Fsr, ThresholdKind::Release) => return false,
        }
        true
    }

    pub fn set_sensor_enabled(
        &mut self,
        device: PadDeviceId,
        button: usize,
        sensor: usize,
        enabled: bool,
    ) -> bool {
        let Some(pad) = self.pad_mut(device, button) else {
            return false;
        };
        if pad.kind != MockKind::Fsr || sensor >= PANEL_SENSOR_COUNT {
            return false;
        }
        pad.enabled[button][sensor] = enabled;
        true
    }

    pub fn set_auto_recalibration(&mut self, device: PadDeviceId, enabled: bool) -> bool {
        let Some(pad) = self.pad_mut(device, 0) else {
            return false;
        };
        pad.auto_recal = enabled;
        true
    }

    pub fn set_debounce_micros(&mut self, device: PadDeviceId, micros: u16) -> bool {
        if !(MIN_DEBOUNCE_US..=MAX_DEBOUNCE_US).contains(&micros) {
            return false;
        }
        let Some(pad) = self.pad_mut(device, 0) else {
            return false;
        };
        pad.debounce_us = micros;
        true
    }

    fn pad_mut(&mut self, device: PadDeviceId, button: usize) -> Option<&mut MockPad> {
        if device.backend != BackendKind::Smx || button >= PAD_BUTTON_COUNT {
            return None;
        }
        self.pads.get_mut(device.index)
    }
}

/// A smooth fake sensor reading: idles at zero with staggered "press" bumps
/// that cross typical thresholds, so bars move and active states light up.
fn wave(t: f32, pad: usize, button: usize, sensor: usize, scale: u16) -> u16 {
    let period = 3.0 + 0.7 * button as f32 + 0.35 * sensor as f32 + 1.3 * pad as f32;
    let phase = 1.1 * sensor as f32 + 2.3 * button as f32;
    let s = (t * std::f32::consts::TAU / period + phase).sin();
    (s.max(0.0).powi(3) * 0.85 * f32::from(scale)) as u16
}

fn load_cell_button(pad: &MockPad, pad_idx: usize, b: usize, t: f32) -> ButtonView {
    let threshold = pad.press[b][0];
    let sensors: Vec<SensorView> = (0..PANEL_SENSOR_COUNT)
        .map(|s| {
            let raw_value = wave(t, pad_idx, b, s, LOADCELL_VALUE_SCALE);
            SensorView {
                firmware_index: s,
                label: None, // corners, numbered 1-4 in the UI
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
        label: PAD_BUTTON_LABELS[b],
        sensors,
        min_raw_threshold: MIN_LOADCELL_THRESHOLD,
        max_raw_threshold: MAX_LOADCELL_THRESHOLD,
        aggregate_value,
        aggregate_threshold: threshold,
        active: aggregate_value >= threshold,
        value_scale: LOADCELL_VALUE_SCALE,
        release_threshold: Some(pad.release[b]),
    }
}

fn fsr_button(pad: &MockPad, pad_idx: usize, b: usize, t: f32) -> ButtonView {
    let sensors: Vec<SensorView> = SENSOR_DISPLAY_ORDER
        .iter()
        .map(|&s| {
            let raw_value = wave(t, pad_idx, b, s, FSR_VALUE_SCALE);
            let raw_threshold = pad.press[b][s];
            SensorView {
                firmware_index: s,
                label: Some(SENSOR_EDGE_LABELS[s]),
                raw_value,
                value_norm: normalize(raw_value, FSR_VALUE_SCALE),
                raw_threshold,
                threshold_norm: normalize(raw_threshold, FSR_VALUE_SCALE),
                active: raw_value >= raw_threshold && raw_threshold > 0,
                enabled: pad.enabled[b][s],
            }
        })
        .collect();
    let aggregate_value = sensors.iter().map(|s| s.raw_value).max().unwrap_or(0);
    let aggregate_threshold = sensors.iter().map(|s| s.raw_threshold).max().unwrap_or(0);
    ButtonView {
        label: PAD_BUTTON_LABELS[b],
        sensors,
        min_raw_threshold: MIN_FSR_THRESHOLD,
        max_raw_threshold: MAX_FSR_THRESHOLD,
        aggregate_value,
        aggregate_threshold,
        active: aggregate_value >= aggregate_threshold && aggregate_threshold > 0,
        value_scale: FSR_VALUE_SCALE,
        release_threshold: None,
    }
}

fn normalize(value: u16, max: u16) -> f32 {
    if max == 0 {
        return 0.0;
    }
    (f32::from(value) / f32::from(max)).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dev(index: usize) -> PadDeviceId {
        PadDeviceId {
            backend: BackendKind::Smx,
            index,
        }
    }

    #[test]
    fn spec_parses_kinds_and_defaults_to_one_load_cell() {
        let m = Monitor::from_spec("loadcell,fsr");
        assert_eq!(m.pads.len(), 2);
        assert_eq!(m.pads[0].kind, MockKind::LoadCell);
        assert_eq!(m.pads[1].kind, MockKind::Fsr);
        // Any other non-empty value still enables one load-cell pad.
        let m = Monitor::from_spec("1");
        assert_eq!(m.pads.len(), 1);
        assert_eq!(m.pads[0].kind, MockKind::LoadCell);
    }

    #[test]
    fn load_cell_edits_round_trip_into_the_view() {
        let mut m = Monitor::from_spec("loadcell");
        assert!(m.set_threshold(dev(0), 1, None, ThresholdKind::Press, 120));
        assert!(m.set_threshold(dev(0), 1, None, ThresholdKind::Release, 95));
        let pads = m.poll_pads();
        assert_eq!(pads[0].buttons[1].aggregate_threshold, 120);
        assert_eq!(pads[0].buttons[1].release_threshold, Some(95));
        // Out-of-range values are rejected like the real backend.
        assert!(!m.set_threshold(dev(0), 1, None, ThresholdKind::Press, 201));
        assert!(!m.set_threshold(dev(0), 1, None, ThresholdKind::Release, 19));
    }

    #[test]
    fn fsr_edits_target_sensors_and_reject_release() {
        let mut m = Monitor::from_spec("fsr");
        assert!(m.set_threshold(dev(0), 2, Some(3), ThresholdKind::Press, 60));
        assert!(!m.set_threshold(dev(0), 2, None, ThresholdKind::Release, 60));
        assert!(m.set_sensor_enabled(dev(0), 2, 3, false));
        let pads = m.poll_pads();
        let button = &pads[0].buttons[2];
        assert!(button.release_threshold.is_none());
        let s3 = button
            .sensors
            .iter()
            .find(|s| s.firmware_index == 3)
            .unwrap();
        assert_eq!(s3.raw_threshold, 60);
        assert!(!s3.enabled);
    }
}
