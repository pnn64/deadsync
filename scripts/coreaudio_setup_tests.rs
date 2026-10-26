use std::cell::{Cell, RefCell};
use std::fmt::{self, Write};
use std::hint::black_box;

type AudioDeviceID = u32;

// Same representation and field types as objc2-core-audio-types 0.3.2.
#[repr(C)]
#[derive(Clone, Copy)]
#[allow(non_snake_case)]
struct AudioValueRange {
    mMinimum: f64,
    mMaximum: f64,
}

#[derive(Clone, Copy)]
struct OutputLatency {
    frames: u32,
    rate_hz: u32,
    nanos: u64,
}

struct Prep {
    device_name: String,
    sample_rate_hz: u32,
    channels: usize,
    buffer_frames: u32,
    latency: OutputLatency,
}

struct LogBuffer {
    bytes: [u8; 8192],
    len: usize,
}

impl Write for LogBuffer {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        self.bytes[self.len..self.len + text.len()].copy_from_slice(text.as_bytes());
        self.len += text.len();
        Ok(())
    }
}

thread_local! {
    static RANGES: RefCell<Vec<AudioValueRange>> = const { RefCell::new(Vec::new()) };
    static QUERY_FAILED: Cell<bool> = const { Cell::new(false) };
    static QUERY_CALLS: Cell<usize> = const { Cell::new(0) };
    static NAME_CALLS: Cell<usize> = const { Cell::new(0) };
    static LOG_ENABLED: Cell<bool> = const { Cell::new(true) };
    static LOG: RefCell<LogBuffer> = const { RefCell::new(LogBuffer { bytes: [0; 8192], len: 0 }) };
}

macro_rules! warn {
    ($($args:tt)*) => {
        if LOG_ENABLED.get() {
            record_log(format_args!($($args)*));
        }
    };
}

macro_rules! info {
    ($($args:tt)*) => { warn!($($args)*); };
}

fn record_log(args: fmt::Arguments<'_>) {
    LOG.with_borrow_mut(|log| {
        log.len = 0;
        log.write_fmt(args).unwrap();
    });
}

fn last_log() -> String {
    LOG.with_borrow(|log| String::from_utf8(log.bytes[..log.len].to_vec()).unwrap())
}

fn get_available_sample_rates(_: AudioDeviceID) -> Result<Vec<AudioValueRange>, &'static str> {
    QUERY_CALLS.set(QUERY_CALLS.get() + 1);
    if QUERY_FAILED.get() {
        Err("fixture error")
    } else {
        Ok(RANGES.with_borrow(Clone::clone))
    }
}

fn get_device_name(id: u32) -> Result<String, ()> {
    NAME_CALLS.set(NAME_CALLS.get() + 1);
    if id == 9 {
        Err(())
    } else {
        Ok("USB 音声 DAC".to_owned())
    }
}

// Original formulas from 50524a532; queries and log text have separate checks.
fn reference_rates(ranges: &[AudioValueRange], default: u32) -> Vec<u32> {
    let mut result = Vec::new();
    if default > 0 {
        result.push(default);
    }
    for range in ranges {
        for value in [range.mMinimum, range.mMaximum] {
            if value.is_finite() && value > 0.0 {
                result.push(value.round().clamp(1.0, u32::MAX as f64) as u32);
            }
        }
    }
    for value in [
        48_000, 44_100, 96_000, 88_200, 192_000, 176_400, 32_000, 22_050, 16_000, 11_025, 384_000,
    ] {
        let rate = f64::from(value);
        if ranges
            .iter()
            .any(|r| rate + 0.5 >= r.mMinimum && rate - 0.5 <= r.mMaximum)
        {
            result.push(value);
        }
    }
    result.sort_unstable();
    result.dedup();
    result
}

fn set_ranges(values: &[(f64, f64)], failed: bool) {
    RANGES.set(
        values
            .iter()
            .map(|&(minimum, maximum)| AudioValueRange {
                mMinimum: minimum,
                mMaximum: maximum,
            })
            .collect(),
    );
    QUERY_FAILED.set(failed);
    QUERY_CALLS.set(0);
}

