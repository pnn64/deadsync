# Histogram storage and smoothing - 0.5.1209

Baseline: `cb48d6353d9c474385d926f7038e10cb6b36b58c` / 0.5.1208.
This pass applies `rust-performance.md` guidance `M-HOTPATH`, `M-MEM-REUSE`
and `M-INITIAL-CAPACITY` to timing histograms used by evaluation and course
summaries. The public histogram types and entry points are unchanged.

1. **Borrow existing dense counts when building a histogram.** The common
   -512..511 ms counting path already populates a 1,024-element stack array.
   Private `HistCounts` now holds a borrowed or owned slice through `Cow`,
   allowing packing and smoothing to read that array directly. This removes
   a temporary heap allocation and copy. Wider counting paths retain their
   owned fallback. The returned bins and smoothed curve remain owned vectors.
2. **Reduce course-histogram merge work and storage.** Find bin bounds,
   capacity and display metadata together, then accumulate counts. Spans of
   up to 1,024 bins use a local stack array; spans of 1,025..4,096 still use
   a heap table. Larger sparse merges sort and coalesce the input pairs in
   their existing allocation, removing the second pair vector. Empty merges
   return before initializing scratch. Unordered inputs, duplicate counts,
   zero counts, and the original output capacity policy are preserved.
3. **Walk sparse bins during smoothing.** Sample positions are nondecreasing,
   including their clamped edges. One initial partition search skips bins
   before the visible interval, then a cursor advances through relevant bins.
   Dense lookup is selected once outside the loop. Both paths use the same
   seven multiply-add operations in the same order, preserving float bits.

Ordinary builds and small-span or sparse merges reduce allocation/free calls
from **three to two**. The 128-note fast build uses **4.4% fewer cycles**,
the eight-stage dense merge **8.0% fewer**, and sparse smoothing across
18,001 output points **46.6% fewer** in the final measurement set.
The smoothing improvement removes repeated searches; its one output allocation
remains. This is preparation work at evaluation/course transitions, not a
measurement of gameplay FPS or whole-application CPU usage.

## Measurement

Windows x86-64, Intel Xeon E5-2696 v4 at 2.20 GHz, Rust 1.98.0 (`88d9e12ae`),
repository release profile with opt-level 3 and LTO. The new rules-crate tests
compile the production functions and a frozen parent histogram implementation
into one binary. The frozen section contains counting, packing, merging and
smoothing; only private count visibility differs. Constants and unchanged
row-judgment selection are shared. The frozen section was checked against
the parent after normalizing whitespace and those visibility additions.

Five complete runs alternate old-first/new-first order. Every measurement
uses three warmups and seven timing samples; tables show medians of the five
per-run medians. Windows `QueryThreadCycleTime` measures calling-thread CPU
cycles. A separate operation counts allocations, reallocations, frees, requested
bytes and freed bytes, with counting disabled during timing. Prepared inputs
are outside measurement, while returned output destruction is included.
Inputs and outputs pass through black boxes. Builds and checks finish before
the final benchmark runs.

The host is shared, so background activity and processor/cache state can vary.
The [CSV](histogram-storage-0.5.1209.csv) includes all 310 measurements and
seven-sample wall-time ranges. These are synthetic fixtures, not a measured
distribution of player charts or courses. Requested bytes describe allocation
churn, not RSS, allocator metadata or peak live memory. Thread cycles are not
retired instructions or hardware cache misses.

## Results


25 of 31 workloads have lower median thread cycles. Negative cycle reductions below indicate slower results; no blanket speedup is claimed.

