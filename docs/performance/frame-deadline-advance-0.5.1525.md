# Advance ordinary redraw deadlines directly

Baseline: `1d0d58cbf`.

`deadlib-platform::frame_pacing::advance_redraw_deadline` previously calculated
elapsed nanoseconds, divided by the interval using 128-bit integers, and
multiplied the interval by the resulting step count for every due deadline.
When less than one interval late, the step count is always one.

The function now tries the next cadence deadline directly. It returns that
deadline only when addition succeeds and the result is strictly after `now`.
Future deadlines and zero intervals still take their original early exits;
missed intervals and overflow retain the original catch-up calculation and
fallbacks. No cache, additional state, or clock reads were introduced.

This affects explicit frame caps and background redraw scheduling. It does not
change the requested frame rate, the polling guard, or pending-request handling.

## Behavior validation

The integration tests contain the exact baseline deadline calculation and
compare it with the production function:

- 20,000 deterministic randomized cases, including exact deadlines, small
  delays, interval boundaries, and longer delays.
- Explicit cases just below, at, and above interval multiples; zero intervals;
  the `u32::MAX` catch-up limit; and the largest representable `Instant` on the
  test platform, including failed checked additions.
- 24,576 event-loop decisions across six intervals. The trace includes polling,
  stalled frames, and pending redraw requests, checking the deadline-dependent
  wait control, scheduling mode, and redraw reason against the baseline.

The 22 existing frame-pacing tests and the three new behavior tests passed
before and after the production edit. The complete platform suite passes in
release and debug: 50 tests, plus one ignored manual benchmark run separately.
Performance Clippy lints for the library and both frame-pacing integration tests,
and formatting checks, pass. A broader `--all-targets` lint run is blocked by the
pre-existing `clippy::manual_contains` error in `src/atomic_write.rs:235`'s test;
that unrelated test is unchanged.

```powershell
cargo test --locked --release -p deadlib-platform
cargo test --locked -p deadlib-platform
cargo clippy --locked -p deadlib-platform --lib --test frame_pacing --test deadline_advance -- -D clippy::perf
```

Validation was run on Windows x86-64. The production code is platform-independent,
but other operating systems' `Instant` implementations were not tested here.

## Before/after benchmark

Intel Xeon E5-2696 v4, Windows x86-64, Rust 1.98.1 / LLVM 22.1.8, repository release
profile with LTO. Three paired runs pinned to logical CPU 6, reversing variant
order in run 2. No builds or other tests from this pass ran during measurement.

The benchmark calls the frozen baseline and actual production function through
equivalent black-boxed function pointers. Timestamps are constructed before
measurement. Each operation advances 128 supplied deadlines; each variant uses
three warmup operations and seven batches of 32,768 operations. Cycle counts
use `QueryThreadCycleTime`; allocation counting is separate from timing.

The jitter cases cycle through lateness values from zero to one millisecond,
all within the first interval. The missed-frame case starts at exactly one
interval late and includes stalls up to one second. The synthetic mixed case
contains 25% future deadlines, 50% ordinary advances, and 25% missed intervals;
it is not a captured game workload.

| Case | Median ns/call, before | After | Median paired cycle reduction | Paired cycle range | Median throughput increase |
| --- | ---: | ---: | ---: | ---: | ---: |
| 240 FPS, exactly due | 25.43 | 7.18 | 71.8% | 71.6% to 74.5% | 253.9% |
| 240 FPS, jitter | 25.21 | 7.16 | 71.7% | 71.6% to 72.9% | 253.0% |
| 60 FPS, jitter | 25.31 | 7.11 | 71.8% | 71.4% to 72.5% | 255.8% |
| Background, jitter | 25.17 | 7.17 | 71.7% | 70.3% to 71.9% | 254.0% |
| 240 FPS, missed frames | 25.62 | 26.66 | -3.2% | -10.2% to -2.7% | -3.2% |
| 240 FPS, synthetic mix | 20.46 | 11.66 | 42.5% | 41.7% to 44.8% | 74.0% |
| Future deadline | 6.20 | 6.12 | 0.4% | -0.4% to 1.2% | 0.2% |
| Zero interval | 6.40 | 6.37 | 0.5% | -0.1% to 15.0% | 0.5% |

Time columns are medians across runs. Percentage columns are medians of paired
run ratios, so they need not equal the ratio of the displayed time medians.
All samples, including the outliers, are retained in the
[raw CSV](frame-deadline-advance-0.5.1525.csv). CSV times, cycles, and allocation
counts are per 128-call operation; throughput units are calls per second.

Ordinary advances save about **18 ns/call**. Missed-frame catch-up pays for the
extra checked addition: its median paired cost increases by **0.85 ns/call**
(0.68 to 2.55 ns across runs). Future-deadline and zero-interval paths are
unchanged; their measured differences are noise. Every case has zero Rust
allocations, reallocations, and frees before and after.

These are small CPU savings in a scheduling calculation, not a measured FPS
or input-latency improvement. The absolute saving is about 0.000018 ms per
ordinary deadline advance. Uncapped scheduling does not call this helper.

## Reproduce

Build once and run the printed integration-test executable directly:

```powershell
cargo test --locked --release -p deadlib-platform --test deadline_advance --no-run
(Get-Process -Id $PID).ProcessorAffinity = 64
# <executable> benchmark_deadline_advance --ignored --nocapture --test-threads=1
# Repeat three times; set DEADSYNC_PERF_REVERSE=1 for the middle run.
```

Capture stderr directly to a file. Local measurement logs are retained under
`target/frame-deadline-pass/run1.txt` through `run3.txt`.
