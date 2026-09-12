# Chart metadata performance — 0.5.1160

Baseline: `da5a127ac` (`0.5.1159`). Measured on 2026-09-12. This pass increments the workspace patch version exactly once to `0.5.1160`, including all three shared-version package entries in Cargo.lock.

## Changes and active callers

The `M-HOTPATH`, `M-INITIAL-CAPACITY`, and `M-MEM-REUSE` guidance in the local `rust-performance.md` motivated three changes in `crates/deadsync-simfile/src/cache.rs`:

1. **Count ordered scoring rows without scratch allocation.** `build_chart_totals` shares beat/judgability checks across notes in each contiguous row and counts scoring rows directly when the input is ordered. It preserves saturating hold/roll/mine totals, fake/warp filtering and row-based grade points. Unordered public/cache inputs retain a sorting fallback with the same allocation budget as before. This also removes redundant fake-segment lookups because `is_judgable_at_beat` already checks fake segments.
2. **Reuse the existing timing cursor for measure boundaries.** `build_measure_seconds` queries increasing four-beat boundaries with `BeatTimeCache` for lists of at least 16 measures. `supports_row_time_cache` guards duplicate, subrow and invalid BPM cases; these and shorter lists retain independent conversions. Nanosecond-to-f32 conversion remains bit-for-bit identical. The returned measure vector still allocates exactly once at its required capacity.
3. **Keep temporary metadata timing independent of the row table.** Song-bound and chart-metadata builders pass an empty row table to temporary `TimingData`; bound calculations already resolve their two beats, and totals borrow the original chart's row table through a private helper. They no longer clone a potentially multi-megabyte table only to discard it. Gameplay timing construction and its retained row lookup remain unchanged.

`parse_song_data_file[_in]` calls `update_precise_song_bounds`; `build_song_meta` calls the owned chart metadata builder. Cache writing calls `encode_song_cache_header → BorrowedCachedSongMeta::new → BorrowedCachedChartMeta::new → compute_cached_chart_meta`, which uses the same optimized totals and measure-time paths. Thus these improvements reach song parsing, metadata construction and cache writes. The cached metadata bytes and public APIs are unchanged.

## Behavior validation

- 204 unit tests passed in both debug and release; five manual benchmarks are ignored by the ordinary test run (one added in this pass).
- Six new behavior/allocation tests compare sorted, reversed, shuffled and duplicate notes, all six note types, out-of-range rows, fake/warp segments, hold tails, edits, lights charts, and absent/nonfinite row beats with the frozen old implementations.
- Measure-time tests compare every f32 bit over 59 timing variants, three offsets and seven list sizes, including 48 generated overlapping/negative stop-delay-warp configurations, invalid/duplicate/subrow BPMs and the 15/16-measure boundary.
- Cached chart metadata and song bounds compare serialized bincode bytes; owned `ChartData` compares all Debug fields. Ordered totals assert zero allocator churn. Bound calculation for both 4,096 and 1,048,576 rows stays below 32 allocation/free calls and 1,024 requested/freed bytes.
- `cargo clippy -p deadsync-simfile --all-targets --locked -- -D clippy::perf` and `cargo check --all-targets --locked` passed. Clippy still reports non-performance style warnings, including the new timing fixture's default-field assignment. Targeted rustfmt, `git diff --check`, exact version/lockfile audits and frozen-baseline/source-identity checks passed.

## Measurement method

Windows x86_64, Intel Xeon E5-2696 v4 @ 2.20 GHz; rustc 1.98.0 (88d9e12ae, LLVM 22.1.8). Release opt-level 3 with full LTO. Old and new implementations run in the same test executable. The six baseline function bodies are frozen from the parent commit, with only imports/visibility/formatting adapted.

Run `cargo test -p deadsync-simfile --lib --release --locked chart_metadata_bench -- --ignored --test-threads=1 --nocapture`. Repeat three times, setting `DEADSYNC_PERF_REVERSE=1` for the middle run. Each scenario has three warmups and seven timing batches. Tables report the median of the three per-run medians. The common helper in `tests/support/perf.rs` separately samples allocator calls for one operation; timing runs disable allocation counting. Inputs, timing construction for isolated totals/measures, and fixture cloning occur outside the measured operation. Output creation and destruction are included. Bounds mutate reusable fixtures and return their scalar bounds; metadata returns and drops a fresh complete `CachedChartMeta`.

