#[allow(dead_code)]
#[path = "../../../tests/support/allocations.rs"]
mod perf;

#[allow(dead_code)]
mod backend {
    include!("../src/lib.rs");

    mod depth_kernel {
        use super::*;

        use deadlib_render_core::DepthMode;


        fn check(z: f32) {
            for mode in DepthMode::ALL {
                for initial in [0.0, -0.0, 0.5, 1.0, f32::INFINITY, f32::NAN] {
                    let mut pixels = [initial];
                    let actual = DepthRows {
                        pixels: &mut pixels,
                        unorm16: true,
                        mode,
                    }
                    .test(0, z);
                    let quantized = (z * 65535.0).round() / 65535.0;
                    let expected = (0.0..=1.0).contains(&z)
                        && !(mode.tests()
                            && if mode.on_fail() {
                                quantized <= initial
                            } else {
                                quantized > initial
                            });
                    let stored = if expected && mode.writes() {
                        quantized
                    } else {
                        initial
                    };
                    assert_eq!(actual, expected);
                    assert_eq!(
                        pixels[0].to_bits(),
                        stored.to_bits(),
                        "z={:08x}, {mode:?}",
                        z.to_bits()
                    );
                }
            }
        }

        #[test]
        fn depth_preserves_rounding_signed_zero_and_all_modes() {
            for step in 0..65535 {
                let z = (step as f32 + 0.5) / 65535.0;
                for z in [z.next_down(), z, z.next_up()] {
                    check(z);
                }
            }
            for z in [
                0.0,
                -0.0,
                1.0,
                -1.0,
                f32::NAN,
                f32::INFINITY,
                f32::NEG_INFINITY,
            ] {
                check(z);
            }
            let mut state = 0x12345678u32;
            for _ in 0..100_000 {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                check(f32::from_bits(state));
            }
        }

        #[inline(never)]
        fn write_depth(rows: &mut DepthRows<'_>, z: f32) -> bool {
            rows.test(0, z)
        }

    }
}
