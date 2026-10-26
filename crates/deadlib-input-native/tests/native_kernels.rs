//! Hardware-free checks and CPU-cycle probes of the production input kernels.
#![allow(dead_code)]

#[cfg(all(windows, not(target_vendor = "win7")))]
use deadlib_input_native::InputThreadPolicy;
use deadlib_input_native::uuid_from_bytes;
#[cfg(windows)]
use deadlib_input_native::{
    BackendHost, GpSystemEvent, PadBackend, PadOrderBackend, emit_dir_edges,
};
#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;
#[path = "../../../tests/support/perf.rs"]
mod perf;

use std::hint::black_box;

fn probe(label: &str, iterations: usize, mut work: impl FnMut()) {
    let (_, churn) = perf::measure(&mut work);
    assert_eq!(churn, perf::Churn::default());
    println!("native/{label}: allocation {churn:?}");
    bench::measure(&format!("native/{label}"), iterations, work);
}

fn reference_uuid(bytes: &[u8]) -> [u8; 16] {
    let hash = |offset| {
        bytes.iter().fold(offset, |h: u64, &b| {
            (h ^ u64::from(b)).wrapping_mul(0x100000001b3)
        })
    };
    let mut out = [0; 16];
    out[..8].copy_from_slice(&hash(0xcbf29ce484222325).to_le_bytes());
    out[8..].copy_from_slice(&hash(0xaf63dc4c8601ec8c).to_le_bytes());
    out
}

#[test]
fn uuid_preserves_persisted_identity_bytes() {
    let mut bytes = Vec::new();
    let mut seed = 53u32;
    for _ in 0..4096 {
        assert_eq!(uuid_from_bytes(&bytes), reference_uuid(&bytes));
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        bytes.push((seed >> 24) as u8);
    }
    for byte in 0..=255 {
        assert_eq!(uuid_from_bytes(&[byte; 64]), reference_uuid(&[byte; 64]));
    }
}

#[test]
#[ignore = "release CPU-cycle and allocation benchmark"]
fn benchmark_uuid() {
    for len in [0, 8, 32, 96, 256, 1024] {
        let inputs: Vec<Vec<u8>> = (0..64)
            .map(|n| (0..len).map(|i| (i * 73 + n * 19) as u8).collect())
            .collect();
        probe(&format!("uuid/len={len}"), 1000, || {
            for bytes in &inputs {
                black_box(uuid_from_bytes(black_box(bytes)));
            }
        });
    }
}

#[cfg(all(windows, not(target_vendor = "win7")))]
mod wgi {
    include!("../src/backend/wgi.rs");

    mod kernels {
        use super::*;
        use crate::black_box;

        #[test]
        fn mapped_edges_preserve_order_and_ignore_unmapped_bits() {
            // Every subset of changed mapped buttons, with both directions of
            // each edge and with unmapped/d-pad bits mixed into the snapshots.
            for changed in 0u32..1024 {
                let mask = BTN_MAP.iter().enumerate().fold(0, |bits, (i, (m, _))| {
                    bits | if changed & (1 << i) != 0 { m.0 } else { 0 }
                });
                for previous in [0, u32::MAX, 0x55555555, 0xaaaaaaaa] {
                    let current = previous ^ mask ^ 0xffffc3c0;
                    let mut expected = Vec::new();
                    for (m, code) in BTN_MAP {
                        if (previous ^ current) & m.0 != 0 {
                            expected.push((code, current & m.0 != 0));
                        }
                    }
                    let mut actual = Vec::new();
                    gamepad_button_edges(
                        GamepadButtons(previous),
                        GamepadButtons(current),
                        |c, p| actual.push((c, p)),
                    );
                    assert_eq!(actual, expected);
                }
            }
        }

        #[test]
        #[ignore = "release CPU-cycle and allocation benchmark"]
        fn benchmark_wgi_edges() {
            for kind in ["unchanged", "dpad", "single", "pair", "dense"] {
                let readings: Vec<_> = (0..256)
                    .map(|n| {
                        let previous = (n as u32).wrapping_mul(0x9e3779b9);
                        let mask = match kind {
                            "unchanged" => 0,
                            "dpad" => 0x3c0,
                            "single" => BTN_MAP[n % 10].0.0,
                            "pair" => BTN_MAP[n % 10].0.0 | BTN_MAP[(n + 3) % 10].0.0,
                            _ => 0x3c3f,
                        };
                        (GamepadButtons(previous), GamepadButtons(previous ^ mask))
                    })
                    .collect();
                crate::probe(&format!("wgi/{kind}"), 1000, || {
                    for &(previous, current) in &readings {
                        gamepad_button_edges(
                            black_box(previous),
                            black_box(current),
                            |code, pressed| {
                                black_box((code, pressed));
                            },
                        );
                    }
                });
            }
        }
    }
}

