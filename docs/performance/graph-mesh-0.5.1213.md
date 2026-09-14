# Evaluation graph meshes - 0.5.1213

Baseline: `30bbc73c204ebe2d433323430651436931394b36` / 0.5.1212.
This pass applies the supplied `rust-performance.md` guidance on measuring
CPU and allocation costs (M-HOTPATH), avoiding short-lived allocations, and
sizing collections for their output (M-INITIAL-CAPACITY).

Three changes:

1. **Keep judgment-band descriptors on the stack.** Each background used a
   temporary Vec for five, six or seven fixed descriptors. Borrowed arrays
   remove that allocation/free pair; only the returned vertex Vec allocates.
2. **Size long scatter meshes for visible points.** A rejected prefix is
   skipped before allocating, so entirely clipped plots allocate nothing.
   Remaining suffixes of at least 4,096 points are counted before reserving
   exactly six vertices per accepted point. Shorter suffixes use an upper
   estimate to avoid the extra scan. Misses and NaN offsets remain accepted,
   matching the original drawing predicate.
3. **Write each triangle pair in one vector extension.** Scatter/background
   quads and histogram segments emit a six-vertex array in their original
   order. One extension replaces six individual push capacity checks.
   All coordinate and color arithmetic remains unchanged.

The mixed 8,192-point Hard EX plot uses **74.0% fewer
thread cycles** and requests **1,179,648 -> 315,648 bytes** for its vertex Vec.
Including the evaluation screen's final immutable Arc conversion, this case
uses **65.3% fewer cycles**, removes the shrink
reallocation, and requests **1,810,960 -> 631,312 bytes** in total. Dense raw
ITG histogram construction uses **42.1% fewer cycles**
with the same allocation count and bytes. Standard ITG background bands use
**25.0% fewer cycles** and **two allocations/frees -> one**.

These are preparation-function measurements. Evaluation builds and retains
these meshes when results load; it already reuses them during unchanged
draws. No whole-game CPU, frame latency, or FPS improvement is claimed.
Scatter gains combine sizing and batched writes; background gains combine
stack descriptors and batched writes. Those gains must not be added together.

## Measurement

Windows x86-64, Intel Xeon E5-2696 v4 at 2.20 GHz, Rust 1.98.0
(`88d9e12ae`, 2026-08-18), repository release profile (opt-level 3, LTO).
The test calls the current public library APIs and compiles frozen parent
builders using the same unchanged timing, palette, vertex and arrow-color
helpers. The complete frozen production module was audited against the
parent, allowing formatting and its provenance comment only.

Five complete runs alternate old-first/new-first; each measurement uses
three warmups and seven timing samples. Tables report medians of the five
per-run medians. Windows `QueryThreadCycleTime` measures calling-thread CPU
cycles separately from the System allocator counters. No worker threads are
started by these operations. Fixtures are allocated outside measurement;
inputs/results use black boxes. Each owning operation includes dropping its
result. Retained cases reproduce Vec -> boxed slice -> Arc and include the
final Arc's destruction. No builds or tests started by this pass run
concurrently with the final benchmarks; the shared host and allocator state
can still introduce variation.

The [CSV](graph-mesh-0.5.1213.csv) contains all 770 measurements, including
timing ranges, throughput, allocations, reallocations, frees, and requested
and freed bytes. Requested bytes measure allocator traffic, not peak live
memory or process RSS. Stack storage and fixture allocations are excluded
from heap counters. No new global cache, persistent workspace, unsafe code,
dependency or public API is introduced.

## Results

66 of 77 workloads have lower median thread cycles; 5 are equal and
6 are higher. Negative reductions indicate slower measurements.
All controls are retained; no workload increases allocation count,
reallocation count, frees, requested bytes or freed bytes.

