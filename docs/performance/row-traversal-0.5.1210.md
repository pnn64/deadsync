# Life recording and row traversal - 0.5.1210

Baseline: `e4ed8302c28ea18e06cccf425a30d8f15f1d7482` / 0.5.1209.
This pass applies `rust-performance.md` guidance on hot-path measurement,
reusing memory and reserving sufficient capacity to three operations:

1. **Record a life change as a pair.** Gameplay previously called the life
   recorder twice with the before and after values. `record_life_change`
   writes the final shifted samples together, avoiding temporary timestamp
   writes and repeated buffer checks. Both gameplay callers use it. Rewinds,
   empty histories and nonfinite timestamps retain the individual-record
   path. The individual recorder also replaces a plateau endpoint directly,
   avoiding a push/remove pair and unnecessary growth of a full buffer.
2. **Summarize judgments during the row-boundary scan.** The shared row
   visitor starts with the first note's result and selects the remaining
   notes while finding the row end. This removes the second traversal for
   timing statistics, judgment-window counts and histogram counting. Miss
   priority, latest-tap selection and ties retain the existing selector.
3. **Prepare scatter rows in one traversal.** Collect the selected judgment,
   representative note and direction code while finding the row boundary.
   This removes a second pass over every row. Timestamp lookup, parity join,
   direction saturation and output order stay the same.

The measured calling pattern for 8,192 life changes uses **21.5% fewer
thread cycles**. Window counting for 4,096 two-note rows uses **6.2%
fewer**, and scatter construction for the same rows uses **16.3%
fewer**. These are synthetic benchmark results, not whole-game CPU or FPS.

## Measurement

Windows x86-64, Intel Xeon E5-2696 v4 at 2.20 GHz, Rust 1.98.0 (`88d9e12ae`),
repository release profile (opt-level 3, LTO). The production functions and
frozen parent functions are compiled into the same rules test executable.
All eight baseline functions were compared with the parent after normalizing
whitespace and the row visitor's test-only visibility. Types and unchanged
judgment-selection and statistics math are shared. The old life-change
benchmark invokes the frozen recorder twice, matching the parent gameplay
call sites.

Five complete runs alternate old-first/new-first. Each measurement uses three
warmups and seven timing samples. Tables show medians of the five per-run
medians. Windows `QueryThreadCycleTime` measures calling-thread CPU cycles;
allocation tracking runs separately from timing. The [CSV](row-traversal-0.5.1210.csv)
contains all 350 measurements, wall-time ranges, throughput, allocation,
reallocation and free counts, and requested/freed bytes.

Inputs are prepared before measurement and passed through black boxes.
Life buffers are allocated once, then cleared and reused per operation;
clearing and recording are timed, while allocation/destruction of those
retained buffers is outside the measured region. The full-plateau case
includes constructing and destroying its exact two-element input buffer.
Scatter benchmarks include allocating and destroying the returned vector.
No builds or checks run during final measurements. The host is shared, so
background load and cache/processor state can vary. Very short controls are
particularly sensitive to measurement overhead.

## Results

28 of 35 workloads have lower median thread cycles. Negative cycle reductions indicate slower results; all controls are retained.

