use deadlib_present::color::Color;
use std::hint::black_box;

#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;
#[allow(dead_code)]
#[path = "../../../tests/support/perf.rs"]
mod perf;

fn original(color: Color) -> String {
    let channel = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    let [a, r, g, b] = [color.a, color.r, color.g, color.b].map(channel);
    if a == 255 {
        format!("#{r:02X}{g:02X}{b:02X}")
    } else {
        format!("#{a:02X}{r:02X}{g:02X}{b:02X}")
    }
}

#[test]
fn color_hex_preserves_half_boundaries_saturation_and_nan() {
    let check = |v: f32| {
        for color in [
            Color {
                a: v,
                r: v,
                g: v,
                b: v,
            },
            Color {
                a: 1.0,
                r: v,
                g: 0.5,
                b: 0.0,
            },
        ] {
            let actual = color.to_hex();
            assert_eq!(actual.capacity(), actual.len());
            assert_eq!(actual, original(color), "{:08x}", v.to_bits());
        }
    };
    for integer in -1..=256 {
        let v = (integer as f32 + 0.5) / 255.0;
        for v in [v.next_down(), v, v.next_up()] {
            check(v);
        }
    }
    for exponent in 0..=255 {
        for sign in [0, 1 << 31] {
            for mantissa in [0, 1, 0x3fffff, 0x400000, 0x7fffff] {
                check(f32::from_bits(sign | exponent << 23 | mantissa));
            }
        }
    }
    let mut state = 0x12345678u32;
    for _ in 0..100_000 {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        check(f32::from_bits(state));
    }
}

#[test]
#[ignore = "release CPU-cycle and allocation benchmark"]
fn benchmark_color_hex_kernel() {
    for kind in ["opaque", "translucent", "bits"] {
        let mut state = 0x12345678u32;
        let colors: Vec<_> = (0..1024)
            .map(|_| {
                let mut next = || {
                    state ^= state << 13;
                    state ^= state >> 17;
                    state ^= state << 5;
                    if kind == "bits" {
                        f32::from_bits(state)
                    } else {
                        (state % 256) as f32 / 255.0
                    }
                };
                Color {
                    r: next(),
                    g: next(),
                    b: next(),
                    a: if kind == "opaque" { 1.0 } else { next() },
                }
            })
            .collect();
        let label = format!("color-hex/{kind}");
        let work = || {
            for &color in &colors {
                black_box(black_box(Color::to_hex as fn(Color) -> String)(black_box(
                    color,
                )));
            }
        };
        let (_, counts) = perf::measure(work);
        println!("{label}: allocation {counts:?}");
        assert_eq!(counts.allocs, colors.len());
        assert_eq!(counts.reallocs, 0);
        bench::measure(&label, 300, work);
    }
}