| Workload | Old ns/op | New ns/op | Thread cycles old/new | Cycle reduction | Throughput old/new (M units/s) |
|---|---:|---:|---:|---:|---:|
| `bands_itg_10` | 337.1 | 99.6 | 742.1 / 221.2 | 70.2% | 2.966 / 10.039 |
| `bands_itg_180` | 230.3 | 172.3 | 507.6 / 380.7 | 25.0% | 4.343 / 5.805 |
| `bands_ex_10` | 315.0 | 118.8 | 693.7 / 263.7 | 62.0% | 3.174 / 8.421 |
| `bands_ex_180` | 272.5 | 192.6 | 600.6 / 425.3 | 29.2% | 3.670 / 5.193 |
| `bands_hard_10` | 374.8 | 111.5 | 824.8 / 250.8 | 69.6% | 2.668 / 8.967 |
| `bands_hard_180` | 211.3 | 158.6 | 455.7 / 351.1 | 23.0% | 4.732 / 6.305 |
| `bands_arrow_10` | 25.8 | 22.7 | 59.2 / 52.3 | 11.7% | 38.788 / 44.138 |
| `bands_arrow_180` | 24.8 | 22.9 | 58.7 / 52.3 | 10.9% | 40.315 / 43.761 |
| `scatter_itg_hits_0` | 26.6 | 27.7 | 63.4 / 66.0 | -4.1% | 37.647 / 36.056 |
| `scatter_hard_hits_0` | 27.0 | 27.3 | 65.2 / 64.3 | 1.4% | 37.101 / 36.571 |
| `scatter_itg_mixed_0` | 27.3 | 27.3 | 65.2 / 65.2 | 0.0% | 36.571 / 36.571 |
| `scatter_hard_mixed_0` | 27.3 | 26.2 | 64.3 / 63.4 | 1.4% | 36.571 / 38.209 |
| `scatter_itg_rejected_0` | 27.0 | 27.7 | 64.3 / 66.0 | -2.6% | 37.101 / 36.056 |
| `scatter_hard_rejected_0` | 25.8 | 27.3 | 61.7 / 65.2 | -5.7% | 38.788 / 36.571 |
| `scatter_itg_hits_64` | 1,241.8 | 970.3 | 2,731.7 / 2,122.1 | 22.3% | 51.538 / 65.958 |
| `scatter_hard_hits_64` | 1,142.2 | 929.3 | 2,513.1 / 2,045.0 | 18.6% | 56.033 / 68.869 |
| `scatter_itg_mixed_64` | 972.7 | 899.6 | 2,123.0 / 1,978.9 | 6.8% | 65.799 / 71.142 |
| `scatter_hard_mixed_64` | 334.4 | 314.8 | 738.2 / 695.4 | 5.8% | 191.402 / 203.275 |
| `scatter_itg_rejected_64` | 144.1 | 80.9 | 321.5 / 182.6 | 43.2% | 444.011 / 791.498 |
| `scatter_hard_rejected_64` | 149.2 | 81.6 | 334.4 / 185.2 | 44.6% | 428.901 / 783.923 |
| `scatter_itg_hits_256` | 5,918.8 | 3,778.1 | 13,032.8 / 8,327.3 | 36.1% | 43.252 / 67.758 |
| `scatter_hard_hits_256` | 5,178.1 | 3,781.2 | 11,407.2 / 8,341.0 | 26.9% | 49.439 / 67.702 |
| `scatter_itg_mixed_256` | 4,737.5 | 3,368.8 | 10,440.0 / 7,428.7 | 28.8% | 54.037 / 75.993 |
| `scatter_hard_mixed_256` | 1,265.6 | 1,203.1 | 2,812.3 / 2,682.0 | 4.6% | 202.272 / 212.779 |
| `scatter_itg_rejected_256` | 409.4 | 234.4 | 932.9 / 548.8 | 41.2% | 625.344 / 1092.267 |
| `scatter_hard_rejected_256` | 396.9 | 228.1 | 905.4 / 535.0 | 40.9% | 645.039 / 1122.192 |
| `scatter_itg_hits_1024` | 21,109.4 | 13,875.0 | 46,211.6 / 30,414.5 | 34.2% | 48.509 / 73.802 |
| `scatter_hard_hits_1024` | 21,593.8 | 13,940.6 | 47,453.2 / 30,572.2 | 35.6% | 47.421 / 73.454 |
| `scatter_itg_mixed_1024` | 19,168.8 | 13,615.6 | 40,936.8 / 29,886.3 | 27.0% | 53.420 / 75.208 |
| `scatter_hard_mixed_1024` | 5,753.1 | 4,318.8 | 12,669.3 / 9,520.8 | 24.9% | 177.990 / 237.106 |
| `scatter_itg_rejected_1024` | 1,046.9 | 846.9 | 2,339.0 / 1,900.0 | 18.8% | 978.149 / 1209.151 |
| `scatter_hard_rejected_1024` | 1,009.4 | 843.8 | 2,284.2 / 1,893.2 | 17.1% | 1014.489 / 1213.630 |
| `scatter_itg_hits_4096` | 88,703.1 | 64,159.4 | 191,932.2 / 140,480.0 | 26.8% | 46.177 / 63.841 |
| `scatter_hard_hits_4096` | 94,400.0 | 68,868.8 | 203,956.6 / 151,084.6 | 25.9% | 43.390 / 59.475 |
| `scatter_itg_mixed_4096` | 84,218.8 | 60,996.9 | 183,440.3 / 133,078.8 | 27.5% | 48.635 / 67.151 |
| `scatter_hard_mixed_4096` | 25,059.4 | 22,437.5 | 54,909.3 / 49,154.3 | 10.5% | 163.452 / 182.552 |
| `scatter_itg_rejected_4096` | 4,531.2 | 3,818.8 | 9,987.2 / 8,423.3 | 15.7% | 903.945 / 1072.602 |
| `scatter_hard_rejected_4096` | 4,740.6 | 4,400.0 | 10,446.8 / 9,706.0 | 7.1% | 864.021 / 930.909 |
| `scatter_itg_hits_8192` | 585,200.0 | 535,693.8 | 1,280,453.2 / 1,172,191.7 | 8.5% | 13.999 / 15.292 |
| `scatter_hard_hits_8192` | 585,128.1 | 500,143.8 | 1,281,207.8 / 1,092,979.7 | 14.7% | 14.000 / 16.379 |
| `scatter_itg_mixed_8192` | 526,984.4 | 503,071.9 | 1,151,915.5 / 1,100,360.4 | 4.5% | 15.545 / 16.284 |
| `scatter_hard_mixed_8192` | 192,843.8 | 50,378.1 | 420,342.5 / 109,256.1 | 74.0% | 42.480 / 162.610 |
| `scatter_itg_rejected_8192` | 27,528.1 | 9,193.8 | 60,479.1 / 20,098.0 | 66.8% | 297.587 / 891.040 |
| `scatter_hard_rejected_8192` | 27,475.0 | 9,121.9 | 60,362.5 / 20,070.5 | 66.8% | 298.162 / 898.061 |
| `scatter_itg_hits_65536` | 3,812,912.5 | 3,424,275.0 | 8,348,929.4 / 7,493,922.0 | 10.2% | 17.188 / 19.139 |
| `scatter_hard_hits_65536` | 3,809,237.5 | 3,399,600.0 | 8,306,044.9 / 7,434,327.9 | 10.5% | 17.204 / 19.278 |
| `scatter_itg_mixed_65536` | 3,518,187.5 | 3,097,650.0 | 7,693,063.6 / 6,786,638.1 | 11.8% | 18.628 / 21.157 |
| `scatter_hard_mixed_65536` | 1,130,600.0 | 1,154,712.5 | 2,477,139.8 / 2,529,737.5 | -2.1% | 57.966 / 56.755 |
| `scatter_itg_rejected_65536` | 110,237.5 | 77,900.0 | 240,901.2 / 170,606.2 | 29.2% | 594.498 / 841.284 |
| `scatter_hard_rejected_65536` | 116,650.0 | 73,225.0 | 256,266.2 / 160,372.1 | 37.4% | 561.817 / 894.995 |
| `scatter_ex_mixed_8192` | 554,581.2 | 504,459.4 | 1,213,629.2 / 1,105,196.2 | 8.9% | 14.772 / 16.239 |
| `scatter_arrow_mixed_8192` | 551,800.0 | 516,096.9 | 1,206,824.7 / 1,123,174.6 | 6.9% | 14.846 / 15.873 |
| `scatter_quant_mixed_8192` | 565,728.1 | 515,300.0 | 1,229,220.6 / 1,125,891.0 | 8.4% | 14.480 / 15.898 |
| `scatter_foot_mixed_8192` | 529,103.1 | 498,112.5 | 1,154,275.0 / 1,089,721.4 | 5.6% | 15.483 / 16.446 |
| `scatter_itg_miss_8192` | 583,506.2 | 520,996.9 | 1,276,906.9 / 1,139,822.3 | 10.7% | 14.039 / 15.724 |
| `scatter_hard_miss_8192` | 543,109.4 | 485,296.9 | 1,188,373.0 / 1,061,879.2 | 10.6% | 15.084 / 16.880 |
| `retained_hard_hits_8192` | 946,443.8 | 905,268.8 | 2,070,406.3 / 1,982,242.8 | 4.3% | 8.656 / 9.049 |
| `retained_hard_mixed_8192` | 215,131.2 | 74,718.8 | 471,760.3 / 163,932.2 | 65.3% | 38.079 / 109.638 |
| `retained_hard_rejected_8192` | 26,462.5 | 8,406.2 | 58,057.8 / 18,506.6 | 68.1% | 309.570 / 974.513 |
| `hist_itg_center_raw` | 22.7 | 22.7 | 60.0 / 60.0 | 0.0% | 44.138 / 44.138 |
| `hist_itg_center_smooth` | 22.7 | 22.7 | 58.3 / 58.3 | 0.0% | 44.138 / 44.138 |
| `hist_ex_center_raw` | 22.7 | 22.7 | 58.3 / 60.0 | -2.9% | 44.138 / 44.138 |
| `hist_ex_center_smooth` | 22.7 | 22.7 | 60.0 / 60.0 | 0.0% | 44.138 / 44.138 |
| `hist_hard_center_raw` | 21.9 | 22.7 | 56.6 / 60.0 | -6.0% | 45.714 / 44.138 |
| `hist_hard_center_smooth` | 22.7 | 23.4 | 60.0 / 60.0 | 0.0% | 44.138 / 42.667 |
| `hist_itg_sparse_raw` | 17,611.7 | 13,748.4 | 38,630.3 / 30,104.1 | 22.1% | 0.511 / 0.655 |
| `hist_itg_sparse_smooth` | 15,782.0 | 11,900.8 | 34,615.8 / 26,074.2 | 24.7% | 0.570 / 0.756 |
| `hist_ex_sparse_raw` | 12,975.8 | 8,856.2 | 28,439.0 / 19,336.6 | 32.0% | 0.694 / 1.016 |
| `hist_ex_sparse_smooth` | 9,670.3 | 6,700.0 | 21,238.3 / 14,679.1 | 30.9% | 0.931 / 1.343 |
| `hist_hard_sparse_raw` | 1,075.0 | 918.8 | 2,369.9 / 1,977.2 | 16.6% | 8.372 / 9.796 |
| `hist_hard_sparse_smooth` | 1,300.0 | 957.0 | 2,862.1 / 2,111.0 | 26.2% | 6.923 / 9.404 |
| `hist_itg_dense_raw` | 10,651.6 | 6,168.8 | 23,390.5 / 13,554.1 | 42.1% | 33.892 / 58.521 |
| `hist_itg_dense_smooth` | 8,952.3 | 5,320.3 | 19,640.1 / 11,667.8 | 40.6% | 40.325 / 67.853 |
| `hist_ex_dense_raw` | 8,536.7 | 4,786.7 | 18,153.3 / 10,494.8 | 42.2% | 42.288 / 75.417 |
| `hist_ex_dense_smooth` | 8,406.2 | 4,657.8 | 18,427.7 / 10,215.3 | 44.6% | 42.944 / 77.504 |
| `hist_hard_dense_raw` | 1,445.3 | 1,165.6 | 3,182.8 / 2,567.1 | 19.3% | 249.773 / 309.705 |
| `hist_hard_dense_smooth` | 1,735.2 | 1,091.4 | 3,758.9 / 2,416.2 | 35.7% | 208.050 / 330.766 |

