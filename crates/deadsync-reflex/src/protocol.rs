//! RE:Flex I/O v2: vendor HID input and complete, segmented GRB light frames.
//! Wire layout follows ReflexCreations/io-firmware at af0d201 (also v2.0).
pub const VID: u16 = 0x0483;
pub const PID: u16 = 0x5750;
pub const SENSORS: usize = 16;
pub const PANELS: usize = 4;
pub const ADC_MAX: u16 = 4095;
pub const PANEL_NAMES: [&str; PANELS] = ["L", "D", "U", "R"];

pub fn decode(report: &[u8]) -> Option<[u16; SENSORS]> {
    // HIDAPI removes the synthetic zero report ID. There is no numbered report
    // in the descriptor; accepting arbitrary offsets would hide framing errors.
    if report.len() != 64 {
        return None;
    }
    let mut sensors = [0; SENSORS];
    for (sensor, bytes) in sensors.iter_mut().zip(report[..32].as_chunks::<2>().0) {
        *sensor = u16::from_le_bytes([bytes[0], bytes[1]]);
        if *sensor > ADC_MAX {
            return None;
        }
    }
    Some(sensors)
}

/// Uniform panel colors still address every LED: four 21-LED segments per panel.
/// Byte zero is HIDAPI's synthetic report ID, not part of the 64-byte payload.
pub fn light_packet(colors: &[[u8; 3]; PANELS], segment: u8, frame: u8) -> [u8; 65] {
    assert!(segment < 16);
    let mut out = [0; 65];
    out[1] = (segment << 4) | (frame & 15);
    let [r, g, b] = colors[usize::from(segment / 4)];
    for led in out[2..].as_chunks_mut::<3>().0 {
        led.copy_from_slice(&[g, r, b]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_all_sensors_in_wire_order_and_rejects_bad_reports() {
        let mut bytes = [0; 64];
        for i in 0..16 {
            bytes[2 * i..2 * i + 2].copy_from_slice(&(257 * i as u16).to_le_bytes());
        }
        assert_eq!(
            decode(&bytes).unwrap(),
            std::array::from_fn(|i| 257 * i as u16)
        );
        assert!(decode(&bytes[..32]).is_none());
        assert!(decode(&[0; 65]).is_none());
        bytes[1] = 16;
        assert!(decode(&bytes).is_none());
    }

    #[test]
    fn complete_frames_cover_every_segment_with_one_frame_id_and_grb_order() {
        let colors = [[1, 2, 3], [4, 5, 6], [7, 8, 9], [10, 11, 12]];
        for frame in 0..32 {
            let mut seen = 0u16;
            for segment in 0..16 {
                let p = light_packet(&colors, segment, frame);
                assert_eq!(p[0], 0);
                assert_eq!(p[1] & 15, frame & 15);
                seen |= 1 << (p[1] >> 4);
                let [r, g, b] = colors[usize::from(segment / 4)];
                assert!(
                    p[2..]
                        .as_chunks::<3>()
                        .0
                        .iter()
                        .all(|led| *led == [g, r, b])
                );
            }
            assert_eq!(seen, u16::MAX);
        }
    }
}
