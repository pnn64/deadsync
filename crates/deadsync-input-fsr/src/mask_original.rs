use super::*;
pub(super) fn mapped_button_mask(mapping: &[i8; SENSOR_COUNT]) -> u16 {
    let mut mapped_mask = 0u16;
    for &mapped in mapping {
        if mapped >= 0 && (mapped as usize) < BUTTON_COUNT {
            mapped_mask |= 1 << mapped;
        }
    }
    mapped_mask
}

pub(super) fn sensor_indices(
    mapping: &[i8; SENSOR_COUNT],
    button: usize,
) -> ArrayVec<usize, SENSOR_COUNT> {
    mapping
        .iter()
        .enumerate()
        .filter_map(|(index, &mapped)| (mapped >= 0 && mapped as usize == button).then_some(index))
        .collect()
}

pub(super) fn normalize_sensor_value(raw: u16) -> f32 {
    SENSOR_NORMALIZED[min(raw, MAX_SENSOR_VALUE) as usize]
}

pub(super) fn group_sensor_indices(
    mapping: &[i8; SENSOR_COUNT],
    group: usize,
) -> ArrayVec<usize, SENSOR_COUNT> {
    let mut mask = mapped_button_mask(mapping);
    if group >= mask.count_ones() as usize {
        return ArrayVec::new();
    }
    for _ in 0..group {
        mask &= mask - 1;
    }
    sensor_indices(mapping, mask.trailing_zeros() as usize)
}

pub(super) fn button_views(config: &ConfigReport, input: &InputReport) -> ButtonViews {
    let mut buttons = ButtonViews::new();
    let mut slots = [0usize; BUTTON_COUNT];
    let mut mask = mapped_button_mask(&config.sensor_to_button_mapping);
    while mask != 0 {
        let button = mask.trailing_zeros() as usize;
        slots[button] = buttons.len();
        buttons.push(ButtonView {
            label: ButtonLabel::Hid((button + 1) as u8),
            sensors: SensorViews::new(),
            min_raw_threshold: 0,
            max_raw_threshold: MAX_SENSOR_VALUE,
            aggregate_value: 0,
            aggregate_threshold: 0,
            active: false,
            value_curve: VALUE_CURVE,
            release_threshold: None,
        });
        mask &= mask - 1;
    }
    for (index, &mapped) in config.sensor_to_button_mapping.iter().enumerate() {
        if mapped < 0 || mapped as usize >= BUTTON_COUNT {
            continue;
        }
        let button = &mut buttons[slots[mapped as usize]];
        let raw_value = input.sensor_values[index];
        let raw_threshold = config.sensor_thresholds[index];
        button.aggregate_value = button.aggregate_value.max(raw_value);
        button.aggregate_threshold = button.aggregate_threshold.max(raw_threshold);
        button.sensors.push(SensorView {
            release_threshold: None,
            baseline: None,
            firmware_index: index,
            label: None,
            raw_value,
            value_norm: normalize_sensor_value(raw_value),
            raw_threshold,
            threshold_norm: normalize_sensor_value(raw_threshold),
            active: raw_value >= raw_threshold && raw_threshold > 0,
            enabled: true,
        });
    }
    for button in &mut buttons {
        button.active =
            button.aggregate_value >= button.aggregate_threshold && button.aggregate_threshold > 0;
    }
    buttons
}