Allocation/free counts and requested/freed bytes match for each operation.
All nonempty histogram cases retain one allocation, no reallocation, and the
same requested bytes. Fully visible scatter cases also retain their previous
counts and bytes. Representative storage results:

| Workload | Allocations/frees old/new | Reallocations old/new | Requested/freed bytes old/new |
|---|---:|---:|---:|
| `bands_itg_180` | 2 / 1 | 0 / 0 | 1,540 / 1,440 |
| `bands_ex_180` | 2 / 1 | 0 / 0 | 1,848 / 1,728 |
| `bands_hard_180` | 2 / 1 | 0 / 0 | 2,156 / 2,016 |
| `scatter_hard_mixed_64` | 1 / 1 | 0 / 0 | 9,216 / 9,216 |
| `scatter_hard_mixed_256` | 1 / 1 | 0 / 0 | 36,864 / 36,864 |
| `scatter_hard_mixed_1024` | 1 / 1 | 0 / 0 | 147,456 / 147,456 |
| `scatter_hard_mixed_4096` | 1 / 1 | 0 / 0 | 589,824 / 157,824 |
| `scatter_hard_mixed_8192` | 1 / 1 | 0 / 0 | 1,179,648 / 315,648 |
| `scatter_hard_mixed_65536` | 1 / 1 | 0 / 0 | 9,437,184 / 2,526,624 |
| `scatter_itg_rejected_8192` | 1 / 0 | 0 / 0 | 1,179,648 / 0 |
| `scatter_itg_rejected_65536` | 1 / 0 | 0 / 0 | 9,437,184 / 0 |
| `retained_hard_hits_8192` | 2 / 2 | 0 / 0 | 2,359,312 / 2,359,312 |
| `retained_hard_mixed_8192` | 2 / 2 | 1 / 0 | 1,810,960 / 631,312 |
| `retained_hard_rejected_8192` | 1 / 0 | 0 / 0 | 1,179,648 / 0 |
| `hist_itg_dense_raw` | 1 / 1 | 0 / 0 | 51,840 / 51,840 |

