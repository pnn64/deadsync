use super::*;
pub(super) fn fsr_button_view(
    label: &'static str,
    readings: [SensorReading; PANEL_SENSOR_COUNT],
    panel_active: bool,
) -> ButtonView {
    let sensors: SensorViews = readings
        .iter()
        .map(|r| SensorView {
            release_threshold: None,
            baseline: None,
            firmware_index: r.firmware_index,
            label: r.label,
            raw_value: r.value,
            value_norm: FSR_VALUE_CURVE.normalize(r.value),
            raw_threshold: r.threshold,
            threshold_norm: FSR_VALUE_CURVE.normalize(r.threshold),
            active: r.value >= r.threshold && r.threshold > 0,
            enabled: r.enabled,
        })
        .collect();
    let aggregate_value = sensors.iter().map(|s| s.raw_value).max().unwrap_or(0);
    let aggregate_threshold = sensors.iter().map(|s| s.raw_threshold).max().unwrap_or(0);
    ButtonView {
        label: ButtonLabel::Named(label),
        sensors,
        min_raw_threshold: MIN_FSR_THRESHOLD,
        max_raw_threshold: MAX_FSR_THRESHOLD,
        aggregate_value,
        aggregate_threshold,
        active: panel_active,
        value_curve: FSR_VALUE_CURVE,
        release_threshold: None,
    }
}

pub(super) fn load_cell_button_view(
    label: &'static str,
    values: [u16; PANEL_SENSOR_COUNT],
    press: u16,
    release: u16,
    panel_active: bool,
) -> ButtonView {
    let sensors: SensorViews = values
        .iter()
        .enumerate()
        .map(|(s, &raw_value)| SensorView {
            release_threshold: None,
            baseline: None,
            firmware_index: s,
            label: None, // corners, not edges -> numbered 1-4 in the UI
            raw_value,
            value_norm: LOADCELL_VALUE_CURVE.normalize(raw_value),
            raw_threshold: press,
            threshold_norm: LOADCELL_VALUE_CURVE.normalize(press),
            active: panel_active,
            enabled: true,
        })
        .collect();
    let aggregate_value = sensors.iter().map(|s| s.raw_value).max().unwrap_or(0);
    ButtonView {
        label: ButtonLabel::Named(label),
        sensors,
        min_raw_threshold: MIN_LOADCELL_THRESHOLD,
        max_raw_threshold: MAX_LOADCELL_THRESHOLD,
        aggregate_value,
        aggregate_threshold: press,
        active: panel_active,
        value_curve: LOADCELL_VALUE_CURVE,
        release_threshold: Some(release),
    }
}
