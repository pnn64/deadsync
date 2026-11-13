use super::*;


#[test]
fn finite_beat_comparator_matches_row_first_order_at_rounding_and_saturation_edges() {
    let mut values = vec![
        -f32::MAX,
        -1e30,
        -0.0,
        0.0,
        f32::from_bits(1),
        f32::MIN_POSITIVE,
        1e30,
        f32::MAX,
    ];
    for row in [-2147483647i32, -1024, -48, -1, 0, 1, 48, 1024, 2147483647] {
        let beat = (row as f32 + 0.5) / 48.0;
        values.extend([beat.next_down(), beat, beat.next_up()]);
    }
    let mut seed = 0x739115adu32;
    for _ in 0..1024 {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        let value = f32::from_bits(seed);
        if value.is_finite() {
            values.push(value);
        }
    }
    for &a in &values {
        for &b in &values {
            assert_eq!(
                beat_to_note_row(a)
                    .cmp(&beat_to_note_row(b))
                    .then_with(|| a.total_cmp(&b)),
                a.total_cmp(&b)
            );
        }
    }
}