| Workload | Old ns/op | New ns/op | Thread cycles old/new | Cycle reduction | Throughput old/new (M units/s) |
|---|---:|---:|---:|---:|---:|
| `life_changes_128` | 647.7 | 610.9 | 1,431.9 / 1,349.6 | 5.7% | 197.636 / 209.514 |
| `life_plateau_128` | 989.1 | 249.2 | 2,183.0 / 559.0 | 74.4% | 129.415 / 513.605 |
| `life_changing_128` | 283.6 | 238.3 | 636.2 / 533.3 | 16.2% | 451.350 / 537.180 |
| `life_mixed_128` | 810.9 | 248.4 | 1,790.3 / 555.6 | 69.0% | 157.842 / 515.220 |
| `life_same_time_128` | 336.7 | 371.1 | 751.1 / 826.6 | -10.1% | 380.139 / 344.926 |
| `life_changes_8192` | 46,679.7 | 36,596.9 | 102,280.1 / 80,239.2 | 21.5% | 175.494 / 223.844 |
| `life_plateau_8192` | 63,763.3 | 14,964.1 | 137,202.9 / 32,829.0 | 76.1% | 128.475 / 547.445 |
| `life_changing_8192` | 15,518.8 | 16,172.7 | 33,945.3 / 35,480.1 | -4.5% | 527.878 / 506.534 |
| `life_mixed_8192` | 50,809.4 | 16,385.2 | 111,391.1 / 35,950.0 | 67.7% | 161.230 / 499.965 |
| `life_same_time_8192` | 20,290.6 | 21,164.1 | 44,524.2 / 46,396.8 | -4.2% | 403.733 / 387.071 |
| `life_full_plateau` | 139.9 | 63.5 | 305.1 / 139.8 | 54.2% | 7.150 / 15.748 |
| `stats_0x1_judged` | 1.0 | 1.2 | 4.7 / 5.1 | -8.5% | 1024.000 / 853.333 |
| `windows_0x1_judged` | 6.6 | 6.8 | 17.1 / 17.1 | 0.0% | 150.588 / 146.286 |
| `scatter_0x1_judged` | 16.0 | 15.6 | 38.2 / 37.3 | 2.4% | 62.439 / 64.000 |
| `stats_1x1_judged` | 12.3 | 9.8 | 29.6 / 23.6 | 20.3% | 81.270 / 102.400 |
| `windows_1x1_judged` | 12.5 | 15.6 | 30.0 / 36.9 | -23.0% | 80.000 / 64.000 |
| `scatter_1x1_judged` | 78.1 | 77.5 | 174.9 / 173.2 | 1.0% | 12.800 / 12.897 |
| `stats_128x1_judged` | 879.7 | 715.0 | 1,921.5 / 1,572.5 | 18.2% | 145.506 / 179.011 |
| `windows_128x1_judged` | 966.6 | 736.7 | 2,123.4 / 1,620.1 | 23.7% | 132.423 / 173.743 |
| `scatter_128x1_judged` | 1,754.1 | 1,424.6 | 3,852.8 / 3,124.0 | 18.9% | 72.972 / 89.849 |
| `stats_8192x1_judged` | 58,989.1 | 49,400.0 | 129,378.1 / 108,162.1 | 16.4% | 138.873 / 165.830 |
| `windows_8192x1_judged` | 66,887.5 | 50,734.4 | 146,704.9 / 111,272.8 | 24.2% | 122.474 / 161.468 |
| `scatter_8192x1_judged` | 136,350.0 | 113,821.9 | 298,677.8 / 247,259.9 | 17.2% | 60.081 / 71.972 |
| `stats_4096x2_judged` | 41,704.7 | 37,465.6 | 90,687.8 / 82,202.8 | 9.4% | 196.429 / 218.654 |
| `windows_4096x2_judged` | 43,850.0 | 41,675.0 | 96,147.9 / 90,152.8 | 6.2% | 186.819 / 196.569 |
| `scatter_4096x2_judged` | 91,098.4 | 76,239.1 | 199,762.2 / 167,166.4 | 16.3% | 89.925 / 107.451 |
| `stats_2048x4_judged` | 38,400.0 | 37,542.2 | 84,054.8 / 81,335.0 | 3.2% | 213.333 / 218.208 |
| `windows_2048x4_judged` | 37,006.2 | 35,846.9 | 80,628.5 / 77,047.9 | 4.4% | 221.368 / 228.528 |
| `scatter_2048x4_judged` | 68,812.5 | 59,175.0 | 148,104.2 / 127,831.3 | 13.7% | 119.048 / 138.437 |
| `stats_1024x8_judged` | 38,151.6 | 36,723.4 | 82,813.3 / 79,904.9 | 3.5% | 214.723 / 223.073 |
| `windows_1024x8_judged` | 36,384.4 | 36,356.2 | 79,795.1 / 79,764.2 | 0.0% | 225.152 / 225.326 |
| `scatter_1024x8_judged` | 63,345.3 | 55,310.9 | 138,929.8 / 121,297.8 | 12.7% | 129.323 / 148.108 |
| `stats_2048x4_mixed` | 44,142.2 | 43,237.5 | 96,861.2 / 94,875.5 | 2.1% | 185.582 / 189.465 |
| `windows_2048x4_mixed` | 46,015.6 | 40,109.4 | 100,781.4 / 88,067.5 | 12.6% | 178.026 / 204.242 |
| `scatter_2048x4_mixed` | 73,820.3 | 75,159.4 | 161,606.9 / 164,769.1 | -2.0% | 110.972 / 108.995 |

Allocation/free counts match in every measurement, as do requested/freed bytes. The following table groups workloads with the same storage behavior; the CSV retains every individual count.

| Workload family | Allocations/frees old/new | Reallocations old/new | Requested/freed bytes old/new |
|---|---:|---:|---:|
| Retained life buffers (all batch cases) | 0 / 0 | 0 / 0 | 0 / 0 |
| Cold full plateau | 1 / 1 | 1 / 0 | 48 / 16 |
| Timing statistics and window counts | 0 / 0 | 0 / 0 | 0 / 0 |
| Empty scatter | 0 / 0 | 0 / 0 | 0 / 0 |
| Nonempty scatter | 1 / 1 | 0 / 0 | 24 * input notes / 24 * input notes |

Higher cycle medians occurred in these cases:

- `life_same_time_128`: 10.1% more cycles; 336.7 -> 371.1 ns/op.
- `life_changing_8192`: 4.5% more cycles; 15,518.8 -> 16,172.7 ns/op.
- `life_same_time_8192`: 4.2% more cycles; 20,290.6 -> 21,164.1 ns/op.
- `stats_0x1_judged`: 8.5% more cycles; 1.0 -> 1.2 ns/op.
- `windows_1x1_judged`: 23.0% more cycles; 12.5 -> 15.6 ns/op.
- `scatter_2048x4_mixed`: 2.0% more cycles; 73,820.3 -> 75,159.4 ns/op.

