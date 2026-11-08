use super::text_performance::bench;
use super::*;
use crate::transforms::numeric_performance::allocations as perf;
use std::io::Write;
#[path = "number_original.rs"]
mod original;
include!("number_adapter.rs");
fn assert_strings(a: String, b: String) {
    assert_eq!(a, b);
    assert_eq!(a.capacity(), b.capacity(), "{a:?}");
}
#[test]
fn decimals_integral_floats_and_capacities_match() {
    for value in i16::MIN..=i16::MAX {
        let mut a = String::with_capacity(64);
        let mut b = String::with_capacity(64);
        original::integer(&mut a, value);
        integer(&mut b, value);
        assert_strings(a, b);
        let mut a = String::with_capacity(64);
        let mut b = String::with_capacity(64);
        original::floating(&mut a, f32::from(value));
        floating(&mut b, f32::from(value));
        assert_strings(a, b);
    }
    for cap in 0..80 {
        for len in 0..=cap {
            for value in [
                -32768i16, -9999, -1000, -100, -10, -1, 0, 1, 9, 10, 99, 100, 999, 1000, 32767,
            ] {
                let mut a = String::with_capacity(cap);
                a.extend(std::iter::repeat_n('x', len));
                let mut b = String::with_capacity(cap);
                b.push_str(&a);
                original::floating(&mut a, f32::from(value));
                floating(&mut b, f32::from(value));
                assert_strings(a, b);
            }
        }
    }
    let mut seed = 0x1234_5678u32;
    for _ in 0..65536 {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        let value = f32::from_bits(seed);
        let mut a = String::with_capacity(64);
        let mut b = String::with_capacity(64);
        original::floating(&mut a, value);
        floating(&mut b, value);
        assert_strings(a, b);
    }
    for value in [
        0.0,
        -0.0,
        1.25,
        -1.25,
        32768.0,
        f32::NAN,
        f32::INFINITY,
        f32::NEG_INFINITY,
    ] {
        let mut a = String::new();
        let mut b = String::new();
        original::floating(&mut a, value);
        floating(&mut b, value);
        assert_strings(a, b);
    }
}
#[test]
fn all_transform_combinations_and_capacities_match() {
    for a in 0..=u8::MAX {
        for b in 0..=u8::MAX {
            for c in 0..32u8 {
                let mut x = String::with_capacity(512);
                let mut y = String::with_capacity(512);
                original::transforms(&mut x, a, b, c);
                transforms(&mut y, a, b, c);
                assert_strings(x, y);
            }
        }
    }
    for cap in 0..80 {
        for (a, b, c) in [
            (0, 0, 0),
            (0, 0, 0xe0),
            (1, 2, 3),
            (255, 255, 255),
            (0, 128, 128),
        ] {
            let mut x = String::with_capacity(cap);
            let mut y = String::with_capacity(cap);
            x.push('x');
            y.push('x');
            original::transforms(&mut x, a, b, c);
            transforms(&mut y, a, b, c);
            assert_strings(x, y);
        }
    }
    for kind in 0..3 {
        for value in [
            0.0,
            -0.0,
            1.0,
            1.25,
            600.0,
            32767.0,
            f32::NAN,
            f32::INFINITY,
        ] {
            assert_strings(original::demo(value, kind), demo(value, kind));
        }
    }
}
fn compare(label: &str, count: usize, mut work: impl FnMut(bool)) {
    for (c, name) in [(false, "original"), (true, "current")] {
        let (_, churn) = perf::measure(|| work(c));
        println!("{label}/{name}: allocation {churn:?}");
    }
    bench::compare(label, count, work);
}
#[test]
#[ignore]
fn benchmark() {
    use std::hint::black_box as bb;
    for (label, values) in [
        ("small", vec![0, 1, 5, 9, -1, -9]),
        ("two", vec![10, 25, 50, 99, -25, -99]),
        ("three", vec![100, 125, 200, 999, -125, -999]),
        ("large", vec![1000, 9999, 32767, -1000, -9999, -32768]),
    ] {
        let mut a = String::with_capacity(4096);
        let mut b = String::with_capacity(4096);
        compare(&format!("integer/{label}"), 20000, |c| {
            for &v in bb(&values) {
                if c {
                    b.clear();
                    integer(bb(&mut b), bb(v));
                    bb(&b);
                } else {
                    a.clear();
                    original::integer(bb(&mut a), bb(v));
                    bb(&a);
                }
            }
        });
    }
    for (label, v) in [
        ("zero", 0.0),
        ("negative-zero", -0.0),
        ("one", 1.0),
        ("speed", 600.0),
        ("fraction", 1.25),
        ("outside", 32768.0),
        ("nan", f32::NAN),
        ("infinity", f32::INFINITY),
    ] {
        let mut a = String::with_capacity(4096);
        let mut b = String::with_capacity(4096);
        compare(&format!("float/{label}"), 20000, |c| {
            if c {
                b.clear();
                floating(bb(&mut b), bb(v));
                bb(&b);
            } else {
                a.clear();
                original::floating(bb(&mut a), bb(v));
                bb(&a);
            }
        });
    }
    for (label, (i, r, h)) in [
        ("empty", (0, 0, 0)),
        ("ignored", (0, 0, 0xe0)),
        ("single", (1, 0, 0)),
        ("sparse", (2, 4, 8)),
        ("mixed", (0xa5, 0x36, 0x12)),
        ("dense", (255, 255, 255)),
    ] {
        let mut a = String::with_capacity(4096);
        let mut b = String::with_capacity(4096);
        compare(&format!("transform/{label}"), 20000, |c| {
            if c {
                b.clear();
                transforms(bb(&mut b), bb(i), bb(r), bb(h));
                bb(&b);
            } else {
                a.clear();
                original::transforms(bb(&mut a), bb(i), bb(r), bb(h));
                bb(&a);
            }
        });
    }
    for (kind, label, speed) in [
        (0, "basic-integer", 600.0),
        (0, "basic-fraction", 1.25),
        (1, "mixed-integer", 600.0),
        (1, "mixed-fraction", 1.25),
    ] {
        compare(&format!("text/{label}"), 10000, |c| {
            drop(bb(if c {
                demo(bb(speed), bb(kind))
            } else {
                original::demo(bb(speed), bb(kind))
            }))
        });
    }
}

