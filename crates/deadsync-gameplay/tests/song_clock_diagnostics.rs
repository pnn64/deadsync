//! Timestamp/log equivalence and an isolated benchmark of the diagnostic wrapper.
use deadsync_gameplay::{SongClockSnapshot, music_time_ns_from_song_clock};
use std::cell::RefCell;
use std::fmt::Write;
use std::hint::black_box;
use std::sync::{Mutex, Once};
use std::time::{Duration, Instant};

#[path = "song_clock_diagnostics/baseline.rs"]
mod baseline;
#[path = "../../../tests/support/perf.rs"]
#[allow(dead_code)]
mod perf;

type Messages = Vec<(log::Level, String)>;
thread_local! {
    static CAPTURE: RefCell<Option<Messages>> = const { RefCell::new(None) };
}
static LOGGER: Logger = Logger;
static INIT: Once = Once::new();
static SERIAL: Mutex<()> = Mutex::new(());

struct Logger;
struct LogBuffer {
    bytes: [u8; 512],
    len: usize,
}

impl Write for LogBuffer {
    fn write_str(&mut self, text: &str) -> std::fmt::Result {
        let end = self.len + text.len();
        let destination = self.bytes.get_mut(self.len..end).ok_or(std::fmt::Error)?;
        destination.copy_from_slice(text.as_bytes());
        self.len = end;
        Ok(())
    }
}

impl log::Log for Logger {
    fn enabled(&self, _: &log::Metadata<'_>) -> bool {
        true
    }

    fn log(&self, record: &log::Record<'_>) {
        CAPTURE.with(|capture| {
            if let Some(messages) = capture.borrow_mut().as_mut() {
                messages.push((record.level(), record.args().to_string()));
            } else {
                // Format every byte without introducing allocation or file I/O.
                let mut buffer = LogBuffer {
                    bytes: [0; 512],
                    len: 0,
                };
                buffer.write_fmt(*record.args()).expect("diagnostic fits");
                black_box(&buffer.bytes[..buffer.len]);
            }
        });
    }

    fn flush(&self) {}
}

fn init_logger() {
    INIT.call_once(|| log::set_logger(&LOGGER).expect("test logger"));
}

fn capture(work: impl FnOnce() -> i64) -> (i64, Messages) {
    CAPTURE.with(|capture| assert!(capture.replace(Some(Vec::new())).is_none()));
    let value = work();
    let messages = CAPTURE.with(|capture| capture.replace(None).expect("capturing"));
    (value, messages)
}

#[test]
fn timestamps_and_diagnostic_messages_match_baseline() {
    let _serial = SERIAL.lock().unwrap();
    init_logger();
    let base = Instant::now();
    let rates = [
        0.0,
        -0.0,
        -1.0,
        0.5,
        1.0,
        1.25,
        2.0,
        f32::MIN_POSITIVE,
        f32::from_bits(1),
        1_000_000.0,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NAN,
        f32::from_bits(0xffc0_0001),
    ];
    let mut cases = 0;
    for level in [log::LevelFilter::Info, log::LevelFilter::Debug] {
        log::set_max_level(level);
        for diagnostics in [false, true] {
            for time in [
                0,
                3_600_000_100_000,
                8_192_000_400_000,
                -8_192_000_400_000,
                i64::MIN,
                i64::MIN + 1,
                i64::MAX,
            ] {
                for rate in rates {
                    for offset in [-24_000_000_i64, -1, 0, 1, 20_833, 5_000_000] {
                        let elapsed = Duration::from_nanos(offset.unsigned_abs());
                        let captured_at = if offset < 0 {
                            base - elapsed
                        } else {
                            base + elapsed
                        };
                        for (anchor, captured_host_nanos) in [
                            (2_000_000_000, (2_000_000_000_i64 + offset) as u64),
                            (0, 0),
                            (0, 2_000_000_000),
                            (2_000_000_000, 0),
                            (u64::MAX, 1),
                            (1, u64::MAX),
                        ] {
                            let snapshot = SongClockSnapshot {
                                song_time_ns: time,
                                seconds_per_second: rate,
                                mapped_audio: anchor != 0,
                                valid_at: base,
                                valid_at_host_nanos: anchor,
                                timing_diag_enabled: diagnostics,
                                timing_diag_callback_gap_ns: if offset < 0 {
                                    12_500_001
                                } else {
                                    u64::MAX
                                },
                            };
                            let old = capture(|| {
                                baseline::music_time_ns_from_song_clock(
                                    snapshot,
                                    captured_at,
                                    captured_host_nanos,
                                )
                            });
                            let new = capture(|| {
                                music_time_ns_from_song_clock(
                                    snapshot,
                                    captured_at,
                                    captured_host_nanos,
                                )
                            });
                            assert_eq!(
                                old, new,
                                "time={time} rate={rate:?} offset={offset} anchor={anchor} captured={captured_host_nanos}"
                            );
                            assert_eq!(
                                new.1.len(),
                                usize::from(diagnostics && level == log::LevelFilter::Debug)
                            );
                            cases += 1;
                        }
                    }
                }
            }
        }
    }
    eprintln!("{cases} timestamp/log-message comparisons passed");
}

