//! Synthetic SMX pads for UI development, enabled with `DEADSYNC_MOCK_PADS`.
//!
//! Set the variable to a comma-separated list of pad kinds, e.g.
//! `DEADSYNC_MOCK_PADS=loadcell` or `DEADSYNC_MOCK_PADS=loadcell,fsr` (any
//! other truthy value means one load-cell pad; empty/`0`/`false`/`off`/`no`
//! leave the mock off). Each kind becomes one fake pad on the Configure Pads
//! screen, with animated sensor readings and thresholds edited in memory, so
//! the whole editor can be exercised without hardware. While the mock is on,
//! `deadsync_smx::init` refuses to start the SDK, so native SMX is fully off
//! and nothing is written over USB.

use super::smx::{
    LOADCELL_RAW_MAX, MAX_DEBOUNCE_US, MAX_FSR_THRESHOLD, MAX_LOADCELL_THRESHOLD, MIN_DEBOUNCE_US,
    MIN_FSR_THRESHOLD, MIN_LOADCELL_THRESHOLD, PANEL_SENSOR_COUNT, SENSOR_DISPLAY_ORDER,
    SENSOR_EDGE_LABELS, SensorReading, fsr_button_view, hysteresis_active, load_cell_button_view,
};
use deadsync_input::fsr::{BackendKind, PAD_BUTTON_COUNT, PAD_BUTTON_LABELS, PadDeviceId, PadView};
use std::fmt::Write as _;
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
    /// Sticky per-sensor pressed state (load cell press/release hysteresis).
    held: [[bool; PANEL_SENSOR_COUNT]; PAD_BUTTON_COUNT],
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
            held: [[false; PANEL_SENSOR_COUNT]; PAD_BUTTON_COUNT],
            auto_recal: true,
            debounce_us: INIT_DEBOUNCE_US,
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
        // Any other truthy value (e.g. "1") still turns the mock on.
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
            .iter_mut()
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

    /// Single-threshold (FSR-style) edit; load-cell pads use the pair edit.
    pub fn set_threshold(
        &mut self,
        device: PadDeviceId,
        button: usize,
        sensor: Option<usize>,
        value: u16,
    ) -> bool {
        let Some(pad) = self.pad_mut(device, button) else {
            return false;
        };
        if pad.kind != MockKind::Fsr || !(MIN_FSR_THRESHOLD..=MAX_FSR_THRESHOLD).contains(&value) {
            return false;
        }
        match sensor {
            Some(s) if s < PANEL_SENSOR_COUNT => pad.press[button][s] = value,
            Some(_) => return false,
            None => pad.press[button] = [value; PANEL_SENSOR_COUNT],
        }
        true
    }

    /// Press/release pair edit for a load-cell button, mirroring the real
    /// backend's checks (ranges, release strictly below press).
    pub fn set_threshold_pair(
        &mut self,
        device: PadDeviceId,
        button: usize,
        press: u16,
        release: u16,
    ) -> bool {
        let Some(pad) = self.pad_mut(device, button) else {
            return false;
        };
        let range = MIN_LOADCELL_THRESHOLD..=MAX_LOADCELL_THRESHOLD;
        if pad.kind != MockKind::LoadCell
            || !range.contains(&press)
            || !range.contains(&release)
            || release >= press
        {
            return false;
        }
        pad.press[button] = [press; PANEL_SENSOR_COUNT];
        pad.release[button] = release;
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

    /// The mock has no hardware test mode to toggle.
    pub fn set_active(&mut self, _active: bool) {}

    /// Mock section for the FSR debug dump, in place of the (dormant) native
    /// SMX section, so the dump matches what the UI shows.
    pub fn debug_dump(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "DeadSync StepManiaX mock pads (DEADSYNC_MOCK_PADS)");
        for (i, pad) in self.pads.iter().enumerate() {
            let _ = writeln!(out, "Pad {i}: {:?}", pad.kind);
            for b in 0..PAD_BUTTON_COUNT {
                match pad.kind {
                    MockKind::LoadCell => {
                        let _ = writeln!(
                            out,
                            "  {} press: {} release: {}",
                            PAD_BUTTON_LABELS[b], pad.press[b][0], pad.release[b]
                        );
                    }
                    MockKind::Fsr => {
                        let _ = writeln!(
                            out,
                            "  {} thresholds: {:?} enabled: {:?}",
                            PAD_BUTTON_LABELS[b], pad.press[b], pad.enabled[b]
                        );
                    }
                }
            }
        }
        out
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
fn wave(t: f32, pad: usize, button: usize, sensor: usize, peak: u16) -> u16 {
    let period = 3.0 + 0.7 * button as f32 + 0.35 * sensor as f32 + 1.3 * pad as f32;
    let phase = 1.1 * sensor as f32 + 2.3 * button as f32;
    let s = (t * std::f32::consts::TAU / period + phase).sin();
    (s.max(0.0).powi(3) * 0.85 * f32::from(peak)) as u16
}

fn load_cell_button(
    pad: &mut MockPad,
    pad_idx: usize,
    b: usize,
    t: f32,
) -> deadsync_input::fsr::ButtonView {
    let press = pad.press[b][0];
    let release = pad.release[b];
    // Real load-cell readings reach 500, well past the 250 display scale, so
    // the mock does too (pegged bars + readouts above 250 get exercised).
    let values: [u16; PANEL_SENSOR_COUNT] =
        std::array::from_fn(|s| wave(t, pad_idx, b, s, LOADCELL_RAW_MAX));
    for (s, &v) in values.iter().enumerate() {
        pad.held[b][s] = hysteresis_active(pad.held[b][s], v, release, press);
    }
    // The panel reads as pressed while any sensor is held (hysteresis).
    let panel_active = pad.held[b].iter().any(|&h| h);
    load_cell_button_view(PAD_BUTTON_LABELS[b], values, press, release, panel_active)
}

fn fsr_button(pad: &MockPad, pad_idx: usize, b: usize, t: f32) -> deadsync_input::fsr::ButtonView {
    let readings = SENSOR_DISPLAY_ORDER.map(|s| SensorReading {
        firmware_index: s,
        label: Some(SENSOR_EDGE_LABELS[s]),
        value: wave(t, pad_idx, b, s, MAX_FSR_THRESHOLD),
        threshold: pad.press[b][s],
        enabled: pad.enabled[b][s],
    });
    // The panel reads as pressed while any enabled sensor is over its own
    // threshold (per-sensor, not max-vs-max, which misreads mixed thresholds).
    let panel_active = readings
        .iter()
        .any(|r| r.enabled && r.threshold > 0 && r.value >= r.threshold);
    fsr_button_view(PAD_BUTTON_LABELS[b], readings, panel_active)
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
        // Any other truthy value still enables one load-cell pad.
        let m = Monitor::from_spec("1");
        assert_eq!(m.pads.len(), 1);
        assert_eq!(m.pads[0].kind, MockKind::LoadCell);
    }

    #[test]
    fn load_cell_pair_edits_round_trip_into_the_view() {
        let mut m = Monitor::from_spec("loadcell");
        assert!(m.set_threshold_pair(dev(0), 1, 120, 95));
        let pads = m.poll_pads();
        assert_eq!(pads[0].buttons[1].aggregate_threshold, 120);
        assert_eq!(pads[0].buttons[1].release_threshold, Some(95));
        // Out-of-range or inverted pairs are rejected like the real backend,
        // as are single-threshold edits on a load-cell pad.
        assert!(!m.set_threshold_pair(dev(0), 1, 201, 95));
        assert!(!m.set_threshold_pair(dev(0), 1, 120, 19));
        assert!(!m.set_threshold_pair(dev(0), 1, 95, 95));
        assert!(!m.set_threshold(dev(0), 1, None, 120));
    }

    #[test]
    fn fsr_edits_target_sensors_and_reject_pairs() {
        let mut m = Monitor::from_spec("fsr");
        assert!(m.set_threshold(dev(0), 2, Some(3), 60));
        assert!(!m.set_threshold_pair(dev(0), 2, 80, 70));
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