## Storage and workload limits

Summaries and direct scatter visits allocate nothing before or after this
change. Owned scatter construction still makes one output allocation for
nonempty notes, with the same capacity (`notes.len()`) and requested bytes.
The life recorder reuses the caller's vector, retaining identical samples;
it adds no cache, global state or stack array. Extending a full plateau now
keeps its two-element capacity instead of growing temporarily to four.
The cold full-plateau benchmark therefore drops from one allocation plus one
reallocation to one allocation, and 48 to 16 requested/freed bytes. Zero
allocations applies to the update itself when its final samples fit the
existing buffer. Other life changes can still grow the buffer as needed.
Requested bytes measure allocator churn, not RSS or peak live process memory.

Gameplay records life only when it changes, as before/after pairs. The
`life_changes` fixtures exercise that pattern with valid 0..1 values and
increasing timestamps. Single-value `life_plateau` and `life_mixed` fixtures
are recorder-level controls, not claims about how often gameplay appends
unchanged values. `life_changing` changes each value; `life_same_time` changes
four values at each timestamp. Each batch contains 128 or 8,192 inputs and
uses 128 iterations per timing sample. Life throughput counts changes for
paired recording and samples for single recording. `life_full_plateau` uses
4,096 iterations and counts one update per operation.

Row fixtures contain 0, 1, 128 or 8,192 single-note rows, 4,096 two-note rows,
2,048 four-note rows, or 1,024 eight-note rows. Non-mixed fixtures contain tap
notes with approximately 80% Fantastic, 13% Excellent, 5% Great, 1% Decent
and 1% Miss results, distributed by `(note_index * 37) % 100`. Offsets cycle
from -50 to +50 ms. These exercise the supplied results, rather than
simulating judgment classification. The four-note mixed fixture cycles all
six grades, note types, fake/disabled/missing results and nonfinite offsets.
Wide chords and mixed edge inputs are compatibility/scaling cases. These are
not a measured distribution of real charts.

Summary/scatter workloads use 512 iterations for up to 128 rows, otherwise
64. Throughput counts input notes, or one operation for empty input. Scatter
supplies cached timestamps and sorted alternating left/right row parity.
Only the owning scatter API is timed; the direct visitor's zero-allocation
contract is tested. Shared-row changes also reach histograms, but no histogram
speedup is claimed without a separate whole-histogram comparison.

## Behavior and validation

Seven new tests compare parent behavior and allocation budgets:

- Life histories match float bits after each event, including plateaus,
  out-of-order/same-time records, signed zeros, clamping and nonfinite values.
- Paired changes match two parent calls at timestamp-shift/epsilon boundaries,
  with empty or nonfinite histories and 4,096 successive changes.
- A full plateau buffer does not grow; paired updates allocate nothing when
  their final samples fit in the caller's buffer.
- Row traversal selects the exact same judgment references, with identical
  finite timing-stat bits, nonfinite classifications and all window counts.
  Arithmetic NaN signs/payloads can differ after optimization and are not
  required to match. Stored life/scatter samples still match raw bits.
  Row tests cover repeated row IDs,
  misses, ties, fake/disabled notes, mines and missing results.
- Scatter comparisons cover every output field, representative notes,
  short/missing timestamp caches, absent/present parity, lane offsets,
  zero lanes and direction saturation with 300-note rows.
- Summaries and scatter visits have zero heap churn; owned scatter output
  keeps its single-allocation budget.

- Rules: **121 passed**, four manual benchmarks ignored, in debug and release.
- Gameplay: **797 passed**, eight pre-existing ignored tests, serial.
- Score: **241 passed**, five pre-existing ignored tests, serial.
- Theme: **1,279 passed**, five pre-existing ignored tests, serial.
- Rules performance Clippy: passed without lint exceptions.
- `cargo check -p deadsync`: passed.

Scoped rustfmt and `git diff --check` pass. The version audit confirms exactly
0.5.1209 -> 0.5.1210 in Cargo.toml and all three matching Cargo.lock entries.
No dependencies or unsafe code were added.

## Reproduce

```powershell
cargo test -p deadsync-rules --lib
cargo test -p deadsync-rules --release --lib
cargo test -p deadsync-gameplay --lib -- --test-threads=1
cargo test -p deadsync-score --lib -- --test-threads=1
cargo test -p deadsync-theme-simply-love --lib -- --test-threads=1
cargo clippy -p deadsync-rules --lib --tests --no-deps -- -A clippy::all -D clippy::perf
cargo check -p deadsync
cargo test -p deadsync-rules --release --lib benchmark_row_traversal -- --ignored --test-threads=1 --nocapture
```

Repeat the benchmark five times, setting `DEADSYNC_PERF_REVERSE=1` for runs
2 and 4 and removing it for runs 1, 3 and 5. The existing allocator/timer in
`tests/support/perf.rs` reports the measurements. Non-Windows runs report zero
for unavailable thread cycles; those zeros are not measured CPU results.
