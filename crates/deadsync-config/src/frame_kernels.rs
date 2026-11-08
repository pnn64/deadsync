use crate::coin::CoinMode;
use crate::dirs::resource_perf::{support as perf, text_codecs::bench};
use crate::frame_pacing::{seconds_to_us_u32, stutter_severity};
use crate::null_or_die::{parse_null_or_die_kernel_target, parse_null_or_die_kernel_type};
use ::null_or_die::{BiasKernel, KernelTarget};
use std::str::FromStr;
#[path = "frame_original.rs"]
mod original;
use std::hint::black_box as bb;
fn check_float(seconds: f32, frame: f32, expected: f32) {
    assert_eq!(
        original::seconds_to_us_u32(seconds),
        seconds_to_us_u32(seconds),
        "seconds {:08x}",
        seconds.to_bits()
    );
    assert_eq!(
        original::stutter_severity(frame, expected),
        stutter_severity(frame, expected),
        "frame {:08x} expected {:08x}",
        frame.to_bits(),
        expected.to_bits()
    );
}
fn float_cases(step: usize, mut visit: impl FnMut(f32, f32, f32)) {
    for hi in (0..=65535u32).step_by(step) {
        for lo in [0, 1, 65535] {
            let x = f32::from_bits((hi << 16) | lo);
            let a = x * 2.0;
            let b = x * 4.0;
            for frame in [
                x,
                0.0,
                -0.0,
                0.1,
                f32::from_bits(0.1f32.to_bits() - 1),
                f32::from_bits(0.1f32.to_bits() + 1),
                a,
                f32::from_bits(a.to_bits().wrapping_sub(1)),
                f32::from_bits(a.to_bits().wrapping_add(1)),
                b,
                f32::from_bits(b.to_bits().wrapping_sub(1)),
                f32::from_bits(b.to_bits().wrapping_add(1)),
                f32::INFINITY,
                f32::NEG_INFINITY,
                f32::from_bits(0x7f800001),
            ] {
                visit(x, frame, x);
            }
        }
    }
    let threshold = ((u32::MAX as f32) / 1_000_000.0).to_bits();
    for delta in 0..4096u32 {
        for bits in [threshold + delta, threshold - delta] {
            let x = f32::from_bits(bits);
            visit(x, x, 0.016);
        }
    }
    let mut seed = 0x12345678u32;
    for _ in 0..131072 {
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            f32::from_bits(seed)
        };
        visit(next(), next(), next());
    }
}
fn check_text(raw: &str) {
    assert_eq!(original::coin(raw), CoinMode::from_str(raw), "coin {raw:?}");
    assert_eq!(
        original::parse_null_or_die_kernel_type(raw),
        parse_null_or_die_kernel_type(raw),
        "type {raw:?}"
    );
    assert_eq!(
        original::parse_null_or_die_kernel_target(raw),
        parse_null_or_die_kernel_target(raw),
        "target {raw:?}"
    );
}
fn text_cases(step: usize, mut visit: impl FnMut(&str)) {
    for scalar in (0..0x110000).step_by(step) {
        if let Some(c) = char::from_u32(scalar) {
            visit(&c.to_string());
            visit(&format!("lo{c}udest"));
            visit(&format!("rising{c}"));
        }
    }
    for name in [
        "rising",
        "loudest",
        "digest",
        "accumulator",
        "pay",
        "free",
        "home",
    ] {
        for mask in 0..1usize << name.len() {
            let s: String = name
                .bytes()
                .enumerate()
                .map(|(i, b)| {
                    char::from(if mask & (1 << i) == 0 {
                        b
                    } else {
                        b.to_ascii_uppercase()
                    })
                })
                .collect();
            visit(&s);
            visit(&format!("\u{2003}{s}\t"));
        }
        for at in 0..=name.len() {
            for b in 0..128u8 {
                visit(&format!("{}{}{}", &name[..at], char::from(b), &name[at..]));
            }
        }
        for n in [0, 1, 31, 32, 33, 255, 4096] {
            visit(&format!("{name}{}", "_\u{96ea}".repeat(n)));
            visit(&format!("{name}{}", "a".repeat(n)));
        }
    }
    let chars = [
        'r', 'L', '1', '_', ' ', '\0', '\u{96ea}', '\u{e9}', '\u{2003}',
    ];
    let mut seed = 12345678u32;
    for _ in 0..16384 {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        let n = seed % 80;
        let s: String = (0..n)
            .map(|_| {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                chars[seed as usize % chars.len()]
            })
            .collect();
        visit(&s);
    }
}
#[test]
fn exact_float_bits_and_kernel_aliases() {
    float_cases(1, check_float);
    text_cases(1, check_text);
}
fn compare(label: &str, iterations: usize, mut work: impl FnMut(bool)) {
    for (c, name) in [(false, "original"), (true, "current")] {
        let (_, v) = perf::measure(|| work(c));
        println!("{label}/{name}: allocation {v:?}");
    }
    bench::compare(label, iterations, work);
}
#[test]
#[ignore]
fn benchmark() {
    for (label, value) in [
        ("normal", 0.016),
        ("zero", 0.0),
        ("negative", -1.0),
        ("overflow", 1e10),
        ("nan", f32::from_bits(0x7fc12345)),
        ("boundary", 4294.9673),
    ] {
        compare(&format!("seconds/{label}"), 100000, |c| {
            bb(if c {
                seconds_to_us_u32(bb(value))
            } else {
                original::seconds_to_us_u32(bb(value))
            });
        });
    }
    for (label, frame, expected) in [
        ("level0", 0.016, 0.016),
        ("level1", 0.04, 0.016),
        ("level2", 0.08, 0.016),
        ("level3", 0.2, 0.016),
        ("nan", 0.2, f32::NAN),
        ("negative", 0.2, -0.016),
        ("nonmonotone", 0.15, 0.1),
    ] {
        compare(&format!("stutter/{label}"), 100000, |c| {
            bb(if c {
                stutter_severity(bb(frame), bb(expected))
            } else {
                original::stutter_severity(bb(frame), bb(expected))
            });
        });
    }
    let values: Vec<_> = (0..256).map(|i| (i as f32 - 16.0) * 0.0007).collect();
    compare("seconds/mixed256", 2000, |c| {
        let mut acc = 0u32;
        for &v in &values {
            acc ^= if c {
                seconds_to_us_u32(bb(v))
            } else {
                original::seconds_to_us_u32(bb(v))
            };
        }
        bb(acc);
    });
    compare("stutter/mixed256", 2000, |c| {
        let mut acc = 0u32;
        for &v in &values {
            acc += u32::from(if c {
                stutter_severity(bb(v), bb(0.016))
            } else {
                original::stutter_severity(bb(v), bb(0.016))
            });
        }
        bb(acc);
    });
    let shuffled: Vec<_> = (0..256).map(|i| values[(i * 73) % 256]).collect();
    compare("stutter/shuffled256", 2000, |c| {
        let mut acc = 0u32;
        for &v in &shuffled {
            acc += u32::from(if c {
                stutter_severity(bb(v), bb(0.016))
            } else {
                original::stutter_severity(bb(v), bb(0.016))
            });
        }
        bb(acc);
    });
    for (label, raw) in [
        ("rising", "Rising".into()),
        ("loudest", "LOUDEST".into()),
        ("punct", "lo-u_\u{96ea}d e s t".into()),
        ("invalid", "unknown".into()),
        ("tail", format!("rising{}", "_\u{96ea}".repeat(64))),
        ("empty", String::new()),
        ("unicode", "\u{96ea}Rising\u{2003}".into()),
        ("extra", "rising1".into()),
    ] {
        compare(&format!("type/{label}"), 20000, |c| {
            bb(if c {
                parse_null_or_die_kernel_type(bb(&raw))
            } else {
                original::parse_null_or_die_kernel_type(bb(&raw))
            });
        });
    }
    for (label, raw) in [("digest", "Digest"), ("accumulator", "ACCUMULATOR")] {
        compare(&format!("target/{label}"), 20000, |c| {
            bb(if c {
                parse_null_or_die_kernel_target(bb(raw))
            } else {
                original::parse_null_or_die_kernel_target(bb(raw))
            });
        });
    }

    for (label, raw) in [
        ("pay", "Pay"),
        ("free", "FREE"),
        ("home", "Home"),
        ("space", "\u{2003}HOME\t"),
        ("invalid", "unknown"),
        ("empty", ""),
    ] {
        compare(&format!("coin/{label}"), 50000, |c| {
            let _ = bb(if c {
                CoinMode::from_str(bb(raw))
            } else {
                original::coin(bb(raw))
            });
        });
    }
}

