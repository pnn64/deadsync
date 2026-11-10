//! Exact pixel checks and paired CPU/allocation measurements.
use super::*;


#[test]
fn axis_identity_respects_float_coordinate_limit() {
    for x in 0..1 << 22 {
        // Reproduce equal-dimension zoom_axis's downsample branch exactly.
        let center = x as f32 + 0.5;
        let left = (center - 0.25) as u32;
        let right = (center + 0.25) as u32;
        let weight = if left == right {
            1 << 24
        } else {
            ((1.0 - (center - (left as f32 + 0.5)) / (right - left) as f32) * 16777216.0) as u32
        };
        assert_eq!((left, weight), (x, 1 << 24));
        assert!(right < 1 << 22);
    }
    assert_eq!(
        zoom_axis((1 << 22) + 1, (1 << 22) + 1)
            .nth(1 << 22)
            .unwrap()
            .1,
        (1 << 22) + 1
    );
}
