use super::*;
pub(super) fn button_views(config: &ConfigReport, input: &InputReport) -> ButtonViews {
    mapped_buttons(&config.sensor_to_button_mapping)
        .into_iter()
        .map(|button| button_view(config, input, button))
        .collect()
}

pub(super) fn button_view(config: &ConfigReport, input: &InputReport, button: usize) -> ButtonView {
    let mut sensors = SensorViews::new();
    let mut aggregate_value = 0;
    let mut aggregate_threshold = 0;
    for (index, &mapped) in config.sensor_to_button_mapping.iter().enumerate() {
        if mapped < 0 || mapped as usize != button {
            continue;
        }
        let raw_value = input.sensor_values[index];
        let raw_threshold = config.sensor_thresholds[index];
        aggregate_value = aggregate_value.max(raw_value);
        aggregate_threshold = aggregate_threshold.max(raw_threshold);
        sensors.push(SensorView {
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
    ButtonView {
        label: ButtonLabel::Hid((button + 1) as u8),
        sensors,
        min_raw_threshold: 0,
        max_raw_threshold: MAX_SENSOR_VALUE,
        aggregate_value,
        aggregate_threshold,
        active: aggregate_value >= aggregate_threshold && aggregate_threshold > 0,
        value_curve: VALUE_CURVE,
        release_threshold: None,
    }
}

pub(super) fn parse_input_report(bytes: &[u8]) -> Option<InputReport> {
    let payload = match bytes {
        [REPORT_ID_SENSOR_VALUES, rest @ ..] if rest.len() >= 2 + SENSOR_COUNT * 2 => rest,
        rest if rest.len() >= 2 + SENSOR_COUNT * 2 => rest,
        _ => return None,
    };

    let _button_bits = u16::from_le_bytes(payload[0..2].try_into().ok()?);
    let mut sensor_values = [0u16; SENSOR_COUNT];
    let mut offset = 2usize;
    for value in &mut sensor_values {
        let end = offset + 2;
        *value = u16::from_le_bytes(payload[offset..end].try_into().ok()?);
        offset = end;
    }
    Some(InputReport { sensor_values })
}

pub(super) fn mapped_buttons(mapping: &[i8; SENSOR_COUNT]) -> ArrayVec<usize, SENSOR_COUNT> {
    let mut mapped_mask = 0u16;
    for &mapped in mapping {
        if mapped >= 0 && (mapped as usize) < BUTTON_COUNT {
            mapped_mask |= 1 << mapped;
        }
    }
    let mut buttons: ArrayVec<usize, SENSOR_COUNT> = ArrayVec::new();
    while mapped_mask != 0 {
        let button = mapped_mask.trailing_zeros() as usize;
        buttons.push(button);
        mapped_mask &= mapped_mask - 1;
    }
    buttons
}

pub(super) fn group_sensor_indices(
    mapping: &[i8; SENSOR_COUNT],
    group: usize,
) -> ArrayVec<usize, SENSOR_COUNT> {
    mapped_buttons(mapping)
        .get(group)
        .map_or_else(ArrayVec::new, |&button| sensor_indices(mapping, button))
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
