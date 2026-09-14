# Course graph assembly and life sampling - 0.5.1212

Baseline: `8037d3c007f466c0160fc6c12ac81a79599bf900` / 0.5.1211.
This pass applies the supplied `rust-performance.md` guidance on hot-path
measurement (M-HOTPATH), sufficient initial capacity (M-INITIAL-CAPACITY),
and caller-owned memory reuse (M-MEM-REUSE) to three operations:

1. **Prepare the life-record boundary once per batch.** Each of the 100
   evaluation graph samples previously searched for the same chart-start
   boundary and clamped the same anchor life. `LifeRecordSampler` prepares
   those once, retaining the original per-sample upper-bound search and
   interpolation. Both graph points and the Barely marker use it. This also
   preserves behavior on duplicate, nonmonotonic and nonfinite records.
2. **Reuse histogram columns across course stages.** `DensityHistScratch`
   clears and refills a caller-owned column vector. Course construction
   retains its capacity across stages instead of allocating, shrinking and
   freeing columns for each stage. Existing owning builders use the same
   filling routine and retain their previous storage contracts.
3. **Append translated segments directly to the combined course mesh.**
   The append API emits complete six-vertex segments into the destination,
   applying the original translation before writing them. This removes the
   temporary vertex vector, separate translation pass and subsequent copy
   for each stage. Scratch is dropped before the final immutable Arc copy.

The batch over 8,192 life records uses **54.0% fewer thread
cycles**. A varied 4,096-measure stage with retained column scratch uses
**42.5% fewer cycles** and **one allocation instead of two**.
Four varied 1,024-measure course stages use **28.1% fewer
cycles**, with **10 allocation/free calls reduced to 3**. The two course
changes share one append API; their whole-course gains are combined and
must not be added together. These are function benchmarks, not measured
whole-game CPU, frame latency or FPS improvements.

## Measurement

Windows x86-64, Intel Xeon E5-2696 v4 at 2.20 GHz, Rust 1.98.0
(`88d9e12ae`, 2026-08-18), repository release profile (opt-level 3, LTO).
The integration test compiles the private production graph module unchanged
and calls the actual library's density APIs. All eight frozen graph functions
were compared with the parent, allowing only import/visibility changes and
signature formatting. The entire frozen density module was also verified;
its owning algorithms are unchanged. Colors, vertex types and chart types
come from the same unchanged dependencies on both sides.

Five complete runs alternate old-first/new-first. Each measurement uses
three warmups and seven timing samples. Tables report medians of the five
per-run medians. Windows `QueryThreadCycleTime` measures calling-thread CPU
cycles, separately from allocation tracking. The [CSV](course-graphs-0.5.1212.csv)
contains all 380 measurements, including timing ranges, throughput,
allocations, reallocations, frees and requested/freed bytes. Final runs are
collected after this pass's builds and checks. The host is shared, so heap
state, processor state and background load can vary.

Fixtures are built before measurement and inputs/outputs pass through black
boxes. Owning and whole-course operations include construction and disposal
of all returned storage. Scratch cases warm the column workspace before
measurement but create/drop a fresh vertex vector each operation. Warm
append cases retain both vectors, clear the output and overwrite it each
operation. Their baseline is the parent stage builder plus its translation
loop and destruction of the temporary vector. Retained allocations are
outside timing/tracking; whole-course measurements include their creation
and destruction. None of these routines launches worker threads.

## Results

36 of 38 workloads have lower median thread cycles.
Negative reductions indicate slower measurements; all controls are retained.

