use super::moves_perf::bench;
use super::*;
use crate::GamepadCodeBinding;
use crate::keymap::perf_tests::allocations as perf;
use std::io::Write;
#[allow(dead_code, unused_imports)]
#[path = "heap_original.rs"]
mod original;
#[path = "gamepad_token_original.rs"]
mod original_token;
include!("heap_adapter.rs");
#[test]
fn heap_layout_deadlines_indices_and_capacities_match() {
    let base = std::time::Instant::now();
    for n in [1, 2, 3, 4, 8, 31, 32, 127, 128, 512] {
        let mut a = original::Fixture::new(n, base);
        let mut e = Fixture::new(n, base);
        let mut seed = 0x1234_5678_9abc_def0u64;
        for step in 0..8192 {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            let slot = seed as usize % n;
            let tick = (seed & 3 != 0).then_some((seed >> 32) % 17);
            a.update(slot, tick);
            e.update(slot, tick);
            if step % 7 == 0 {
                a.cancel_root();
                e.cancel_root();
            }
            let expected = a.snapshot();
            assert_eq!(expected, e.snapshot(), "n={n}, step={step}");
        }
    }
}
#[test]
fn token_bytes_capacities_and_append_growth_match() {
    for code in
        (0..=u16::MAX)
            .map(u32::from)
            .chain([0x1234_5678, u32::MAX, 0x8000_0000, 0x0101_0101])
    {
        for (device, uuid) in [(None, None), (Some(usize::MAX), Some([255; 16]))] {
            let binding = GamepadCodeBinding {
                code_u32: code,
                device,
                uuid,
            };
            let a = original_token::gamepad_code_binding_to_token(binding);
            let b = crate::gamepad_code_binding_to_token(binding);
            assert_eq!(a, b);
            assert_eq!(a.capacity(), b.capacity());
        }
    }
    for capacity in 0..160 {
        for len in 0..=capacity {
            for code in [0, 1, 15, 16, 255, 256, 0x1234_5678, u32::MAX] {
                for device in [None, Some(0), Some(1), Some(12345), Some(usize::MAX)] {
                    for uuid in [None, Some([0; 16]), Some([255; 16])] {
                        let binding = GamepadCodeBinding {
                            code_u32: code,
                            device,
                            uuid,
                        };
                        let mut a = String::with_capacity(capacity);
                        a.extend(std::iter::repeat_n('x', len));
                        let mut b = String::with_capacity(capacity);
                        b.push_str(&a);
                        original_token::write_gamepad_code_binding_token(&mut a, binding);
                        crate::write_gamepad_code_binding_token(&mut b, binding);
                        assert_eq!(a, b);
                        assert_eq!(
                            a.capacity(),
                            b.capacity(),
                            "cap={capacity} len={len} {binding:?}"
                        );
                    }
                }
            }
        }
    }
}
fn benchmark_token() {
    use std::hint::black_box as bb;
    for (label, binding) in [
        (
            "zero",
            GamepadCodeBinding {
                code_u32: 0,
                device: None,
                uuid: None,
            },
        ),
        (
            "max",
            GamepadCodeBinding {
                code_u32: u32::MAX,
                device: None,
                uuid: None,
            },
        ),
        (
            "device",
            GamepadCodeBinding {
                code_u32: 0xff,
                device: Some(usize::MAX),
                uuid: None,
            },
        ),
        (
            "uuid",
            GamepadCodeBinding {
                code_u32: 0x1234_5678,
                device: None,
                uuid: Some([0xab; 16]),
            },
        ),
        (
            "full",
            GamepadCodeBinding {
                code_u32: 0x1234_5678,
                device: Some(usize::MAX),
                uuid: Some([0xcd; 16]),
            },
        ),
    ] {
        let label = format!("token/{label}");
        let work = |c| {
            drop(bb(if c {
                crate::gamepad_code_binding_to_token(bb(binding))
            } else {
                original_token::gamepad_code_binding_to_token(bb(binding))
            }))
        };
        for (c, name) in [(false, "original"), (true, "current")] {
            let (_, churn) = perf::measure(|| work(c));
            println!("{label}/{name}: allocation {churn:?}");
        }
        bench::compare(&label, 20000, work);
        let mut a = String::with_capacity(4096);
        let mut b = String::with_capacity(4096);
        let label = label.replace("token/", "append/");
        let mut work = |c| {
            if c {
                b.clear();
                crate::write_gamepad_code_binding_token(bb(&mut b), bb(binding));
                bb(&b);
            } else {
                a.clear();
                original_token::write_gamepad_code_binding_token(bb(&mut a), bb(binding));
                bb(&a);
            }
        };
        for (c, name) in [(false, "original"), (true, "current")] {
            let (_, churn) = perf::measure(|| work(c));
            println!("{label}/{name}: allocation {churn:?}");
        }
        bench::compare(&label, 30000, work);
    }
}

