use crate::{
    Device,
    protocol::{ADC_MAX, PANEL_NAMES},
};
use deadsync_input::fsr::*;
use std::sync::atomic::Ordering;

pub(super) fn pad(d: &Device, save_failed: bool) -> PadView {
    let c = d.core.lock().unwrap();
    let cfg = &c.config;
    let status = if save_failed {
        "Settings could not be saved. Check the log and save directory."
    } else if c.calibration.is_some() {
        "Calibrating: keep all panels unloaded."
    } else if c.calibration_failed {
        "Calibration moved too much. Step off and retry with F5."
    } else if cfg.baseline.is_none() {
        "Calibration needed: Start opens Advanced; step off, then press F5 twice."
    } else if d.light_error.load(Ordering::Relaxed) {
        "Light output unavailable; sensor input remains active."
    } else {
        "Calibration and thresholds are stored on this computer."
    };
    PadView {
        device_id: PadDeviceId {
            backend: BackendKind::Reflex,
            index: d.token,
        },
        device_name: d.name.clone(),
        thresholds_editable: true,
        status: Some(status.into()),
        host_controls: Some(HostPadControls {
            player: d.player,
            brightness: cfg.brightness,
            lights: cfg.lights.label(),
        }),
        is_p2_side: d.player == 1,
        supports_advanced: true,
        simple_per_sensor_bars: true,
        supports_sensor_toggle: true,
        auto_recalibration: None,
        debounce_micros: None,
        buttons: (0..4)
            .map(|panel| {
                // Pressure deltas and thresholds share one linear curve. Keep useful
                // resolution for the default ~30-count threshold, without clipping
                // legitimately larger user thresholds at an arbitrary software cap.
                let max = cfg.press[panel * 4..panel * 4 + 4]
                    .iter()
                    .copied()
                    .max()
                    .unwrap_or(30)
                    .saturating_mul(2)
                    .clamp(100, ADC_MAX);
                let curve = ValueCurve::linear(max);
                let sensors: SensorViews = (panel * 4..panel * 4 + 4)
                    .map(|i| {
                        let raw = cfg
                            .baseline
                            .map_or(c.values[i], |base| c.values[i].saturating_sub(base[i]));
                        SensorView {
                            firmware_index: i,
                            label: None,
                            raw_value: raw,
                            value_norm: curve.normalize(raw),
                            raw_threshold: cfg.press[i],
                            threshold_norm: curve.normalize(cfg.press[i]),
                            release_threshold: Some(cfg.release[i]),
                            baseline: cfg.baseline.map(|b| b[i]),
                            active: c.active[i],
                            enabled: cfg.sensors_enabled[i],
                        }
                    })
                    .collect();
                ButtonView {
                    label: ButtonLabel::Named(PANEL_NAMES[panel]),
                    min_raw_threshold: 1,
                    max_raw_threshold: ADC_MAX,
                    aggregate_value: sensors
                        .iter()
                        .filter(|s| s.enabled)
                        .map(|s| s.raw_value)
                        .max()
                        .unwrap_or(0),
                    aggregate_threshold: sensors
                        .iter()
                        .filter(|s| s.enabled)
                        .map(|s| s.raw_threshold)
                        .max()
                        .unwrap_or(0),
                    active: c.panels & (1 << panel) != 0,
                    value_curve: curve,
                    release_threshold: None,
                    sensors,
                }
            })
            .collect(),
    }
}