Higher median cycle measurements:

- `scatter_itg_hits_0`: 4.1% more cycles; 26.6 -> 27.7 ns/op.
- `scatter_itg_rejected_0`: 2.6% more cycles; 27.0 -> 27.7 ns/op.
- `scatter_hard_rejected_0`: 5.7% more cycles; 25.8 -> 27.3 ns/op.
- `scatter_hard_mixed_65536`: 2.1% more cycles; 1,130,600.0 -> 1,154,712.5 ns/op.
- `hist_ex_center_raw`: 2.9% more cycles; 22.7 -> 22.7 ns/op.
- `hist_hard_center_raw`: 6.0% more cycles; 21.9 -> 22.7 ns/op.

The only nonempty workload with a higher cycle median is the 65,536-point
mixed Hard EX plot: 2.1% more cycles with 73.2% fewer requested bytes. The
other five slower cases return empty meshes; their elapsed medians differ
by at most 1.5 ns. The 4,096-point cutoff retains the parent's small-plot
capacity estimate to avoid a costly sizing tradeoff at smaller inputs.

## Workloads and storage bounds

Background cases use width 854, height 64 and requested windows 10 or 180 ms,
with ITG, EX, Hard EX and the empty Arrow-background control. Each sample
contains 512 operations; throughput counts complete graphs. Descriptor
arrays contain at most seven `(f32, [f32; 4])` entries (140 bytes of values);
the compiler controls their actual stack placement. The vertex reservation
retains the old maximum of 12 vertices per descriptor, even when a narrow
window emits fewer bands.