| Workload | Old ns/op | New ns/op | Thread cycles old/new | Cycle reduction | Throughput old/new (M units/s) |
|---|---:|---:|---:|---:|---:|
| `build_fast_0` | 3,800.0 | 3,249.6 | 8,300.7 / 7,126.5 | 14.1% | 0.263 / 0.308 |
| `build_shifted_0` | 3,556.4 | 3,228.5 | 7,799.5 / 7,051.9 | 9.6% | 0.281 / 0.310 |
| `build_sparse_0` | 3,657.0 | 3,445.1 | 8,019.0 / 7,558.6 | 5.7% | 0.273 / 0.290 |
| `build_fast_1` | 3,881.6 | 3,769.7 | 8,515.5 / 8,266.8 | 2.9% | 0.258 / 0.265 |
| `build_shifted_1` | 3,638.5 | 3,453.7 | 7,979.2 / 7,574.5 | 5.1% | 0.275 / 0.290 |
| `build_sparse_1` | 3,561.9 | 3,448.0 | 7,813.3 / 7,564.6 | 3.2% | 0.281 / 0.290 |
| `build_fast_128` | 5,441.2 | 5,204.1 | 11,926.7 / 11,407.1 | 4.4% | 23.524 / 24.596 |
| `build_shifted_128` | 6,326.4 | 6,065.0 | 13,854.7 / 13,300.8 | 4.0% | 20.233 / 21.105 |
| `build_sparse_128` | 8,046.1 | 7,060.9 | 17,647.9 / 15,489.3 | 12.2% | 15.908 / 18.128 |
| `build_fast_8192` | 92,059.4 | 96,207.8 | 201,881.7 / 210,744.0 | -4.4% | 88.986 / 85.149 |
| `build_shifted_8192` | 141,659.4 | 145,850.0 | 310,685.1 / 319,609.2 | -2.9% | 57.829 / 56.167 |
| `build_sparse_8192` | 211,643.8 | 205,764.1 | 463,882.4 / 450,907.9 | 2.8% | 38.707 / 39.813 |
| `merge_dense_0` | 13.3 | 13.7 | 35.2 / 35.2 | 0.0% | 75.294 / 73.143 |
| `merge_wide_dense_0` | 15.6 | 15.6 | 75.4 / 82.3 | -9.2% | 64.000 / 64.000 |
| `merge_sparse_0` | 15.6 | 15.6 | 75.5 / 82.3 | -9.0% | 64.000 / 64.000 |
| `merge_dense_1` | 8,456.6 | 7,234.8 | 18,554.6 / 15,870.9 | 14.5% | 0.118 / 0.138 |
| `merge_wide_dense_1` | 57,193.8 | 53,862.5 | 125,423.7 / 118,084.2 | 5.9% | 0.017 / 0.019 |
| `merge_sparse_1` | 386,131.2 | 312,709.4 | 846,344.0 / 680,710.6 | 19.6% | 0.003 / 0.003 |
| `merge_dense_8` | 10,932.0 | 10,055.5 | 23,984.7 / 22,055.5 | 8.0% | 0.732 / 0.796 |
| `merge_wide_dense_8` | 53,850.0 | 55,203.1 | 118,255.6 / 121,232.6 | -2.5% | 0.149 / 0.145 |
| `merge_sparse_8` | 383,321.9 | 283,971.9 | 840,108.9 / 622,227.6 | 25.9% | 0.021 / 0.028 |
| `merge_dense_64` | 40,159.8 | 39,820.3 | 88,033.2 / 87,319.8 | 0.8% | 1.594 / 1.607 |
| `merge_wide_dense_64` | 70,328.1 | 67,503.1 | 153,814.6 / 148,066.5 | 3.7% | 0.910 / 0.948 |
| `merge_sparse_64` | 399,218.8 | 295,387.5 | 874,995.6 / 647,531.9 | 26.0% | 0.160 / 0.217 |
| `smooth_dense_42` | 1,577.7 | 1,309.0 | 3,467.4 / 2,878.4 | 17.0% | 53.875 / 64.936 |
| `smooth_dense_180` | 5,792.6 | 5,623.0 | 12,706.1 / 12,335.7 | 2.9% | 62.321 / 64.200 |
| `smooth_dense_900` | 29,021.1 | 26,897.3 | 63,475.8 / 58,959.8 | 7.1% | 62.058 / 66.958 |
| `smooth_sparse_42` | 1,848.8 | 1,533.2 | 4,026.5 / 3,370.5 | 16.3% | 45.975 / 55.439 |
| `smooth_sparse_180` | 8,735.2 | 5,882.8 | 19,126.5 / 12,901.6 | 32.5% | 41.327 / 61.365 |
| `smooth_sparse_900` | 51,580.5 | 32,263.7 | 113,085.4 / 70,708.2 | 37.5% | 34.916 / 55.821 |
| `smooth_sparse_9000` | 585,962.5 | 313,178.1 | 1,284,596.3 / 686,602.8 | 46.6% | 30.720 / 57.478 |