| Workload | Old ns/op | New ns/op | Thread cycles old/new | Cycle reduction | Throughput old/new (M units/s) |
|---|---:|---:|---:|---:|---:|
| `life_points_0` | 23.0 | 23.0 | 54.9 / 55.7 | -1.5% | 43.390 / 43.390 |
| `life_points_1` | 853.5 | 680.5 | 1,880.3 / 1,498.8 | 20.3% | 117.162 / 146.958 |
| `life_points_8` | 2,138.7 | 1,171.5 | 4,686.7 / 2,576.6 | 45.0% | 46.758 / 85.362 |
| `life_points_128` | 4,182.0 | 2,134.0 | 9,162.4 / 4,675.5 | 49.0% | 23.912 / 46.861 |
| `life_points_8192` | 7,726.2 | 3,549.6 | 16,947.8 / 7,788.0 | 54.0% | 12.943 / 28.172 |
| `life_points_65536` | 13,007.0 | 5,150.4 | 28,523.0 / 11,296.5 | 60.4% | 7.688 / 19.416 |
| `owning_flat_64` | 693.8 | 549.6 | 1,527.1 / 1,211.5 | 20.7% | 92.252 / 116.446 |
| `scratch_flat_64` | 414.5 | 309.8 | 914.9 / 684.2 | 25.2% | 154.420 / 206.608 |
| `warm_append_flat_64` | 479.3 | 251.2 | 1,057.2 / 557.3 | 47.3% | 133.529 / 254.806 |
| `owning_blocks_64` | 765.6 | 546.5 | 1,684.8 / 1,204.7 | 28.5% | 83.592 / 117.112 |
| `scratch_blocks_64` | 468.4 | 325.8 | 1,032.3 / 720.2 | 30.2% | 136.647 / 196.451 |
| `warm_append_blocks_64` | 462.1 | 284.4 | 1,018.6 / 629.3 | 38.2% | 138.495 / 225.055 |
| `owning_varied_64` | 1,582.4 | 1,458.6 | 3,477.7 / 3,212.8 | 7.6% | 40.444 / 43.878 |
| `scratch_varied_64` | 1,668.0 | 1,333.2 | 3,646.6 / 2,931.5 | 19.6% | 38.370 / 48.005 |
| `warm_append_varied_64` | 1,628.1 | 1,272.3 | 3,569.4 / 2,674.3 | 25.1% | 39.309 / 50.304 |
| `owning_flat_4096` | 11,534.4 | 11,200.0 | 25,359.1 / 24,625.2 | 2.9% | 355.112 / 365.714 |
| `scratch_flat_4096` | 17,587.5 | 10,493.8 | 38,645.7 / 23,074.9 | 40.3% | 232.893 / 390.328 |
| `warm_append_flat_4096` | 17,053.1 | 10,359.4 | 37,445.3 / 22,718.2 | 39.3% | 240.191 / 395.391 |
| `owning_blocks_4096` | 66,459.4 | 68,306.2 | 145,782.3 / 149,801.9 | -2.8% | 61.632 / 59.965 |
| `scratch_blocks_4096` | 74,940.6 | 37,215.6 | 163,486.3 / 81,750.0 | 50.0% | 54.657 / 110.061 |
| `warm_append_blocks_4096` | 78,321.9 | 21,387.5 | 171,813.7 / 46,458.6 | 73.0% | 52.297 / 191.514 |
| `owning_varied_4096` | 224,668.8 | 208,759.4 | 491,083.2 / 455,174.4 | 7.3% | 18.231 / 19.621 |
| `scratch_varied_4096` | 248,450.0 | 141,668.8 | 538,872.5 / 309,899.7 | 42.5% | 16.486 / 28.913 |
| `warm_append_varied_4096` | 244,200.0 | 99,628.1 | 532,239.5 / 218,478.0 | 59.0% | 16.773 / 41.113 |
| `course_empty` | 8.2 | 7.8 | 23.1 / 22.3 | 3.5% | 121.905 / 128.000 |
| `course_1x64_flat` | 800.0 | 642.2 | 1,780.0 / 1,430.2 | 19.7% | 80.000 / 99.659 |
| `course_1x64_blocks` | 901.6 | 698.4 | 1,999.5 / 1,553.7 | 22.3% | 70.988 / 91.633 |
| `course_1x64_varied` | 2,428.1 | 1,903.1 | 5,346.9 / 4,194.5 | 21.6% | 26.358 / 33.629 |
| `course_4x64_flat` | 2,523.4 | 1,770.3 | 5,556.1 / 3,903.0 | 29.8% | 101.449 / 144.607 |
| `course_4x64_blocks` | 2,781.2 | 2,034.4 | 6,125.4 / 4,489.5 | 26.7% | 92.045 / 125.837 |
| `course_4x64_varied` | 21,745.3 | 16,590.6 | 47,652.1 / 36,365.0 | 23.7% | 11.773 / 15.430 |
| `course_4x1024_flat` | 15,184.4 | 12,062.5 | 33,346.8 / 26,497.8 | 20.5% | 269.751 / 339.565 |
| `course_4x1024_blocks` | 43,906.2 | 29,334.4 | 94,127.8 / 64,316.9 | 31.7% | 93.290 / 139.631 |
| `course_4x1024_varied` | 323,879.7 | 232,707.8 | 704,629.3 / 506,575.2 | 28.1% | 12.647 / 17.601 |
| `course_16x1024_flat` | 54,237.5 | 45,187.5 | 118,941.5 / 99,488.5 | 16.4% | 302.079 / 362.578 |
| `course_16x1024_blocks` | 142,987.5 | 114,937.5 | 308,589.6 / 251,958.6 | 18.4% | 114.583 / 142.547 |
| `course_16x1024_varied` | 2,526,887.5 | 2,282,712.5 | 5,516,528.8 / 4,973,046.8 | 9.9% | 6.484 / 7.177 |
| `course_mixed` | 547,312.5 | 459,850.0 | 1,190,732.6 / 1,001,372.7 | 15.9% | 10.064 / 11.978 |