#[test]
fn supported_rates_match_original_for_adversarial_ranges() {
    for default in [0, 1, 44_100, 48_000, u32::MAX] {
        let values = [
            f64::NAN,
            f64::NEG_INFINITY,
            -1.0,
            -0.0,
            0.0,
            f64::MIN_POSITIVE,
            0.5,
            44_099.499_999,
            44_099.5,
            44_100.0,
            44_100.5,
            44_100.500_001,
            u32::MAX as f64,
            f64::MAX,
            f64::INFINITY,
        ];
        for min in values {
            for max in values {
                set_ranges(&[(min, max)], false);
                let expected = RANGES.with_borrow(|ranges| reference_rates(ranges, default));
                assert_eq!(supported_sample_rates_hz(7, default), expected);
                assert_eq!(QUERY_CALLS.get(), 1);
            }
        }
        set_ranges(&[], false);
        assert_eq!(
            supported_sample_rates_hz(7, default),
            if default > 0 { vec![default] } else { vec![] }
        );
        set_ranges(&[], true);
        assert_eq!(
            supported_sample_rates_hz(7, default),
            if default > 0 { vec![default] } else { vec![] }
        );
        assert_eq!(
            last_log(),
            "failed to query CoreAudio sample rates for device 7: fixture error"
        );
    }
    let mut seed = 0x3abc_5865_a12c_ff79_u64;
    for count in 0..96 {
        for _ in 0..25 {
            let mut next = || {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                seed
            };
            let ranges: Vec<_> = (0..count)
                .map(|_| {
                    let a = f64::from_bits(next());
                    let b = if next() & 1 == 0 {
                        a
                    } else {
                        f64::from_bits(next())
                    };
                    (a, b)
                })
                .collect();
            let default = next() as u32;
            set_ranges(&ranges, false);
            let expected = RANGES.with_borrow(|ranges| reference_rates(ranges, default));
            assert_eq!(supported_sample_rates_hz(7, default), expected);
        }
    }
}

#[test]
fn endpoint_rounding_matches_original_clamp_and_cast() {
    let mut actual = Vec::with_capacity(1);
    let mut seed = 0x5d12_3ace_7789_fe12_u64;
    for value in [
        f64::NAN,
        f64::NEG_INFINITY,
        -0.0,
        0.0,
        f64::MIN_POSITIVE,
        0.499_999_999,
        0.5,
        1.5,
        44_099.5,
        u32::MAX as f64 - 0.5,
        u32::MAX as f64 + 0.5,
        f64::MAX,
        f64::INFINITY,
    ]
    .into_iter()
    .chain((0..200_000).map(|_| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        f64::from_bits(seed)
    })) {
        actual.clear();
        push_sample_rate_hz(&mut actual, value);
        if value.is_finite() && value > 0.0 {
            assert_eq!(actual, [value.round().clamp(1.0, u32::MAX as f64) as u32]);
        } else {
            assert!(actual.is_empty());
        }
    }
}

#[test]
fn complete_common_rates_keep_extra_endpoints_and_missing_rate_tolerance() {
    for missing in 0..=COMMON_SAMPLE_RATES_HZ.len() {
        let mut ranges: Vec<_> = COMMON_SAMPLE_RATES_HZ
            .iter()
            .enumerate()
            .map(|(index, &rate)| {
                let value = f64::from(rate) + if index == missing { 0.5 } else { 0.0 };
                (value, value)
            })
            .collect();
        ranges.push((12_345.25, 67_890.75));
        ranges.reverse();
        for default in [0, 48_000, 1_234_567, u32::MAX] {
            set_ranges(&ranges, false);
            let expected = RANGES.with_borrow(|ranges| reference_rates(ranges, default));
            assert_eq!(supported_sample_rates_hz(7, default), expected);
        }
    }
}

fn prep(name: &str) -> Prep {
    Prep {
        device_name: name.to_owned(),
        sample_rate_hz: 48_000,
        channels: 2,
        buffer_frames: 128,
        latency: OutputLatency {
            frames: 480,
            rate_hz: 48_000,
            nanos: 12_666_666,
        },
    }
}

#[test]
fn startup_message_preserves_name_and_numeric_fields() {
    for name in [
        "",
        "Built-in Output",
        "USB 音声 DAC",
        "name with 'quotes' and\nnewlines",
    ] {
        let prep = prep(name);
        LOG_ENABLED.set(true);
        startup_log(&prep);
        assert_eq!(
            last_log(),
            format!(
                "CoreAudio '{}' using native shared output at 48000 Hz, 2 ch (buffer=128 frames, downstream latency=480 frames at 48000 Hz, total=12.7 ms).",
                name
            )
        );
        assert_eq!(prep.device_name, name);
    }
}

