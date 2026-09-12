# Sync graph preparation performance - 0.5.1147

This pass applies `M-HOTPATH`, `M-INITIAL-CAPACITY`, `M-MEM-REUSE`, and
`M-THROUGHPUT` from the supplied `rust-performance.md` to sync-analysis graphs.
It measures graph preparation, not audio analysis, GPU upload, full UI latency,
or gameplay FPS.

## Three changes

1. **Select percentile endpoints instead of sorting the entire matrix.** Large
   unordered inputs copy once and select four interpolation ranks, reusing
   partitions and duplicate ranks. Ordered inputs are read directly; small
   inputs use a 32-element stack buffer. Interpolation and total float ordering
   are preserved. The old large stable sort allocated both a copy and sort scratch.
2. **Reuse heatmap colors and completed rows during nearest-neighbor scaling.**
   Each sampled value is converted once when the matrix axis is enlarged, then
   its pixel run is filled. Repeated output rows copy the previous row. Missing
   streamed rows remain transparent. Downsampling converts each displayed value.
   No extra heap scratch is required beyond the output image when percentile
   limits are disabled.
3. **Retain curve vertex storage and avoid the intermediate mesh copy.** The
   overlay owns `Arc<Vec<MeshVertex>>` through the existing `ReusableMesh` actor.
   Unique storage is cleared/refilled; shared storage is replaced so previously
   emitted actors remain immutable. Cold/shared updates keep the generated
   vector instead of copying it into a second large allocation. Curve arithmetic
   and vertex order remain unchanged.

Production changes use safe Rust and introduce no dependencies or global caches.
Pure graph preparation is now a private module compiled unchanged in the tests.
Curve capacity persists for the overlay lifetime; growth can allocate, and
invalid/empty output releases storage. Shared actors or weak references prevent
reuse. Heatmap ownership still requires an output allocation, and large unordered
percentiles still copy input. Allocation-free refresh requires sufficient retained
capacity and unique ownership; this is not an allocation-free UI-frame claim.

## Method and workloads

Baseline: `2230885b3` / 0.5.1146. All ten frozen function bodies match that commit
ignoring whitespace; imports/visibility differ and the unchanged column descriptor
is shared. Windows x64, Intel Xeon E5-2696 v4 at 2.20 GHz, rustc 1.98.0
(`88d9e12ae`, LLVM 22.1.8), repository release profile, opt-level 3 and full LTO.

Old/new routines share one executable, fixtures, and the repository's System
allocator wrapper (`tests/support/perf.rs`). Inputs/outputs are black-boxed;
percentile/image/cold-curve pairs use opaque function pointers. Timing disables
allocation counters. Each workload warms three times and times seven batches;
a separate operation counts allocations/reallocations/frees and requested/freed
bytes. Windows QueryThreadCycleTime measures calling-thread CPU cycles; these
routines do not launch workers. Tables are medians of three invocation medians,
ordered old/new, new/old, old/new, with no builds or tests during final timing.
Fixture generation is excluded; output creation/replacement/disposal is included.
Refresh cases retain outputs across calls and separately cover unique/shared ownership.

Fixtures are deterministic synthetic matrices, not recorded user sessions:

- Percentiles: empty, four values, 65,536 values (random, sorted, reversed,
  constant, seven repeating values), and 524,288 random values; limits 10/90.
- Heatmaps: 512x132 output. Vertical enlargement uses 64 columns x 32 rows;
  horizontal zoom uses 32 of 512 columns and 128 rows; streaming has 32 of 512
  rows and 256 columns; downsampling uses 1024 columns x 512 rows. Complete
  preparation uses a 256x256 matrix with 10/90 percentile limits. Tiny is 2x2.
- Curves: 1,024 values in each orientation, middle-third zoom of 4,096 values,
  a flat 1,024-value curve, and two values. Refresh cases use 1,024 values.

Percentile batches use `(524288 / max(len, 1)).clamp(2, 2000)` operations;
heatmaps use eight (2,000 for tiny), curves use 64. Throughput counts input
values for percentiles, output pixels for heatmaps, and visible line segments
for curves. Complete heatmap gains combine changes 1 and 2 and are not additive.
Allocation bytes are allocator requests/churn, not RSS, live heap, or measured peak.

