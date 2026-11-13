//! Hardware-free checks and CPU-cycle probes of the production input kernels.
#![allow(dead_code)]

#[cfg(all(windows, not(target_vendor = "win7")))]
use deadlib_input_native::InputThreadPolicy;
use deadlib_input_native::uuid_from_bytes;
#[cfg(windows)]
use deadlib_input_native::{
    BackendHost, GpSystemEvent, PadBackend, PadOrderBackend, emit_dir_edges,
};
#[path = "../../../tests/support/allocations.rs"]
mod perf;


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


#[cfg(all(windows, not(target_vendor = "win7")))]
mod wgi {
    include!("../src/backend/wgi.rs");

    mod kernels {
        use super::*;


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
        fn axis_scaling_matches_floating_clamp_at_quantization_boundaries() {
            let check = |value: f64| {
                let finite = if value.is_finite() { value } else { 0.0 };
                assert_eq!(
                    scale_axis(value),
                    (finite.max(-1.0) * 32767.0) as i16,
                    "axis bits={:016x}",
                    value.to_bits()
                );
                assert_eq!(
                    scale_trigger(value),
                    (finite.max(0.0) * 32767.0) as i16,
                    "trigger bits={:016x}",
                    value.to_bits()
                );
            };
            for n in i16::MIN..=i16::MAX {
                let value = f64::from(n) / 32767.0;
                check(value);
                check(value.next_up());
                check(value.next_down());
            }
            for value in [1.0 / 32768.0, -1.0 / 32768.0] {
                check(value);
                check(value.next_up());
                check(value.next_down());
            }
            let mut seed = 79u64;
            for _ in 0..1_000_000 {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                check(f64::from_bits(seed));
            }
            for value in [
                f64::INFINITY,
                f64::NEG_INFINITY,
                f64::NAN,
                -0.0,
                0.0,
                f64::MAX,
                f64::MIN,
                f64::MIN_POSITIVE,
                -f64::MIN_POSITIVE,
            ] {
                check(value);
            }
        }

    }
}

#[cfg(windows)]
mod rawinput {
    include!("../src/backend/w32_raw_input.rs");

    mod kernels {
        use super::*;


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
        fn uuid_hex_preserves_leading_zeroes_and_lowercase() {
            for byte in 0..=255 {
                let uuid = [byte; 16];
                let expected: String = uuid.iter().map(|b| format!("{b:02x}")).collect();
                assert_eq!(uuid_hex(&uuid), expected);
            }
            for n in 0..256u32 {
                let uuid = crate::uuid_from_bytes(&n.to_le_bytes());
                let expected: String = uuid.iter().map(|b| format!("{b:02x}")).collect();
                assert_eq!(uuid_hex(&uuid), expected);
            }
        }


        #[test]
        fn serial_identity_preserves_formatted_bytes_for_all_vendor_values() {
            let long = "abé\0🔔".repeat(64);
            let serials = ["", "00001234", "é🔔\0空", long.as_str()];
            for vendor in 0u16..=u16::MAX {
                let product = vendor.rotate_left(7);
                let serial = serials[vendor as usize % serials.len()];
                let expected = crate::uuid_from_bytes(
                    format!("rawinput:serial:{vendor:04x}:{product:04x}:{serial}").as_bytes(),
                );
                assert_eq!(
                    rawinput_uuid(vendor, product, Some(serial), "ignored"),
                    expected
                );
            }
        }

    }
}