Allocation/free counts and requested/freed bytes match within each owning
operation. Life sampling has zero heap churn before and after. Owning
density controls retain their allocation counts and requested bytes. Scratch
cases reduce two allocation/free calls to one and eliminate the column
shrink reallocation. Warm appends have zero churn after reserving capacity.

| Whole-course workload | Allocations/frees old/new | Reallocations old/new | Requested/freed bytes old/new |
|---|---:|---:|---:|
| `course_empty` | 0 / 0 | 0 / 0 | 0 / 0 |
| `course_1x64_flat` | 4 / 3 | 1 / 0 | 2,512 / 2,152 |
| `course_1x64_blocks` | 4 / 3 | 1 / 0 | 4,336 / 3,304 |
| `course_1x64_varied` | 4 / 3 | 0 / 0 | 29,224 / 20,008 |
| `course_4x64_flat` | 10 / 3 | 6 / 2 | 10,864 / 4,744 |
| `course_4x64_blocks` | 10 / 3 | 6 / 2 | 19,888 / 11,080 |
| `course_4x64_varied` | 10 / 3 | 2 / 2 | 144,496 / 102,952 |
| `course_4x1024_flat` | 10 / 3 | 6 / 2 | 103,024 / 27,784 |
| `course_4x1024_blocks` | 10 / 3 | 6 / 2 | 382,768 / 224,200 |
| `course_4x1024_varied` | 10 / 3 | 2 / 2 | 2,310,256 / 1,646,632 |
| `course_16x1024_flat` | 34 / 3 | 20 / 4 | 412,912 / 38,152 |
| `course_16x1024_blocks` | 34 / 3 | 20 / 4 | 1,585,456 / 877,384 |
| `course_16x1024_varied` | 34 / 3 | 4 / 4 | 9,683,344 / 6,955,048 |
| `course_mixed` | 13 / 3 | 6 / 7 | 3,867,976 / 3,223,456 |

The mixed course has one additional reallocation as retained column capacity
grows across stage sizes. Total allocation-plus-reallocation calls and
requested bytes still decrease. No workload increases either total.

Higher cycle medians occurred in these controls/workloads:

- `life_points_0`: 1.5% more cycles; 23.0 -> 23.0 ns/op.
- `owning_blocks_4096`: 2.8% more cycles; 66,459.4 -> 68,306.2 ns/op.

## Workloads and storage bounds

Life fixtures contain 0, 1, 8, 128, 8,192 or 65,536 records. Timestamps are
`(index - count * 0.25) * 0.25`, placing a quarter of large histories before
the chart start. Life is `((index * 17) % 101) / 100`. Each operation builds
100 graph points, at record start 0, graph first -2, width 854 and height 64.
The graph end is one second beyond the last timestamp, or at least 2.
Empty input uses end 10. Each timing sample contains 256 operations;
throughput counts 100 output samples, or one empty operation.