Cycles are calling-thread `QueryThreadCycleTime` readings, not retired-instruction hardware counters or whole-process CPU accounting. Throughput is notes/s, measures/s or charts/s for the indicated operation. Empty controls use one operation as one unit. Allocated bytes are total requested bytes, not peak live memory or process RSS; freed bytes are tracked separately. No claim is made about end-to-end song scanning, disk throughput, cache misses or application RSS.

Fixtures use 48 rows/beat, one note row every 12 rows, and one or four notes per row. The 65,536-row chord chart contains 21,848 notes and 341 measures. Dense timing introduces a BPM, stop, delay, warp and fake segment every eight beats. `fallback` also adds duplicate subrow BPMs. Measure fixtures have half as many timing-event groups as measures. Bounds retain a full row table, including a 1,048,576-row case; all generated tails are within the table, so each nonempty bounds fixture performs the actual timing construction. An explicit separate invalid-tail test covers the skipped path.

## CPU and elapsed time

Positive cycle savings mean improvement. These are synthetic workload measurements on this machine.

| Scenario | Old µs/op | New µs/op | Old cycles/op | New cycles/op | Fewer cycles |
|---|---:|---:|---:|---:|---:|
| totals_empty | 0.0332 | 0.0181 | 72.8 | 39.6 | 45.60% |
| totals_small | 0.1565 | 0.1106 | 343.3 | 242.4 | 29.39% |
| totals_single | 8.2489 | 6.9208 | 18,098.6 | 15,152.1 | 16.28% |
| totals_chords | 178.5818 | 79.1500 | 391,518.2 | 173,574.6 | 55.67% |
| totals_dense | 7,377.9727 | 1,891.8591 | 16,168,320.4 | 4,146,175.3 | 74.36% |
| totals_unordered | 8,387.9273 | 6,653.3091 | 18,362,242.4 | 14,558,756.5 | 20.71% |
| measures_empty | 0.0040 | 0.0140 | 10.0 | 31.7 | -217.00% |
| measures_small | 0.3234 | 0.3263 | 710.5 | 714.9 | -0.62% |
| measures_constant16 | 0.6086 | 0.5337 | 1,335.0 | 1,170.7 | 12.31% |
| measures_constant128 | 4.4488 | 3.6754 | 9,728.3 | 8,057.7 | 17.17% |
| measures_constant1024 | 35.0423 | 29.2495 | 76,810.2 | 63,683.1 | 17.09% |
| measures_dense64 | 117.9688 | 9.4625 | 258,831.7 | 20,866.2 | 91.94% |
| measures_dense512 | 7,379.6875 | 81.7375 | 16,169,467.6 | 179,509.8 | 98.89% |
| measures_dense2048 | 128,467.0562 | 321.5250 | 281,009,443.8 | 691,850.3 | 99.75% |
| measures_fallback512 | 7,581.1938 | 7,393.8875 | 16,619,085.4 | 16,189,524.6 | 2.58% |
| bounds_small | 1.2797 | 1.1047 | 2,826.1 | 2,445.4 | 13.47% |
| bounds_long | 148.9828 | 22.7641 | 323,807.1 | 49,867.7 | 84.60% |
| bounds_million | 1,616.9875 | 370.9188 | 3,533,655.1 | 811,851.7 | 77.03% |
| bounds_dense | 152.9391 | 100.3281 | 334,909.0 | 219,788.1 | 34.37% |
| metadata_small | 2.3625 | 1.7781 | 5,226.8 | 3,944.2 | 24.54% |
| metadata_long | 261.2094 | 99.8562 | 572,600.0 | 218,903.2 | 61.77% |
| metadata_dense | 10,878.3594 | 1,737.5812 | 23,820,990.6 | 3,794,146.7 | 84.07% |
| metadata_fallback | 10,734.6938 | 5,316.4031 | 23,488,235.4 | 11,633,932.2 | 50.47% |

## Allocation churn and throughput

All cases made zero reallocation calls. Allocations equaled frees and requested bytes equaled freed bytes in every sampled operation, including output destruction. Below, A/F lists allocation and free counts; B lists both requested and freed bytes. No counter reduction is implied for the cursor-only optimization.