Scatter sizes are 0, 64, 256, 1,024, 4,096, 8,192 and 65,536 points. Timestamps are
`index * 0.025`, first time -0.1, last time `max(count, 1) * 0.025`, graph size
854 by 64, requested window 180 ms, and dimming after `count * 0.015`.
Hit offsets are `(index * 7 % 31) - 15` ms. Mixed plots use a miss every 17th
point and otherwise `(index * 37 % 401) - 200` ms. Rejected plots contain
250 ms offsets. Both ITG and Hard EX cover all sizes/shapes; EX, Arrow,
Quant and FootParity add mixed 8,192-point plots, and ITG/Hard EX add
miss-only plots. Direction codes cycle 0..10, quantizations 0..11, parity
cycles all four values, and every third miss is held. Retained Arc cases
use the three Hard EX shapes at 8,192 points.

Scatter timing samples contain 256 operations up to 64 points, 32 up to
8,192, and eight for 65,536. Throughput counts input points, including those
clipped, or one empty operation. The extra counting pass is used only when
the suffix after the rejected prefix has at least 4,096 points. A smaller
suffix reserves less than 576 KiB of vertices, never more than the parent;
its final boxed-slice conversion may still shrink. Long accepted outputs
reserve exactly their visible length and need no shrink reallocation. The
final Arc copy remains. Conversion consumes the temporary vertex Vec; the
resulting Arc keeps its normal screen lifetime. There is no new retained
workspace.