fn measure_variant(
    name: &str,
    iterations: usize,
    input: (SongClockSnapshot, Instant, u64),
    mut clock: impl FnMut(SongClockSnapshot, Instant, u64) -> i64,
) {
    for _ in 0..1024 {
        let (snapshot, captured_at, captured_host) = black_box(input);
        black_box(clock(snapshot, captured_at, captured_host));
    }
    perf::measure_sampled(name, iterations, 1, || {
        let (snapshot, captured_at, captured_host) = black_box(input);
        clock(snapshot, captured_at, captured_host)
    });
}

#[test]
#[ignore = "manual release benchmark: thread cycles, allocations, and calls/second"]
fn song_clock_diagnostics_bench() {
    let _serial = SERIAL.lock().unwrap();
    init_logger();
    #[cfg(windows)]
    if let Ok(mask) = std::env::var("DEADSYNC_PERF_AFFINITY") {
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn GetCurrentThread() -> *mut std::ffi::c_void;
            fn SetThreadAffinityMask(thread: *mut std::ffi::c_void, mask: usize) -> usize;
        }
        let mask = mask.parse().expect("decimal affinity mask");
        // SAFETY: Windows validates the mask for this live thread's pseudo-handle.
        assert_ne!(
            unsafe { SetThreadAffinityMask(GetCurrentThread(), mask) },
            0
        );
    }
    let reverse = std::env::var_os("DEADSYNC_PERF_REVERSE").is_some();
    let base = Instant::now();
    for (diagnostics, level, label) in [
        (false, log::LevelFilter::Debug, "off"),
        (true, log::LevelFilter::Info, "filtered"),
        (true, log::LevelFilter::Debug, "formatted"),
    ] {
        log::set_max_level(level);
        let iterations = if label == "formatted" {
            16_384
        } else {
            1_048_576
        };
        for path in ["host", "instant-past", "instant-future"] {
            let snapshot = SongClockSnapshot {
                song_time_ns: 8_192_000_400_000,
                seconds_per_second: 1.25,
                mapped_audio: true,
                valid_at: base,
                valid_at_host_nanos: if path == "host" { 20_000_000_000 } else { 0 },
                timing_diag_enabled: diagnostics,
                timing_diag_callback_gap_ns: 5_000_000,
            };
            let captured_at = if path == "instant-future" {
                base + Duration::from_millis(5)
            } else {
                base - Duration::from_millis(5)
            };
            let input = (snapshot, captured_at, 19_995_000_000);
            for old in if reverse {
                [false, true]
            } else {
                [true, false]
            } {
                let name = format!("{path}/{label}/{}", if old { "old" } else { "new" });
                if old {
                    measure_variant(
                        &name,
                        iterations,
                        input,
                        baseline::music_time_ns_from_song_clock,
                    );
                } else {
                    measure_variant(&name, iterations, input, music_time_ns_from_song_clock);
                }
            }
        }
    }
}