Density fixtures use flat NPS 8, 16-measure blocks cycling 0/8/12/16, or
varied NPS `(index * 17) % 31 + 1`. Peak NPS is 32, timestamps are integer
seconds and each stage ends at its measure count (at least 1). Colors use
desaturation 0.5 and alpha 0.65. Standalone stages have 64 or 4,096 measures,
width 854, height 64, and translation zero. Timing samples contain 256 or
32 operations respectively. Owning controls call the old and current
one-shot API, checking the shared column-fill refactor in isolation.

Course cases cover empty input, one or four 64-measure stages, and four or
16 stages of 1,024 measures, at music rate 1.5 and size 854 by 64. They use
64 iterations unless total measures exceed 4,096, then eight. The mixed
course uses lengths `[0, 2, 64, 1024, 1, 4096, 64, 0, 257]` with rotating
flat/block/varied shapes, and 16 iterations. Empty courses use 256 iterations.
Density/course throughput counts input measures (including skipped leading
zeros), or one operation for empty courses. These are deterministic scaling
fixtures, not a measured distribution of real charts.

Life sampling adds a small borrowed view and no heap storage. It still
produces the existing fixed 100-point array. The column workspace owns a
vector bounded by the largest source measure count plus one closing column.
It retains that high-water capacity across smaller or empty stages, so a
warm benchmark's lower churn does not mean the retained storage is free.
For 4,096 measures, column capacity is 98,328 bytes on this target. A new
larger stage can reallocate. The course builder owns scratch locally and
releases it before converting the combined vertex vector into an immutable
Arc. The destination still grows as needed and its final Arc copy remains.
There is no new global cache, persistent screen cache or unsafe code.
Requested bytes quantify allocation churn, not measured RSS or peak memory.

Graph points and course meshes are prepared when evaluation loads results.
The Barely marker is sampled when its graph subtree is rebuilt; the existing
actor cache still handles unchanged draws. This pass does not claim that
every evaluation frame previously built these meshes or sampled life.

## Behavior and validation

Five new regression tests cover:

- Scalar life sampling at chart anchors, exact timestamps, duplicate times,
  pre-chart records, empty histories, nonmonotonic records and nonfinite
  times/life values. All finite values, infinities and signed zeros match
  float bits; arithmetic NaNs match by classification.
- Every life-graph coordinate and all layout gates, including invalid and
  nonfinite dimensions and record boundaries.
- Appends after large/small/empty inputs, existing destination prefixes,
  short/missing timestamp arrays, duplicate times, nonfinite densities and
  translations. Existing owning density builders also match parent vertices.
- Complete course vertices and colors across stage counts, peaks, rates,
  empty/invalid stages, overflow durations and variable stage lengths.
- Reduced allocation churn with retained columns, zero-churn appends with
  both capacities warmed, and reduced cold whole-course allocation counts.

Validation results:

- New course-graph suite: **5 passed**, one manual benchmark ignored, in
  debug and release.
- Existing density-builder suite: **5 passed**, one manual benchmark ignored,
  in debug and release.
- Theme library: **1,279 passed**, five pre-existing ignored tests, serial.
- Performance Clippy: JSON diagnostics contain only the pre-existing
  `large_enum_variant` at unchanged `src/effects.rs:928`; no findings in
  changed code and no performance-lint allowances added.
- `cargo check -p deadsync`, scoped rustfmt, `git diff --check`, frozen-source
  audit and version audit passed.
- Version changes exactly 0.5.1211 -> 0.5.1212 in Cargo.toml and the three
  corresponding Cargo.lock entries. No dependency changes.

## Reproduce

```powershell
cargo test -p deadsync-theme-simply-love --test course_graphs --test density_build
cargo test -p deadsync-theme-simply-love --release --test course_graphs --test density_build
cargo test -p deadsync-theme-simply-love --lib -- --test-threads=1
cargo clippy -p deadsync-theme-simply-love --lib --test course_graphs --test density_build --no-deps -- -A clippy::all -W clippy::perf
cargo check -p deadsync
cargo test -p deadsync-theme-simply-love --release --test course_graphs benchmark_course_graphs -- --ignored --test-threads=1 --nocapture
```

Run the benchmark five times, setting `DEADSYNC_PERF_REVERSE=1` for runs 2
and 4 and removing it for runs 1, 3 and 5. The shared System allocator and
timer in `tests/support/perf.rs` provide counters. Non-Windows runs report
zero for unavailable thread cycles; those zeros are not CPU measurements.