## Timing and throughput

Positive cycle savings indicate improvement. M units/s means millions of the
workload-specific units above per second. Empty input has zero useful units.

| Workload | Old ns/op | New ns/op | Old cycles/op | New cycles/op | Cycles saved | Old M units/s | New M units/s |
|---|---:|---:|---:|---:|---:|---:|---:|
| percentile_empty | 6.2 | 7.0 | 14.2 | 15.9 | -12.0% | 0.00 | 0.00 |
| percentile_tiny | 93.1 | 36.9 | 205.0 | 81.4 | 60.3% | 42.96 | 108.55 |
| percentile_random | 2,165,125.0 | 388,575.0 | 4,743,422.2 | 842,688.0 | 82.2% | 30.27 | 168.66 |
| percentile_sorted | 101,725.0 | 67,425.0 | 223,505.9 | 146,104.9 | 34.6% | 644.25 | 971.98 |
| percentile_descending | 120,587.5 | 69,100.0 | 264,497.5 | 151,894.0 | 42.6% | 543.47 | 948.42 |
| percentile_constant | 96,887.5 | 67,812.5 | 212,311.4 | 149,314.9 | 29.7% | 676.41 | 966.43 |
| percentile_repeated | 771,837.5 | 431,300.0 | 1,692,701.5 | 946,346.8 | 44.1% | 84.91 | 151.95 |
| percentile_large | 22,823,950.0 | 4,290,200.0 | 49,956,664.0 | 9,394,598.5 | 81.2% | 22.97 | 122.21 |
| heat_vertical_expand | 3,152,762.5 | 104,600.0 | 6,902,726.4 | 229,295.2 | 96.7% | 21.44 | 646.12 |
| heat_horizontal_zoom | 3,622,837.5 | 390,525.0 | 7,941,756.9 | 855,967.6 | 89.2% | 18.65 | 173.06 |
| heat_streaming | 774,987.5 | 123,225.0 | 1,698,929.9 | 270,177.1 | 84.1% | 87.21 | 548.46 |
| heat_downsample | 3,956,150.0 | 3,905,075.0 | 8,671,484.6 | 8,545,162.4 | 1.5% | 17.08 | 17.31 |
| heat_complete | 5,506,900.0 | 1,999,325.0 | 12,054,006.6 | 4,373,263.4 | 63.7% | 12.27 | 33.80 |
| heat_tiny | 266.6 | 268.4 | 586.3 | 586.1 | 0.0% | 15.01 | 14.90 |
| curve_vertical | 31,001.6 | 23,495.3 | 68,003.8 | 51,537.9 | 24.2% | 33.00 | 43.54 |
| curve_horizontal | 28,681.2 | 23,845.3 | 62,910.8 | 52,292.5 | 16.9% | 35.67 | 42.90 |
| curve_zoom | 47,959.4 | 35,182.8 | 105,229.7 | 77,174.8 | 26.7% | 28.44 | 38.77 |
| curve_flat | 28,829.7 | 25,096.9 | 63,222.9 | 54,751.5 | 13.4% | 35.48 | 40.76 |
| curve_tiny | 429.7 | 287.5 | 960.3 | 651.6 | 32.1% | 2.33 | 3.48 |
| refresh_warm | 31,231.2 | 22,043.8 | 68,014.1 | 48,125.4 | 29.2% | 32.76 | 46.41 |
| refresh_shared | 35,964.1 | 24,375.0 | 78,889.7 | 52,796.6 | 33.1% | 28.45 | 41.97 |

## Allocation churn

A/R/F means allocation/reallocation/free calls per operation. Freed bytes equal
requested bytes, all realloc counts are zero, and counters are stable across
all three rounds. Warm retained buffers exist before counting and are released
after counting ends.