| Scenario | Old A/F | New A/F | Old B | New B | Old units/s | New units/s | Unit |
|---|---:|---:|---:|---:|---:|---:|---|
| totals_empty | 0/0 | 0/0 | 0 | 0 | 30,144,088.7 | 55,377,118.2 | operations/s |
| totals_small | 1/1 | 0/0 | 128 | 0 | 102,230,673.3 | 144,629,892.1 | notes/s |
| totals_single | 1/1 | 0/0 | 10,928 | 0 | 165,597,694.7 | 197,377,023.3 | notes/s |
| totals_chords | 1/1 | 0/0 | 174,784 | 0 | 122,341,681.9 | 276,032,849.0 | notes/s |
| totals_dense | 1/1 | 0/0 | 174,784 | 0 | 2,961,247.1 | 11,548,428.8 | notes/s |
| totals_unordered | 1/1 | 1/1 | 174,784 | 174,784 | 2,604,695.9 | 3,283,779.5 | notes/s |
| measures_empty | 0/0 | 0/0 | 0 | 0 | 252,839,506.2 | 71,358,885.0 | operations/s |
| measures_small | 1/1 | 1/1 | 32 | 32 | 24,738,034.1 | 24,515,936.0 | measures/s |
| measures_constant16 | 1/1 | 1/1 | 64 | 64 | 26,290,115.5 | 29,979,871.9 | measures/s |
| measures_constant128 | 1/1 | 1/1 | 512 | 512 | 28,771,937.5 | 34,825,767.5 | measures/s |
| measures_constant1024 | 1/1 | 1/1 | 4,096 | 4,096 | 29,221,838.6 | 35,009,189.9 | measures/s |
| measures_dense64 | 1/1 | 1/1 | 256 | 256 | 542,516.6 | 6,763,540.3 | measures/s |
| measures_dense512 | 1/1 | 1/1 | 2,048 | 2,048 | 69,379.6 | 6,263,954.7 | measures/s |
| measures_dense2048 | 1/1 | 1/1 | 8,192 | 8,192 | 15,941.8 | 6,369,644.7 | measures/s |
| measures_fallback512 | 1/1 | 1/1 | 2,048 | 2,048 | 67,535.5 | 69,246.4 | measures/s |
| bounds_small | 14/14 | 13/13 | 1,000 | 232 | 781,440.8 | 905,233.4 | charts/s |
| bounds_long | 14/14 | 13/13 | 262,376 | 232 | 6,712.2 | 43,928.9 | charts/s |
| bounds_million | 14/14 | 13/13 | 4,194,536 | 232 | 618.4 | 2,696.0 | charts/s |
| bounds_dense | 18/18 | 17/17 | 277,248 | 15,104 | 6,538.6 | 9,967.3 | charts/s |
| metadata_small | 19/19 | 17/17 | 1,545 | 265 | 423,280.4 | 562,390.2 | charts/s |
| metadata_long | 19/19 | 17/17 | 441,273 | 4,345 | 3,828.3 | 10,014.4 | charts/s |
| metadata_dense | 23/23 | 21/21 | 456,145 | 19,217 | 91.9 | 575.5 | charts/s |
| metadata_fallback | 24/24 | 22/22 | 457,569 | 20,641 | 93.2 | 188.1 | charts/s |

## Tradeoffs and limits

- `measures_empty` increased thread cycles by 217.00% and elapsed time by 10.0 ns/op (4.0 → 14.0 ns). This empty-call control does no measure conversion.
- `measures_small` increased thread cycles by 0.62% and elapsed time by 2.9 ns/op (323.4 → 326.3 ns). This small difference on the direct compatibility path may be measurement noise; there is no allocation increase.

Per-round cycle savings for the larger representative cases (paired within each run) were:

- `totals_dense`: 75.03%, 77.85%, 74.36%.
- `measures_dense512`: 98.89%, 99.04%, 99.10%.
- `bounds_long`: 86.83%, 83.75%, 48.59%.
- `metadata_dense`: 86.13%, 84.02%, 84.07%.

Unordered totals still need a scratch row vector; zero allocation is guaranteed only for ordered input. The order check adds a linear pass, which the measured valid chart workloads recover through fewer timing checks, no scratch writes and no sort. Small measure lists and unsupported BPM layouts retain direct timing queries. Temporary `TimingData` still allocates timing segments and control structures; the third change removes its row-table copy rather than making all metadata allocation-free. Fixture construction, parsing, decoding and cache file I/O are outside the isolated measurements.

Raw runs, medians, source hashes, commands and logs are retained locally under `target/chart-metadata-perf/` (ignored). The committed benchmark contains all fixtures and frozen old implementations needed to reproduce the comparison.