#[cfg(windows)]
mod rawinput {
    include!("../src/backend/w32_raw_input.rs");

    mod kernels {
        use super::*;
        use crate::black_box;

        fn device(previous: &[u16]) -> Dev {
            Dev {
                id: PadId(13),
                name: String::new(),
                vendor_id: None,
                product_id: None,
                uuid: [37; 16],
                preparsed: Vec::new(),
                max_buttons: 0,
                buttons_prev: previous.into(),
                buttons_prev_len: previous.len(),
                buttons_now: Box::new([]),
                hat_min: 0,
                hat_max: 7,
                dir: [false; 4],
                last_report: Vec::new(),
            }
        }

        #[test]
        fn sorted_usage_diff_preserves_all_event_fields_and_duplicate_order() {
            let timestamp = Instant::now();
            let mut seed = 89u32;
            for n in 0..10000 {
                let mut usages = || {
                    (0..n % 129)
                        .filter_map(|_| {
                            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                            (seed & 3 != 0).then_some(((seed >> 24) % 24) as u16)
                        })
                        .collect::<Vec<_>>()
                };
                let mut previous = usages();
                let mut current = usages();
                previous.sort_unstable();
                current.sort_unstable();
                match n % 5 {
                    0 => current.clone_from(&previous),
                    1 => {
                        current.clone_from(&previous);
                        if let Some(last) = current.last_mut() {
                            *last = u16::MAX;
                        }
                    }
                    2 => {
                        current.clone_from(&previous);
                        current.truncate(current.len() / 2);
                    }
                    _ => (),
                }
                let mut expected = Vec::new();
                let (mut a, mut b) = (0, 0);
                while a < previous.len() || b < current.len() {
                    match (previous.get(a), current.get(b)) {
                        (Some(p), Some(c)) if p == c => {
                            a += 1;
                            b += 1;
                        }
                        (Some(&p), c) if c.is_none_or(|&c| p < c) => {
                            expected.push((p, false));
                            a += 1;
                        }
                        (_, Some(&c)) => {
                            expected.push((c, true));
                            b += 1;
                        }
                        _ => unreachable!(),
                    }
                }
                let dev = device(&previous);
                let mut actual = Vec::new();
                emit_button_diff(
                    &mut |event| {
                        let PadEvent::RawButton {
                            id,
                            timestamp: ts,
                            host_nanos,
                            code,
                            uuid,
                            value,
                            pressed,
                        } = event
                        else {
                            panic!("unexpected event");
                        };
                        assert_eq!(
                            (id, ts, host_nanos, uuid),
                            (dev.id, timestamp, 71, dev.uuid)
                        );
                        assert_eq!(code.0 >> 16, 9);
                        assert_eq!(
                            value.to_bits(),
                            if pressed { 1.0f32 } else { 0.0f32 }.to_bits()
                        );
                        actual.push((code.0 as u16, pressed));
                    },
                    &dev,
                    timestamp,
                    71,
                    &current,
                );
                assert_eq!(actual, expected);
            }
        }

        #[test]
        #[ignore = "release CPU-cycle and allocation benchmark"]
        fn benchmark_hid_diff() {
            let timestamp = Instant::now();
            for len in [0, 1, 4, 16, 64] {
                let previous: Vec<_> = (0..len as u16).map(|n| n * 2).collect();
                let dev = device(&previous);
                for kind in ["same", "last", "all", "release"] {
                    let mut current = previous.clone();
                    match kind {
                        "last" => {
                            if let Some(last) = current.last_mut() {
                                *last += 1;
                            }
                        }
                        "all" => {
                            for c in &mut current {
                                *c += 1;
                            }
                        }
                        "release" => current.clear(),
                        _ => (),
                    }
                    crate::probe(&format!("hid/{kind}/len={len}"), 1000, || {
                        for _ in 0..256 {
                            emit_button_diff(
                                &mut |event| {
                                    black_box(event);
                                },
                                black_box(&dev),
                                timestamp,
                                71,
                                black_box(&current),
                            );
                        }
                    });
                }
            }
        }
    }
}
