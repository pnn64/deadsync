# Frame diagnostics performance, 0.5.1214

Baseline: `09ab0b740` / 0.5.1213. This pass applies the supplied guide's
M-HOTPATH, M-THROUGHPUT, M-MEM-REUSE and M-INITIAL-CAPACITY guidance to
frame-statistics collection and overlay readouts. The overlay runs only when
enabled; these are diagnostic-path measurements, not a claim of higher game FPS.

## Three optimizations

1. **Copy the rolling window in contiguous slices.** `FixedFrameStatsRing::snapshot`
   reserves the complete result once and copies the chronological suffix and
   prefix. This removes per-sample modulo, capacity checks and pushes. Empty
   snapshots return after clearing the destination. Warm output capacity is reused;
   cold 128-sample snapshots remove all five reallocations.
2. **Decay and query only the occupied histogram range.** `DecayingHist` retains
   the smallest range covering every inserted bucket since reset. Outside buckets
   stay positive zero, and arithmetic order inside the range is unchanged. Negative
   or nonfinite decay factors promote the range to all 256 buckets, preserving
   signed-zero and NaN behavior. A full range uses the original fixed-size decay
   loop. Including alignment, histogram storage changes from 1,028 to 1,048 bytes
   on this target, and `FrameStatsLong` from 2,116 to 2,160 bytes. Both
   implementations already allocate nothing.
3. **Reuse readout formatting and equal displayed text.** Each readout keeps a
   reusable String, its last formatted key/text, and one preceding equivalent key.
   These keys bypass hashing. Other retained keys return directly from the map
   without replacing the recent Arc. A miss writes into scratch; identical text
   reuses the Arc and does not admit another key. Distinct output allocates its
   final Arc only. This preserves sharing for retained keys, handles alternating
   equivalent inputs, and continues reusing stable text after cache saturation.

Readout ownership remains presentation-thread-local. Each of the six maps still
admits at most 1,024 keys, starts at capacity 128, never evicts, and drops with its
thread. The scratch buffers start at 128 bytes each and retain their largest
formatted size, including extreme float representations. Each readout can hold
one additional Arc after its map saturates. Histogram ranges reset with the
existing overlay statistics; they do not shrink or periodically scan for eviction.
The ring retains its existing 128-sample application capacity and output buffer.
No dependencies, production allocator, target CPU, or unsafe production code changed.

## Measurement

Windows x86-64, Intel Xeon E5-2696 v4 @ 2.20 GHz, Rust 1.98.0
(`88d9e12ae`, LLVM 22.1.8), `x86_64-pc-windows-msvc`, release with full LTO.
The parent implementations, including their inline attributes, are frozen under
`tests/frame_diagnostics/` and compared with production code in the same binaries.

Five independent invocations alternate old/new order (new first on invocations
2 and 4). Each invocation uses the existing `tests/support/perf.rs` helper: three
warmups, seven timing samples, and one separately allocation-counted operation.
Tables report medians of the five invocation medians. Windows
`QueryThreadCycleTime` measures calling-thread CPU cycles, not elapsed TSC ticks.
Requested bytes include reallocation traffic; they are not peak RSS. Throughput
is derived from elapsed time. No timing thresholds are asserted in tests.
[All 140 measurements, including ranges and counters](frame-diagnostics-0.5.1214.csv)
are retained with this report.

## Results

Times and cycles are per operation. Throughput is millions of input samples/s
for ring and histogram cases, or millions of readouts/s for text cases.
An empty ring counts one operation as its throughput unit.

| Workload | ns old -> new | Thread cycles old -> new | Fewer cycles | M units/s old -> new |
|---|---:|---:|---:|---:|
| `ring_128_0_warm` | 4.6 -> 1.4 | 10.6 -> 3.4 | 67.9% | 215.579 -> 718.596 |
| `ring_128_32_warm` | 62.5 -> 36.1 | 136.8 -> 80.0 | 41.5% | 512.000 -> 885.622 |
| `ring_128_128_warm` | 204.0 -> 104.1 | 447.3 -> 228.9 | 48.8% | 627.364 -> 1229.568 |
| `ring_128_177_warm` | 189.4 -> 92.7 | 415.4 -> 186.4 | 55.1% | 675.977 -> 1381.523 |
| `ring_127_177_warm` | 257.3 -> 104.1 | 565.4 -> 228.9 | 59.5% | 493.634 -> 1219.676 |
| `ring_128_177_cold` | 1,244.8 -> 230.0 | 2,717.8 -> 505.6 | 81.4% | 102.828 -> 556.451 |
| `hist_fixed` | 6,932.8 -> 1,320.3 | 15,181.5 -> 2,908.4 | 80.8% | 36.926 -> 193.893 |
| `hist_narrow` | 6,710.9 -> 1,845.3 | 14,224.6 -> 4,038.5 | 71.6% | 38.147 -> 138.730 |
| `hist_full` | 9,900.0 -> 9,687.5 | 21,691.1 -> 21,253.8 | 2.0% | 25.859 -> 26.426 |
| `readout_stable` | 126.3 -> 110.1 | 276.9 -> 240.5 | 13.1% | 47.490 -> 54.504 |
| `readout_alternating` | 114.9 -> 121.7 | 252.7 -> 252.3 | 0.2% | 52.234 -> 49.300 |
| `readout_jitter` | 3,580.7 -> 2,426.2 | 7,828.4 -> 5,285.0 | 32.5% | 1.676 -> 2.473 |
| `readout_changing` | 4,926.5 -> 4,532.6 | 10,779.1 -> 9,920.4 | 8.0% | 1.218 -> 1.324 |
| `readout_saturated_stable` | 4,980.8 -> 107.1 | 10,904.6 -> 235.5 | 97.8% | 1.205 -> 56.046 |

