use std::cell::Cell;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OutputTimingQuality {
    Trusted,
    Fallback,
}

#[derive(Clone, Copy)]
struct Timebase {
    numer: u32,
    denom: u32,
}

thread_local! {
    static NOW_VALUES: Cell<[u64; 2]> = const { Cell::new([0; 2]) };
    static NOW_CALLS: Cell<usize> = const { Cell::new(0) };
    static FALLBACK_NOTE: Cell<Option<(u64, bool)>> = const { Cell::new(None) };
    static TRACE_ENABLED: Cell<bool> = const { Cell::new(false) };
}

mod log {
    pub enum Level {
        Trace,
    }

    macro_rules! log_enabled {
        ($level:expr) => {{
            let _ = $level;
            $crate::TRACE_ENABLED.get()
        }};
    }
    pub(crate) use log_enabled;
}

fn now_nanos() -> u64 {
    let index = NOW_CALLS.get();
    NOW_CALLS.set(index + 1);
    NOW_VALUES.get()[index]
}

fn note_output_clock_fallback(nanos: u64, trace: bool) {
    assert!(FALLBACK_NOTE.get().is_none());
    FALLBACK_NOTE.set(Some((nanos, trace)));
}

#[cfg(test)]
mod host_clock_tests {
    use super::*;
    use std::hint::black_box;

    fn reference(time: u64, info: Timebase, calibrated_mach: u64, host_mid: u64) -> u64 {
        let scaled = ((u128::from(time) * u128::from(info.numer)) / u128::from(info.denom))
            .min(u128::from(u64::MAX)) as u64;
        let offset = i128::from(calibrated_mach) - i128::from(host_mid);
        (i128::from(scaled) - offset).clamp(0, i128::from(u64::MAX)) as u64
    }

    #[test]
    fn calibrated_offset_preserves_full_u64_domain_and_saturation() {
        let values = [
            0,
            1,
            2,
            u64::MAX / 2,
            u64::MAX / 2 + 1,
            u64::MAX - 1,
            u64::MAX,
        ];
        for info in [
            Timebase { numer: 1, denom: 1 },
            Timebase {
                numer: 125,
                denom: 3,
            },
            Timebase {
                numer: u32::MAX,
                denom: 7,
            },
        ] {
            for calibrated in values {
                for host_mid in values {
                    let clock = clock_from_calibration(calibrated, host_mid, info);
                    for time in values.into_iter().filter(|&value| value != 0) {
                        assert_eq!(
                            clock.callback_nanos(time),
                            (
                                reference(time, info, calibrated, host_mid),
                                OutputTimingQuality::Trusted
                            )
                        );
                    }
                }
            }
        }
        let mut seed = 0x6712_5734_fa2b_bc35_u64;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        for _ in 0..200_000 {
            let calibrated = next();
            let host_mid = next();
            let info = Timebase {
                numer: next() as u32,
                denom: (next() as u32).max(1),
            };
            let time = next().max(1);
            let clock = clock_from_calibration(calibrated, host_mid, info);
            assert_eq!(
                clock.callback_nanos(time),
                (
                    reference(time, info, calibrated, host_mid),
                    OutputTimingQuality::Trusted
                )
            );
        }
    }

    #[test]
    fn zero_timestamp_keeps_both_host_reads_and_fallback_note() {
        for trace in [false, true] {
            for now in [[0, 0], [123, 456], [u64::MAX, u64::MAX]] {
                let clock = clock_from_calibration(
                    u64::MAX,
                    0,
                    Timebase {
                        numer: 125,
                        denom: 3,
                    },
                );
                NOW_VALUES.set(now);
                NOW_CALLS.set(0);
                FALLBACK_NOTE.set(None);
                TRACE_ENABLED.set(trace);
                assert_eq!(
                    clock.callback_nanos(0),
                    (now[1], OutputTimingQuality::Fallback)
                );
                assert_eq!(NOW_CALLS.get(), 2);
                assert_eq!(FALLBACK_NOTE.get(), Some((now[0], trace)));
            }
        }
    }

    #[test]
    fn trusted_clock_conversion_has_no_allocation_churn() {
        let clock = clock_from_calibration(
            12_345_678,
            9_876_543,
            Timebase {
                numer: 125,
                denom: 3,
            },
        );
        perf::assert_no_churn(|| {
            black_box(clock.callback_nanos(black_box(24_000_000_000)));
        });
    }

    #[test]
    #[ignore = "CPU benchmark; run explicitly in release mode"]
    fn benchmark_host_clock() {
        println!(
            "host_clock_state_bytes: {}",
            size_of::<CoreAudioHostClock>()
        );
        for (label, calibrated, host_mid, numer, denom) in [
            ("host_positive_unit", 2_000_000, 1_000_000, 1, 1),
            ("host_negative_unit", 1_000_000, 2_000_000, 1, 1),
            ("host_positive_thirds", 2_000_000, 1_000_000, 125, 3),
            ("host_negative_thirds", 1_000_000, 2_000_000, 125, 3),
            ("host_positive_generic", 2_000_000, 1_000_000, 125, 7),
            ("host_negative_generic", 1_000_000, 2_000_000, 125, 7),
        ] {
            let clock = black_box(clock_from_calibration(
                calibrated,
                host_mid,
                Timebase { numer, denom },
            ));
            let (_, counts) = perf::measure(|| {
                black_box(clock.callback_nanos(black_box(24_000_000_000)));
            });
            println!("{label}: allocation {counts:?}");
            kernel_bench::measure(label, 1024, || {
                for index in 0..256 {
                    black_box(clock.callback_nanos(black_box(24_000_000_000 + index)));
                }
            });
        }
    }
}