Histogram fixtures cover bins -180..180 inclusive: dense uses all bins,
sparse uses multiples of 41, and the center-only control uses bin zero.
Counts are `abs(bin * 13) % 100 + 1`, maximum count 100, and smoothed values
are `abs(bin * 7) % 100 + 0.125` in a full centered 361-bin domain. Graph
dimensions are pane width 300, graph height 141 and pane height 180. All
three scales run raw and smoothed; each sample has 128 operations and
throughput counts source bins. These deterministic scaling fixtures satisfy
the builders' input contracts; they are not a measured real-chart distribution.

## Behavior and validation

Five new tests compare every vertex with the parent, preserving output
length, order, positions and colors. Finite floats, infinities and signed
zeros match bits; arithmetic NaNs match by classification. Coverage includes
all six scatter scales, three histogram scales, custom palettes, exact
timing boundaries and neighboring floats, misses/held misses, failure
dimming, nonfinite values, empty/invalid dimensions, raw and smoothed
histograms, long charts, allocation thresholds and rejected prefixes.
Allocation tests verify the removed band allocation, zero heap churn for
fully rejected plots, and visible-length reservations for long plots.

Validation results:

- New graph-mesh suite: **5 passed**, one manual benchmark ignored, in both
  debug and release.
- Theme library: **1,279 passed**, five pre-existing ignored tests, serial.
- Performance Clippy: only the pre-existing `large_enum_variant` in
  unchanged `src/effects.rs:928`; no findings in changed code and no
  performance-lint allowances added.
- `cargo check -p deadsync`, scoped rustfmt, `git diff --check`, frozen-source
  and version audits passed.
- Exact patch increment 0.5.1212 -> 0.5.1213 in Cargo.toml and the three
  corresponding Cargo.lock packages, with no dependency changes.

## Reproduce

```powershell
cargo test -p deadsync-theme-simply-love --test graph_mesh
cargo test -p deadsync-theme-simply-love --release --test graph_mesh
cargo test -p deadsync-theme-simply-love --lib -- --test-threads=1
cargo clippy -p deadsync-theme-simply-love --lib --test graph_mesh --no-deps -- -A clippy::all -W clippy::perf
cargo check -p deadsync
cargo test -p deadsync-theme-simply-love --release --test graph_mesh benchmark_graph_mesh -- --ignored --test-threads=1 --nocapture
```

Run the benchmark five times, setting `DEADSYNC_PERF_REVERSE=1` for runs 2
and 4 and removing it for runs 1, 3 and 5. The unchanged allocator/timer in
`tests/support/perf.rs` provides the counters. Non-Windows runs report zero
for unavailable thread cycles; those zeros are not CPU measurements.