fn emit(out: &mut impl Write, s: &String) {
    out.write_all(&(s.len() as u64).to_le_bytes()).unwrap();
    out.write_all(&(s.capacity() as u64).to_le_bytes()).unwrap();
    out.write_all(s.as_bytes()).unwrap();
}
#[test]
#[ignore = "deterministic formatting bytes and capacities"]
fn dump_trace() {
    let mut out = std::io::BufWriter::new(
        std::fs::File::create(
            std::env::var_os("DEADSYNC_DISPLAY_NUMBERS_TRACE_OUT").expect("trace path"),
        )
        .unwrap(),
    );
    for value in i16::MIN..=i16::MAX {
        let mut s = String::with_capacity(64);
        integer(&mut s, value);
        emit(&mut out, &s);
        let mut s = String::with_capacity(64);
        floating(&mut s, f32::from(value));
        emit(&mut out, &s);
    }
    for a in 0..=255u8 {
        for b in 0..=255u8 {
            for c in 0..32u8 {
                if (u32::from(a) * 8192 + u32::from(b) * 32 + u32::from(c)) % 17 == 0 {
                    let mut s = String::with_capacity(512);
                    transforms(&mut s, a, b, c);
                    emit(&mut out, &s);
                }
            }
        }
    }
    for cap in 0..80 {
        for value in [
            -32768.0,
            -0.0,
            0.0,
            1.0,
            1.25,
            600.0,
            32767.0,
            32768.0,
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
        ] {
            let mut s = String::with_capacity(cap);
            s.push('x');
            floating(&mut s, value);
            emit(&mut out, &s);
        }
    }
    for kind in 0..3 {
        for value in [
            0.0,
            -0.0,
            1.0,
            1.25,
            600.0,
            32767.0,
            f32::NAN,
            f32::INFINITY,
        ] {
            emit(&mut out, &demo(value, kind));
        }
    }
    out.flush().unwrap();
}
