use super::*;

#[test]
fn input_reports_decode_little_endian_samples_with_and_without_report_id() {
    for raw in 0..=u16::MAX {
        let values: [u16; SENSOR_COUNT] = std::array::from_fn(|i| raw.wrapping_add(i as u16));
        let mut bytes = [0; 27];
        bytes[0] = REPORT_ID_SENSOR_VALUES;
        for (i, value) in values.iter().enumerate() {
            bytes[3 + i * 2..5 + i * 2].copy_from_slice(&value.to_le_bytes());
        }
        assert_eq!(parse_input_report(&bytes).unwrap().sensor_values, values);
        assert_eq!(parse_input_report(&bytes[1..]).unwrap().sensor_values, values);
    }
}

#[test]
fn input_reports_reject_payloads_shorter_than_all_sensor_samples() {
    for length in 0..26 {
        assert!(parse_input_report(&[0; 26][..length]).is_none());
    }
}
