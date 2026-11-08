use super::rgba_pixel;

fn check(y: f32, u: f32, v: f32, coeffs: [f32; 4]) {
    let expected = [
        (coeffs[0].mul_add(v, y) * 255.0).round() as u8,
        (coeffs[2].mul_add(v, coeffs[1].mul_add(u, y)) * 255.0).round() as u8,
        (coeffs[3].mul_add(u, y) * 255.0).round() as u8,
        255,
    ];
    assert_eq!(
        rgba_pixel(y, u, v, coeffs),
        expected,
        "y={:08x}, u={:08x}, v={:08x}, coeffs={:?}",
        y.to_bits(),
        u.to_bits(),
        v.to_bits(),
        coeffs.map(f32::to_bits)
    );
}

#[test]
fn yuv_quantization_preserves_half_boundaries_and_arbitrary_float_bits() {
    for integer in -1..=256 {
        let y = (integer as f32 + 0.5) / 255.0;
        for y in [
            y.next_down().next_down(),
            y.next_down(),
            y,
            y.next_up(),
            y.next_up().next_up(),
        ] {
            check(y, 0.0, 0.0, [0.0; 4]);
        }
    }
    for exponent in 0..=255 {
        for sign in [0, 1 << 31] {
            for mantissa in [0, 1, 0x3f_ffff, 0x40_0000, 0x7f_ffff] {
                check(
                    f32::from_bits(sign | (exponent << 23) | mantissa),
                    0.0,
                    0.0,
                    [0.0; 4],
                );
            }
        }
    }
    let mut state = 0x1234_5678u32;
    let mut next = || {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        f32::from_bits(state)
    };
    for _ in 0..1_000_000 {
        check(next(), next(), next(), std::array::from_fn(|_| next()));
    }
}