Every measurement has zero reallocations. Allocation and free counts match, as do requested and freed bytes.

| Workload | Allocations/frees old/new | Requested/freed bytes old/new |
|---|---:|---:|
| `build_fast_0` | 1 / 1 | 1,672 / 1,672 |
| `build_shifted_0` | 1 / 1 | 1,672 / 1,672 |
| `build_sparse_0` | 1 / 1 | 1,672 / 1,672 |
| `build_fast_1` | 3 / 2 | 1,684 / 1,680 |
| `build_shifted_1` | 3 / 3 | 1,684 / 1,684 |
| `build_sparse_1` | 3 / 3 | 1,684 / 1,684 |
| `build_fast_128` | 3 / 2 | 3,124 / 2,640 |
| `build_shifted_128` | 3 / 3 | 3,124 / 3,124 |
| `build_sparse_128` | 3 / 3 | 3,208 / 3,208 |
| `build_fast_8192` | 3 / 2 | 3,124 / 2,640 |
| `build_shifted_8192` | 3 / 3 | 3,124 / 3,124 |
| `build_sparse_8192` | 3 / 3 | 99,976 / 99,976 |
| `merge_dense_0` | 0 / 0 | 0 / 0 |
| `merge_wide_dense_0` | 0 / 0 | 0 / 0 |
| `merge_sparse_0` | 0 / 0 | 0 / 0 |
| `merge_dense_1` | 3 / 2 | 7,220 / 5,776 |
| `merge_wide_dense_1` | 3 / 3 | 26,916 / 26,916 |
| `merge_sparse_1` | 3 / 2 | 144,312 / 144,160 |
| `merge_dense_8` | 3 / 2 | 7,220 / 5,776 |
| `merge_wide_dense_8` | 3 / 3 | 40,932 / 40,932 |
| `merge_sparse_8` | 3 / 2 | 146,440 / 145,224 |
| `merge_dense_64` | 3 / 2 | 7,220 / 5,776 |
| `merge_wide_dense_64` | 3 / 3 | 40,932 / 40,932 |
| `merge_sparse_64` | 3 / 2 | 163,464 / 153,736 |
| `smooth_dense_42` | 1 / 1 | 680 / 680 |
| `smooth_dense_180` | 1 / 1 | 2,888 / 2,888 |
| `smooth_dense_900` | 1 / 1 | 14,408 / 14,408 |
| `smooth_sparse_42` | 1 / 1 | 680 / 680 |
| `smooth_sparse_180` | 1 / 1 | 2,888 / 2,888 |
| `smooth_sparse_900` | 1 / 1 | 14,408 / 14,408 |
| `smooth_sparse_9000` | 1 / 1 | 144,008 / 144,008 |

Higher cycle medians occurred in these cases:

- `build_fast_8192`: 4.4% more cycles; 92,059.4 -> 96,207.8 ns/op.
- `build_shifted_8192`: 2.9% more cycles; 141,659.4 -> 145,850.0 ns/op.
- `merge_wide_dense_0`: 9.2% more cycles; 15.6 -> 15.6 ns/op.
- `merge_sparse_0`: 9.0% more cycles; 15.6 -> 15.6 ns/op.
- `merge_wide_dense_8`: 2.5% more cycles; 53,850.0 -> 55,203.1 ns/op.

## Storage and workload limits

