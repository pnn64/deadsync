use super::*;
use std::hint::black_box;
#[path = "../../../tests/support/paired_bench.rs"]
mod paired;

// Frozen from starting main a60358c4d.
impl GlowEffect {
    #[inline(always)]
    fn original_color_at(&self, time: f32, base_alpha: f32, optimized: bool) -> [f32; 4] {
        if self.period <= f32::EPSILON || base_alpha <= f32::EPSILON {
            return [0.0, 0.0, 0.0, 0.0];
        }

        let phase = (time / self.period).rem_euclid(1.0);
        if !phase.is_finite() {
            return [0.0, 0.0, 0.0, 0.0];
        }

        const OPAQUE_WHITE: [f32; 4] = [1.0; 4];
        if optimized
            && base_alpha.is_finite()
            && self.color1 == OPAQUE_WHITE
            && self.color2 == OPAQUE_WHITE
        {
            return [1.0, 1.0, 1.0, base_alpha];
        }

        if optimized
            && self.color1 == self.color2
            && self
                .color1
                .iter()
                .all(|channel| *channel == 0.0 || *channel == 1.0)
        {
            let mut color = self.color1;
            color[3] *= base_alpha;
            return color;
        }

        let percent_between = ((phase + 0.25) * std::f32::consts::TAU)
            .sin()
            .mul_add(0.5, 0.5);

        let mut color = [0.0; 4];
        for (i, channel) in color.iter_mut().enumerate() {
            *channel =
                self.color1[i].mul_add(percent_between, self.color2[i] * (1.0 - percent_between));
        }
        color[3] *= base_alpha;
        color
    }
}

#[test]
fn deferred_glow_phase_preserves_float_behavior() {
    let values = [
        0.0,
        -0.0,
        -1.0,
        f32::EPSILON,
        0.125,
        1.0,
        3.25,
        4096.0,
        f32::MIN_POSITIVE,
        f32::MAX,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NAN,
    ];
    for (color1, color2) in [
        ([1.0; 4], [1.0; 4]),
        ([1.0, 0.0, -0.0, 1.0], [1.0, 0.0, -0.0, 1.0]),
        ([0.25; 4], [0.25; 4]),
        ([1.0, 0.3, 0.0, 0.5], [0.0, 0.7, 1.0, 0.9]),
        ([f32::NAN; 4], [0.0; 4]),
    ] {
        for period in values {
            let effect = GlowEffect {
                period,
                color1,
                color2,
            };
            for time in values {
                for alpha in values {
                    for optimized in [false, true] {
                        let a = effect.original_color_at(time, alpha, optimized);
                        let b = effect.color_at_impl(time, alpha, optimized);
                        for (a, b) in a.into_iter().zip(b) {
                            assert!(
                                a.to_bits() == b.to_bits() || (a.is_nan() && b.is_nan()),
                                "{effect:?} time={time} alpha={alpha}: {a:?} vs {b:?}"
                            );
                        }
                    }
                }
            }
        }
        let effect = GlowEffect {
            period: 0.73,
            color1,
            color2,
        };
        for i in -4096..4096 {
            for (a, b) in effect
                .original_color_at(i as f32 * 0.03125, 0.65, true)
                .into_iter()
                .zip(effect.color_at(i as f32 * 0.03125, 0.65))
            {
                assert!(a.to_bits() == b.to_bits() || (a.is_nan() && b.is_nan()));
            }
        }
    }
}

#[test]
#[ignore = "paired release benchmark"]
fn benchmark_constant_glow_phase() {
    for (name, color1, color2) in [
        ("white", [1.0; 4], [1.0; 4]),
        ("binary", [1.0, 0.0, 1.0, 1.0], [1.0, 0.0, 1.0, 1.0]),
        (
            "varying control",
            [1.0, 0.3, 0.0, 0.5],
            [0.0, 0.7, 1.0, 0.9],
        ),
    ] {
        let effect = black_box(GlowEffect {
            period: 0.73,
            color1,
            color2,
        });
        let times: [f32; 256] = std::array::from_fn(|i| 123.0 + i as f32 * 0.015625);
        paired::compare(&format!("glow {name}, 256 calls"), 4_000, |current| {
            for time in black_box(&times) {
                black_box(if current {
                    effect.color_at(black_box(*time), black_box(0.65))
                } else {
                    effect.original_color_at(black_box(*time), black_box(0.65), true)
                });
            }
        });
    }
}