| Workload | Old A/R/F | New A/R/F | Old requested/freed bytes | New requested/freed bytes |
|---|---:|---:|---:|---:|
| percentile_empty | 0/0/0 | 0/0/0 | 0 | 0 |
| percentile_tiny | 1/0/1 | 0/0/0 | 32 | 0 |
| percentile_random | 2/0/2 | 1/0/1 | 1,048,576 | 524,288 |
| percentile_sorted | 2/0/2 | 0/0/0 | 1,048,576 | 0 |
| percentile_descending | 2/0/2 | 0/0/0 | 1,048,576 | 0 |
| percentile_constant | 2/0/2 | 0/0/0 | 1,048,576 | 0 |
| percentile_repeated | 2/0/2 | 1/0/1 | 1,048,576 | 524,288 |
| percentile_large | 2/0/2 | 1/0/1 | 8,388,608 | 4,194,304 |
| heat_vertical_expand | 1/0/1 | 1/0/1 | 270,336 | 270,336 |
| heat_horizontal_zoom | 1/0/1 | 1/0/1 | 270,336 | 270,336 |
| heat_streaming | 1/0/1 | 1/0/1 | 270,336 | 270,336 |
| heat_downsample | 1/0/1 | 1/0/1 | 270,336 | 270,336 |
| heat_complete | 3/0/3 | 2/0/2 | 1,318,912 | 794,624 |
| heat_tiny | 1/0/1 | 1/0/1 | 16 | 16 |
| curve_vertical | 2/0/2 | 2/0/2 | 294,640 | 147,352 |
| curve_horizontal | 2/0/2 | 2/0/2 | 294,640 | 147,352 |
| curve_zoom | 2/0/2 | 2/0/2 | 392,848 | 196,456 |
| curve_flat | 2/0/2 | 2/0/2 | 294,640 | 147,352 |
| curve_tiny | 2/0/2 | 2/0/2 | 304 | 184 |
| refresh_warm | 2/0/2 | 0/0/0 | 294,640 | 0 |
| refresh_shared | 2/0/2 | 2/0/2 | 294,640 | 147,352 |

## Controls and limits

- `percentile_empty` used 12.0% more cycles (6.2 -> 7.0 ns/op).

Downsampling has no repeated samples to reuse. Tiny/empty controls expose dispatch
costs and timing noise. No universal speedup or reduction in peak memory is claimed.
Retaining curve capacity trades a longer-lived buffer for less allocation churn.

## Behavior and validation

Seven integration tests pass in debug and release. They compare percentile bit
patterns (reversed/equal ranks, 3/97 limits, arbitrary float bits, NaNs, infinities,
signed zero, and input immutability), complete image bytes (zoom, transpose, both
origins, partial streaming, odd/fractional dimensions), and every vertex's position
and color bits (edges and degenerate segments). Empty/invalid graphs, allocation
budgets, storage reuse, shrinking/regrowing views, and immutable shared snapshots
are covered. Mesh and ReusableMesh actors produce equal final renderer frames,
including draw operations, transforms, tint, and backend-visible vertices.

All 1,263 theme library tests pass (four existing tests ignored). Formatting,
diff whitespace checks, and workspace binary checks pass. Strict performance
Clippy encounters an existing large_enum_variant error in effects.rs:
SimplyLoveRuntimeRequest's JudgmentPalettes variant (424 bytes versus 176).
That unchanged enum and its palette type match the baseline. Rerunning with only
that lint allowed on the command line passes; no source lint allowance was added.
Other existing non-performance warnings remain.

Reproduction:

```text
cargo test -p deadsync-theme-simply-love --test sync_graph -- --test-threads=1
cargo test -p deadsync-theme-simply-love --release --test sync_graph -- --test-threads=1
cargo test -p deadsync-theme-simply-love --lib -- --test-threads=1
cargo clippy -p deadsync-theme-simply-love --lib --test sync_graph -- -D clippy::perf -A clippy::large_enum_variant
cargo check --workspace --bins
cargo test -p deadsync-theme-simply-love --release --test sync_graph sync_graph_bench -- --ignored --nocapture --test-threads=1
```

Run the benchmark three times, setting DEADSYNC_PERF_REVERSE=1 only for the middle
invocation. Committed tests reproduce the workloads; timing values are reported
measurements, not CI performance assertions. The four user-excluded files are
absent from this commit. Cargo.toml/Cargo.lock bump only 0.5.1146 to 0.5.1147.