Builds already reserved a 4 KiB stack counting array; the common path now
borrows it instead of copying the used span to the heap. Merging adds up to
4 KiB of local stack scratch. No cache or global retained storage is added.
Returned vector capacities remain unchanged; output allocation is still needed.
The sparse merge removes an intermediate vector equal in capacity to the
returned bin vector. Smaller heap churn therefore does not imply zero total
memory or a measured reduction in RSS.

Build fixtures contain 0, 1, 128 or 8,192 distinct tap-note rows graded Excellent.
`fast` offsets cycle from -60 to +60 ms; `shifted` offsets run from 600 to
720 ms, exercising owned dense fallback; `sparse` offsets cycle through 13
multiples of 1,000 ms from -6,000 to +6,000. The single `sparse` note therefore
has a one-bin dense table outside the fast interval. These outlier offsets
exercise compatibility paths, not ordinary judgment windows. Builds use 512
iterations per sample, or 64 for 8,192 rows. Throughput counts input notes,
with empty inputs counting one operation.

Merge fixtures contain 0, 1, 8 or 64 histograms. `dense` has every integer bin
from -180 to +180; `wide_dense` uses every seventh bin starting at -1,024
(ending at +1,020, a 2,045-bin span); `sparse` uses every thousandth bin from
-9,000 to +9,000. Counts vary by bin and stage. Metadata windows are 180,
1,024 or 9,000 ms respectively. The 64-stage fixture is a scaling case.
Throughput counts input histograms, or one empty operation. Dense merges use
256 iterations per sample; wide/sparse merges use 32.

Smoothing fixtures have nonzero bins every seven milliseconds, with varying
counts, inside the named radius. Dense fixtures also provide the full count
array. Their 42, 180, 900 and 9,000 ms radii yield `2 * radius + 1` output
points; throughput counts those points. Samples use 256 iterations, or 32
at radius 9,000. The large sparse windows demonstrate scaling. Repeated empty
build/merge rows intentionally use the same empty input under each fixture
family; differences between them expose measurement variability.

## Behavior and validation

Five new tests compare complete raw bins, peak counts, metadata and every
smoothed float bit against the parent. Coverage includes empty/unjudged input,
jumps, mixed grades, misses, mines, lifts, holds, rolls, fake/disabled notes,
offset-floor boundaries, all three count-storage paths, filtered iterators,
unsorted and duplicate bins, zero counts, large counts, integer endpoint bins,
NaN metadata, and span transitions at 1,024 and 4,096 bins. Smoothing tests
exercise repeated clamped samples, negative/zero windows, dense/sparse inputs,
and long prefixes outside the displayed interval. Retained results remain
unchanged through later calls and input replacement.

Allocation assertions require at most two output allocations for common builds
and applicable merges, and zero churn for empty merges. Existing histogram
tests retain their older smoother oracle; their fixture merely adapts to the
private borrowed/owned count representation.

- Rules suite: **114 passed**, three manual benchmarks ignored, in both debug and release.
- Score suite: **241 passed**, five pre-existing ignored tests, serial.
- Theme suite: **1,279 passed**, five pre-existing ignored tests, serial.
- Performance Clippy for the rules library and tests: passed without exceptions.
- `cargo check -p deadsync`: passed.

Scoped rustfmt and `git diff --check` pass. The version audit confirms exactly
0.5.1208 -> 0.5.1209 in the workspace manifest and all three matching lock entries.
No dependencies or unsafe code were added.

## Reproduce

```powershell
cargo test -p deadsync-rules --lib
cargo test -p deadsync-rules --release --lib
cargo test -p deadsync-score --lib -- --test-threads=1
cargo test -p deadsync-theme-simply-love --lib -- --test-threads=1
cargo clippy -p deadsync-rules --lib --tests --no-deps -- -A clippy::all -D clippy::perf
cargo check -p deadsync
cargo test -p deadsync-rules --release --lib benchmark_histogram_storage -- --ignored --test-threads=1 --nocapture
```

Repeat the last command five times, setting `DEADSYNC_PERF_REVERSE=1` for runs
2 and 4 and removing it for runs 1, 3 and 5. The benchmark uses the existing
allocator and timing implementation in `tests/support/perf.rs`.