#[test]
#[ignore = "deterministic integer/enum trace for frame and config kernels"]
fn dump_trace() {
    use std::io::Write;
    let mut out = std::io::BufWriter::new(
        std::fs::File::create(
            std::env::var_os("DEADSYNC_FRAME_KERNELS_TRACE_OUT").expect("trace path"),
        )
        .unwrap(),
    );
    float_cases(17, |seconds, frame, expected| {
        for x in [seconds, frame, expected] {
            out.write_all(&x.to_bits().to_le_bytes()).unwrap();
        }
        out.write_all(&seconds_to_us_u32(seconds).to_le_bytes())
            .unwrap();
        out.write_all(&[stutter_severity(frame, expected)]).unwrap();
    });
    text_cases(17, |raw| {
        out.write_all(&(raw.len() as u64).to_le_bytes()).unwrap();
        out.write_all(raw.as_bytes()).unwrap();
        let kind = match parse_null_or_die_kernel_type(raw) {
            None => 0,
            Some(BiasKernel::Rising) => 1,
            Some(BiasKernel::Loudest) => 2,
        };
        let target = match parse_null_or_die_kernel_target(raw) {
            None => 0,
            Some(KernelTarget::Digest) => 1,
            Some(KernelTarget::Accumulator) => 2,
        };
        let coin = match CoinMode::from_str(raw) {
            Err(()) => 0,
            Ok(CoinMode::Pay) => 1,
            Ok(CoinMode::Free) => 2,
            Ok(CoinMode::Home) => 3,
        };
        out.write_all(&[kind, target, coin]).unwrap();
    });
    out.flush().unwrap();
}
