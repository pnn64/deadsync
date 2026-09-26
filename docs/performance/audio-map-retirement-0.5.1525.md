# Audio position-map retirement

Baseline: `59179a336`.

`PlaybackPosMap::cleanup` used to advance the stream and music timestamps of
every expired segment before removing it. Fully expired segments now leave the
deque directly. Partial trims retain the same timestamp arithmetic and backlog
accounting. No cache, storage, allocation, or synchronization was added.

This code runs when the application drains played-audio timing records into its
position map. Savings apply when old segments expire completely, such as when
timing records cannot be merged. Ordinary fixed-rate playback usually merges
records into one segment and only trims its beginning.

## Behavior checks

The integration test uses the production position module and the existing frozen
reference. The reference's `insert` and `cleanup` implementations were verified
identical to the baseline above before the edit (apart from line endings).

After each of 128,784 insertions, the test compares backlog length, queue order,
segment frame fields, and the exact bits of both floating-point timestamp fields.
Cases cover partial and complete expiration, wrapped queues, removal of multiple
segments, large incoming records, merged playback, positive/negative/zero rates,
subnormal and extreme finite values, and rejected non-finite updates.

The full audio-core release suite passed before and after: 48 library tests,
32 existing integration tests, and 16 tests in the new integration target
(14 of those are existing position-module tests). Debug tests also passed after
the change. The two manual benchmarks remain ignored during normal tests.
Clippy passed with performance warnings denied; its existing telemetry
argument-count warning remains.

```powershell
cargo test --locked --release -p deadlib-audio-core
cargo test --locked -p deadlib-audio-core
cargo clippy --locked -p deadlib-audio-core --lib -- -D clippy::perf
```

## Before/after measurements

Windows x86-64, Intel Xeon E5-2696 v4, Rust 1.98.1 / LLVM 22.1.8. Repository
release profile with full LTO. Three paired runs pinned to logical CPU 6;
baseline first in runs 1/3, changed version first in run 2. Each variant measures
seven batches of 1,048,576 insertions after 2,048 warmup insertions. No builds or
other tests from this pass ran concurrently with these measurements.

The benchmark calls the complete `insert` operation, including validation,
merge checks, queue updates, and cleanup. Input construction is also timed.
Inputs and mutable maps pass through `black_box`; allocation counting is
separate from timing. Fragmented fixtures alternate music origins to prevent
coalescing. Replacement fixtures are stress cases, not normal callback sizes.

| Fixture | Before ns/update | After ns/update | Median paired time reduction | Paired range |
| --- | ---: | ---: | ---: | ---: |
| Merged, 512 frames | 16.0 | 15.5 | 1.4% | -1.9% to 3.7% |
| Fragmented, 512 frames | 22.3 | 18.9 | 11.2% | 10.4% to 15.2% |
| Fragmented, 64 frames | 18.5 | 16.7 | 9.7% | 8.4% to 10.1% |
| Replace with 80,000 frames | 17.9 | 15.7 | 12.3% | 8.5% to 14.4% |
| Replace with 120,000 frames | 20.8 | 19.5 | 11.6% | 6.3% to 12.3% |

Times are medians of run medians; reductions are medians of paired reductions.
The fragmented 512/64-frame cases used 11.0%/9.6% fewer calling-thread cycles,
with throughput increasing 12.4%/10.6%. Every fixture had zero allocations,
reallocations, and frees before and after. [Raw measurements](audio-map-retirement-0.5.1525.csv)
include each run's timing range, cycles, throughput, and allocation counts.

The merged case stayed within noise. These are nanosecond-scale CPU savings in
position-map maintenance, not measured improvements to frame rate, audio
latency, or complete audio callbacks. Audio sample generation is unchanged.

Reproduce after compilation finishes:

```powershell
cargo test --locked --release -p deadlib-audio-core --test map_retirement --no-run
(Get-Process -Id $PID).ProcessorAffinity = 64
cargo test --locked --release -p deadlib-audio-core --test map_retirement map_retirement_benchmark -- --ignored --nocapture --test-threads=1
# Set DEADSYNC_PERF_REVERSE=1 to run the changed implementation first.
```