#[test]
fn fixed_log_sink_is_allocation_free() {
    perf::assert_no_churn(|| record_log(format_args!("音声 {} {:.1}", 7, 12.7)));
    assert_eq!(last_log(), "音声 7 12.7");
}

#[test]
fn refresh_messages_and_name_queries_match() {
    let current = OutputLatency {
        frames: 960,
        rate_hz: 48_000,
        nanos: 20_000_000,
    };
    for enabled in [false, true] {
        LOG_ENABLED.set(enabled);
        for (device, previous, prefix, calls) in [
            (7, 7, "CoreAudio latency compensation", 0),
            (
                7,
                2,
                "CoreAudio output changed to 'USB 音声 DAC'; latency compensation",
                1,
            ),
            (9, 2, "CoreAudio latency compensation", 1),
        ] {
            NAME_CALLS.set(0);
            refresh_log(device, previous, current, 10_000_000, 441, 44_100);
            assert_eq!(NAME_CALLS.get(), calls);
            if enabled {
                assert_eq!(
                    last_log(),
                    format!(
                        "{prefix} changed: 10.0 ms (441 frames at 44100 Hz) -> 20.0 ms (960 frames at 48000 Hz)."
                    )
                );
            }
        }
    }
    LOG_ENABLED.set(true);
}

#[test]
#[ignore = "CPU and allocation benchmark; run explicitly"]
fn benchmark_coreaudio_setup() {
    for (label, ranges, default, failed) in [
        ("rates_continuous", vec![(8000.0, 192000.0)], 48000, false),
        (
            "rates_discrete_2",
            vec![(44100.0, 44100.0), (48000.0, 48000.0)],
            48000,
            false,
        ),
        (
            "rates_discrete_11",
            COMMON_SAMPLE_RATES_HZ
                .iter()
                .map(|&rate| (f64::from(rate), f64::from(rate)))
                .collect(),
            48000,
            false,
        ),
        (
            "rates_ranges_32",
            (0..32)
                .map(|i| (i as f64 * 10000.0 + 1000.0, i as f64 * 10000.0 + 9999.0))
                .collect(),
            48000,
            false,
        ),
        ("rates_error_default", vec![], 48000, true),
        ("rates_error_empty", vec![], 0, true),
    ] {
        set_ranges(&ranges, failed);
        LOG_ENABLED.set(false);
        let (_, counts) = perf::measure(|| {
            black_box(supported_sample_rates_hz(black_box(7), black_box(default)));
        });
        println!("{label}: allocation {counts:?}");
        kernel_bench::measure(label, 8192, || {
            black_box(supported_sample_rates_hz(black_box(7), black_box(default)));
        });
    }
    for (label, value) in [
        ("endpoint_fractional", 44_099.5),
        ("endpoint_saturating", f64::MAX),
    ] {
        let mut output = Vec::with_capacity(1);
        let mut work = || {
            output.clear();
            push_sample_rate_hz(&mut output, black_box(value));
            black_box(output.as_slice());
        };
        let (_, counts) = perf::measure(&mut work);
        println!("{label}: allocation {counts:?}");
        kernel_bench::measure(label, 262_144, work);
    }
    for (label, name) in [
        ("startup_name_short", "Built-in Output".to_owned()),
        ("startup_name_long", "音声 DAC ".repeat(128)),
    ] {
        let prep = prep(&name);
        LOG_ENABLED.set(true);
        let (_, counts) = perf::measure(|| startup_log(black_box(&prep)));
        println!("{label}: allocation {counts:?}");
        kernel_bench::measure(label, 8192, || startup_log(black_box(&prep)));
    }
    let current = OutputLatency {
        frames: 960,
        rate_hz: 48000,
        nanos: 20_000_000,
    };
    for (label, device, previous, enabled) in [
        ("refresh_same", 7, 7, true),
        ("refresh_changed", 7, 2, true),
        ("refresh_name_error", 9, 2, true),
        ("refresh_disabled", 7, 7, false),
    ] {
        LOG_ENABLED.set(enabled);
        let work = || {
            refresh_log(
                black_box(device),
                black_box(previous),
                black_box(current),
                black_box(10_000_000),
                black_box(441),
                black_box(44100),
            )
        };
        let (_, counts) = perf::measure(work);
        println!("{label}: allocation {counts:?}");
        kernel_bench::measure(label, 8192, work);
    }
}