The full-domain histogram and alternating-readout controls are near the parent
in CPU cycles; these small differences are not evidence of a meaningful speedup.
Alternating readouts have a 5.9% higher elapsed median (114.9 -> 121.7 ns), while
the independently aggregated cycle median is essentially unchanged (-0.2%).
Timing ranges overlap. The intended gains are the substantial reductions in
snapshot work, narrow histogram work, and formatting/allocation on readout misses.

Allocation counts and free counts match, as do requested and freed bytes, for
each reported operation. Warm ring and all histogram cases have zero heap churn
in both implementations. Stable and alternating readouts also remain at zero.

| Workload | Allocations/frees old -> new | Reallocations old -> new | Requested/freed bytes old -> new |
|---|---:|---:|---:|
| `ring_128_177_cold` | 1 -> 1 | 5 -> 0 | 12,096 -> 6,144 |
| `readout_jitter` | 10 -> 0 | 1 -> 0 | 1,000 -> 0 |
| `readout_changing` | 12 -> 5 | 1 -> 0 | 1,160 -> 456 |
| `readout_saturated_stable` | 12 -> 0 | 1 -> 0 | 1,160 -> 0 |

An additional cold-cache behavior test feeds 4,096 distinct telemetry keys that
all render the same six strings. Retained entries fall from **4,098 to 6**, and
retained string payloads from **284,763 to 369 bytes**. Those payload figures
exclude Arc headers, hash tables, recent-key metadata and reusable scratch; they
are not total process memory. Scratch adds 768 initially reserved bytes across
six readouts. Warm ring output remains 6,144 bytes for 128 samples in both versions.

## Workloads and behavior

Ring fixtures use real 48-byte `FrameStatsSample` values, with capacity 128 and
0, 32, 128, or 177 insertions, plus capacity 127 with 177 insertions. The last two
wrap. Warm snapshots reuse their destination, including destruction-free empty
snapshots; the cold case includes allocating and dropping the output Vec.
Each timing sample runs 4,096 snapshots, with fixtures built outside timing.

Histogram operations contain 256 updates and 13 p99 queries (one per 20 updates).
Each timing sample runs 128 operations. Fixed traffic is 16,667 us, narrow traffic
is `16000 + (i * 17 % 1000)` us, and full traffic is `i * 200` us for `i=0..255`.
All use gamma 1023/1024 and 200-us buckets, and are warmed with one input sequence.
The benchmark retains history across operations, as the overlay does.

Readout operations build and drop all **six** helper results, covering both the
five-cell full overlay and the single compact readout. Actual rendering chooses
five or one, not all six simultaneously. Fixtures are outside timing; each sample
contains 4,096 operations. Stable uses a 60-FPS gameplay summary. Alternating
switches two summaries. Jitter changes the low 12 bits of FPS/display-error/audio
float keys within the same displayed precision. Changing advances FPS, mean time,
spike time, display error and underrun count. Jitter, changing and saturated-stable
cases first feed 2,048 distinct summaries to exercise saturated caches; the latter
then repeats a previously uncached summary. No text is rounded before formatting.

Eight new behavioral/allocation tests cover:

- Empty, partial, wrapped and cleared rings, capacities 0/1/3/127/128/513, and
  zero-sized elements; cold capacity budgets and warm no-churn assertions.
- Every histogram bin and total (bitwise, with NaNs compared by classification),
  percentile boundaries, resets, 120,000 updates, zero/negative/nonfinite decay,
  bucket-width extremes and overflow values.
- Exact text at neighboring float values around rounding boundaries, signed zero,
  infinities, NaNs, maximum integers, gameplay/menu modes and p99 visibility.
- Saturation, immutable held Arcs, retained-key hits, equivalent alternating keys,
  scratch reuse, reduced churn on changed output, and lower retained payloads.

Validation on the final production code:

- Config, shell and theme library suites: **1,844 passed**, six existing ignored.
- Focused diagnostic suites: **29 passed**, two manual benchmarks ignored, in
  debug and release (including the included modules' existing regression tests).
- Application `cargo check --locked` passed.
- Performance Clippy passed with the existing `large_enum_variant` warning in
  unchanged `deadsync-theme-simply-love/src/effects.rs:928`; no findings in changed code.
- Scoped rustfmt, `git diff --check`, parent-source and version/lockfile audits passed.
- Cargo.toml bumps 0.5.1213 -> 0.5.1214 exactly once; Cargo.lock updates only the
  application, theme and version crates that inherit the workspace version.

## Reproduce

```powershell
cargo test -p deadsync-config -p deadsync-shell -p deadsync-theme-simply-love --lib --locked -- --test-threads=1
cargo test -p deadsync-shell -p deadsync-theme-simply-love --test frame_diagnostics --locked
cargo test -p deadsync-shell -p deadsync-theme-simply-love --release --test frame_diagnostics --locked
cargo check -p deadsync --locked
cargo clippy -p deadsync-config -p deadsync-shell -p deadsync-theme-simply-love --lib --test frame_diagnostics --locked --no-deps -- -A clippy::all -W clippy::perf
cargo test -p deadsync-shell -p deadsync-theme-simply-love --release --test frame_diagnostics --locked -- --ignored --test-threads=1 --nocapture
```

Run the last command five times, setting `$env:DEADSYNC_PERF_REVERSE = '1'` for
runs 2 and 4, and removing it for runs 1, 3 and 5. Stop other builds before measuring.
On non-Windows systems the helper reports zero for unavailable thread cycles;
those zeros must not be interpreted as measurements.