#[test]
fn complete_debounce_cycles_preserve_edges_and_cleanup() {
    let base = Instant::now();
    for n in [1, 4, 8, 32, 128] {
        let mut a = original::EventFixture::new(n, base);
        let mut b = EventFixture::new(n, base);
        for _ in 0..128 {
            assert_eq!(a.trace(), b.trace(), "n={n}");
        }
    }
}
fn compare(label: &str, repetitions: usize, mut work: impl FnMut(bool)) {
    for (c, name) in [(false, "original"), (true, "current")] {
        let (_, churn) = perf::measure(|| work(c));
        println!("{label}/{name}: allocation {churn:?}");
    }
    bench::compare(label, repetitions, work);
}
fn benchmark_heap() {
    for n in [1usize, 4, 8, 32, 128, 512] {
        for (kind, name) in [(0, "up"), (1, "down"), (2, "drain"), (3, "cancel")] {
            let base = Instant::now();
            let mut a = original::Fixture::new(n, base);
            let mut b = Fixture::new(n, base);
            compare(&format!("heap/{name}/{n}"), (50000 / n).max(128), |c| {
                if c { b.run(kind) } else { a.run(kind) }
            });
        }
    }
    for n in [1usize, 4, 8, 32, 128] {
        let base = Instant::now();
        let mut a = original::EventFixture::new(n, base);
        let mut b = EventFixture::new(n, base);
        compare(&format!("event/{n}"), (10000 / n).max(64), |c| {
            if c {
                b.run(|edge| {
                    std::hint::black_box(edge);
                })
            } else {
                a.run(|edge| {
                    std::hint::black_box(edge);
                })
            }
        });
    }
}
fn emit(out: &mut impl Write, words: &[u64]) {
    out.write_all(&(words.len() as u64).to_le_bytes()).unwrap();
    for word in words {
        out.write_all(&word.to_le_bytes()).unwrap();
    }
}
#[test]
#[ignore = "deterministic exact heap layout and complete edge trace"]
fn dump_trace() {
    let mut out = std::io::BufWriter::new(
        std::fs::File::create(
            std::env::var_os("DEADSYNC_INPUT_HEAP_TRACE_OUT").expect("trace path"),
        )
        .unwrap(),
    );
    let base = Instant::now();
    for n in [1, 2, 3, 4, 8, 31, 32, 127, 128, 512] {
        let mut f = Fixture::new(n, base);
        let mut seed = 0x1234_5678_9abc_def0u64;
        for step in 0..8192 {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            f.update(
                seed as usize % n,
                (seed & 3 != 0).then_some((seed >> 32) % 17),
            );
            if step % 7 == 0 {
                f.cancel_root();
            }
            if step % 17 == 0 {
                emit(&mut out, &f.snapshot());
            }
        }
    }
    for n in [1, 4, 8, 32, 128] {
        let mut f = EventFixture::new(n, base);
        for _ in 0..128 {
            emit(&mut out, &f.trace());
        }
    }
    for capacity in 0..80 {
        for code in [0, 1, 15, 16, 255, 256, 0x1234_5678, u32::MAX] {
            for device in [None, Some(0), Some(usize::MAX)] {
                for uuid in [None, Some([255; 16])] {
                    let binding = GamepadCodeBinding {
                        code_u32: code,
                        device,
                        uuid,
                    };
                    let mut s = String::with_capacity(capacity);
                    crate::write_gamepad_code_binding_token(&mut s, binding);
                    emit(&mut out, &[s.capacity() as u64, s.len() as u64]);
                    out.write_all(s.as_bytes()).unwrap();
                }
            }
        }
    }
    out.flush().unwrap();
}

#[test]
#[ignore = "paired heap/event and fixed-width gamepad output measurements"]
fn benchmark() {
    benchmark_heap();
    benchmark_token();
}
