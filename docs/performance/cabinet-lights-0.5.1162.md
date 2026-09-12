# Cabinet light preparation performance - 0.5.1162

Baseline: `8bf5d81d5` (`0.5.1161`), measured 2026-09-12. This pass increments the workspace patch exactly once to `0.5.1162`, including the three inherited-version entries in Cargo.lock.

## Three changes

The local `rust-performance.md` guidance on profiling CPU and allocations (M-HOTPATH), batching work (M-THROUGHPUT), sufficient initial capacity (M-INITIAL-CAPACITY), and minimizing long-lived collection storage (M-SHRINK-TO-FIT) motivated these changes:

1. **Reuse row timing calculations.** `LightRowTimes` memoizes the previous row's result, including rejected rows. Chords share beat lookup, judgability and time conversion. For batches of at least 16 parsed notes, compatible timing uses the existing stack-only `BeatTimeCache` to resume timing searches. Subrow, duplicate, nonpositive or nonfinite BPM boundaries fall back to independent conversions via `supports_row_time_cache`; cursor rewinds are supported. No persistent cache or heap storage is added.
2. **Fuse generated lights from a shared source chart.** Marquee notes and bass row events are emitted in one traversal with one timing cursor. Already ordered event vectors skip sorting; others retain the unstable time sort. Different source charts retain separate traversals. Missing second-chart fallback preserves the original simplify-bass flag. Equal-time ordering was already unstable and does not affect the runtime blink bitmask; event multiplicity, times, rows, light identity and flags are preserved.
3. **Reserve storage for eligible events and bass rows.** Explicit charts count supported columns and non-fake notes. Generated charts count tap/hold/roll notes and two bass events per candidate row transition, instead of two per note. A shared chart needs one count scan. This reduces the song-lifetime buffer without a shrink/reallocation copy. The bound deliberately avoids timing queries, so warped/fake timing regions can leave unused capacity. Zero-capacity results return immediately.

These functions run through `cabinet_light_chart_from_loaded` in the shell's loaded-chart paths and `lighting.rs` chart loading. This targets chart preparation, not per-frame lighting or audio processing.

## Behavior and project checks

- 46 lighting unit tests pass in debug and release; one manual benchmark is ignored by normal runs.
- Six new behavior/allocation tests include comparisons against frozen old functions: 2,430 full-builder combinations cover empty/tiny/large chords, six versus unsupported columns, explicit/shared/separate plans, missing charts, stops/delays/warps/fakes, ambiguous BPM boundaries and finite/NaN/infinite offsets. Additional comparisons cover all note types, invalid rows/columns, repeated/reversed/shuffled rows, unusual/nonfinite beat-table values, rewind queries, saturating offsets and both bass simplification settings.
- Comparisons check the complete event multiset, output time ordering and the visible light mask at every event timestamp, including duplicate equal-time events. The fused emitter is also compared with separate passes using the same new timing cache.
- Allocation assertions verify no timing-query churn, one allocation with zero reallocations and exact retained capacity for ordinary generated chords, and no allocation for all-fake charts.
- `cargo test -p deadsync-lights --lib --locked`, the equivalent `--release` run, `cargo clippy -p deadsync-lights --all-targets --locked -- -D clippy::perf`, and root `cargo check --all-targets --locked` pass. Targeted rustfmt, diff, exact version/lockfile, frozen-baseline and benchmark-source audits pass. Existing non-performance style warnings are not treated as failures.

## Measurement method

Windows x86_64, Intel Xeon E5-2696 v4 @ 2.20 GHz; rustc 1.98.0 (88d9e12ae, LLVM 22.1.8). Release opt-level 3, full LTO. The old builder and helpers are frozen from the parent commit with only visibility/import/formatting adaptations. Old and new run in the same test executable. Builds and other Cargo activity finish before measurement.

Run `cargo test -p deadsync-lights --lib --release --locked cabinet_lights_bench -- --ignored --test-threads=1 --nocapture` three times, with `DEADSYNC_PERF_REVERSE=1` for the middle run. Each case has three warmups and seven timing batches; tables show the median of three per-run medians. Iteration counts are embedded in the benchmark (8-50,000 depending on workload). Chart/timing construction, input cloning, plan strings and correctness checks are outside measurement. Output allocation and destruction are included.

Timing disables allocation counting; the shared `tests/support/perf.rs` allocator records allocation/reallocation/free calls and requested/freed bytes separately for one operation. Windows `QueryThreadCycleTime` measures the calling thread's cycles, not retired instructions, system-wide CPU or hardware cache misses. Requested byte churn is not process RSS or peak live memory. Exact Vec capacities are separately tested. These are warmed synthetic preparation workloads, not end-to-end song load measurements or physical cabinet tests.

Fixtures contain note rows spaced 12 engine rows apart (quarter-beat spacing), with 1, 4, 6 or 10 notes per chord. Dense timing changes BPM every eight beats and adds stops, delays, warps and fake segments between changes. The largest fixture contains 4,096 note rows / 16,384 notes and 128 BPM points. Ambiguous fixtures add duplicate subrow BPMs. Reverse fixtures reverse parsed notes; ignored fixtures contain only fake notes. The separate-chart fixture includes a second chart with half as many rows and two-note chords.

- `timing_*` isolates old independent queries versus memoization/cursor reuse, with no allocation on either side.
- `fusion_*` isolates fusion/order handling after **both** sides already use the new timing cache. Both reserve the same old conservative maximum; these are controlled intermediate variants rather than a claim that the parent used the new cache.
- `storage_*` isolates reservation sizing with the same fused emitter on both sides; it exposes the CPU cost of the extra count scan as well as byte savings.
- `events_*` compares the complete frozen parent builder against the complete production replacement.

Throughput units are primary-chart parsed notes/s (including filtered notes); for the separate case this excludes the additional second-chart notes from the denominator. Empty input uses operations/s. Output events/s and physical cabinet throughput are not measured.

## CPU and elapsed time

Positive cycle savings mean improvement on this machine and workload.

| Scenario | Old us/op | New us/op | Old cycles/op | New cycles/op | Fewer cycles |
|---|---:|---:|---:|---:|---:|
| storage_single | 48.2860 | 51.1620 | 105,897.8 | 112,310.5 | -6.06% |
| storage_chords | 72.9760 | 76.3585 | 159,952.9 | 167,368.8 | -4.64% |
| storage_wide | 117.7390 | 132.5570 | 257,950.9 | 290,569.7 | -12.65% |
| storage_ignored | 3.1805 | 2.5275 | 6,963.6 | 5,554.4 | 20.24% |
| timing_tiny | 0.0434 | 0.0465 | 95.3 | 102.1 | -7.14% |
| timing_simple | 180.7760 | 53.2300 | 396,246.9 | 116,744.4 | 70.54% |
| timing_dense | 100,474.7875 | 1,236.7875 | 220,265,973.8 | 2,710,358.5 | 98.77% |
| timing_ambiguous | 6,597.5375 | 1,684.3375 | 14,464,501.5 | 3,691,715.5 | 74.48% |
| fusion_tiny | 0.1536 | 0.1047 | 336.9 | 229.6 | 31.85% |
| fusion_single | 153.5660 | 63.5060 | 335,720.8 | 139,081.8 | 58.57% |
| fusion_chords | 272.6640 | 87.4230 | 597,542.6 | 191,643.3 | 67.93% |
| fusion_dense | 3,227.1950 | 1,454.5150 | 7,071,774.2 | 3,187,886.4 | 54.92% |
| fusion_reverse | 3,446.7500 | 1,832.6800 | 7,547,573.4 | 4,017,244.9 | 46.77% |
| events_empty | 0.0128 | 0.0132 | 28.1 | 28.9 | -2.85% |
| events_tiny | 0.1545 | 0.1239 | 338.8 | 271.8 | 19.78% |
| events_single | 162.2000 | 58.5770 | 355,453.9 | 128,389.9 | 63.88% |
| events_chords | 398.4250 | 86.8920 | 873,129.3 | 190,490.9 | 78.18% |
| events_wide | 816.8125 | 153.5800 | 1,790,047.2 | 336,575.8 | 81.20% |
| events_dense | 128,199.3125 | 1,366.7625 | 281,052,382.8 | 2,995,406.6 | 98.93% |
| events_separate | 27,297.2875 | 901.4250 | 59,836,029.6 | 1,974,430.0 | 96.70% |
| events_explicit | 45,006.5625 | 253.4500 | 98,655,619.6 | 555,938.6 | 99.44% |
| events_ambiguous | 8,547.9750 | 1,701.2375 | 18,738,742.8 | 3,729,003.5 | 80.10% |
| events_reverse | 8,328.5750 | 1,758.6375 | 18,257,159.0 | 3,854,996.2 | 78.89% |
| events_ignored | 6.0752 | 2.5545 | 13,315.5 | 5,602.4 | 57.93% |

## Allocation churn and throughput

A/R/F means allocation/reallocation/free calls. Requested and freed bytes are cumulative per-operation totals, including returned-vector destruction.

| Scenario | Old A/R/F | New A/R/F | Old requested/freed B | New requested/freed B | Old units/s | New units/s |
|---|---:|---:|---:|---:|---:|---:|
| storage_single | 1/0/1 | 1/0/1 | 73,728/73,728 | 73,728/73,728 | 21,206,975.1 | 20,014,854.8 |
| storage_chords | 1/0/1 | 1/0/1 | 294,912/294,912 | 147,456/147,456 | 56,128,042.1 | 53,641,703.3 |
| storage_wide | 1/0/1 | 1/0/1 | 737,280/737,280 | 294,912/294,912 | 86,972,031.4 | 77,249,786.9 |
| storage_ignored | 1/0/1 | 0/0/0 | 294,912/294,912 | 0/0 | 1,287,847,822.7 | 1,620,573,689.4 |
| timing_tiny | 0/0/0 | 0/0/0 | 0/0 | 0/0 | 23,020,257.8 | 21,485,046.4 |
| timing_simple | 0/0/0 | 0/0/0 | 0/0 | 0/0 | 22,657,874.9 | 76,949,088.9 |
| timing_dense | 0/0/0 | 0/0/0 | 0/0 | 0/0 | 163,065.8 | 13,247,223.1 |
| timing_ambiguous | 0/0/0 | 0/0/0 | 0/0 | 0/0 | 620,837.7 | 2,431,816.7 |
| fusion_tiny | 1/0/1 | 1/0/1 | 72/72 | 72/72 | 6,509,145.3 | 9,553,835.9 |
| fusion_single | 1/0/1 | 1/0/1 | 73,728/73,728 | 73,728/73,728 | 6,668,142.7 | 16,124,460.7 |
| fusion_chords | 1/0/1 | 1/0/1 | 294,912/294,912 | 294,912/294,912 | 15,022,151.8 | 46,852,658.9 |
| fusion_dense | 1/0/1 | 1/0/1 | 1,179,648/1,179,648 | 1,179,648/1,179,648 | 5,076,854.7 | 11,264,235.8 |
| fusion_reverse | 1/0/1 | 1/0/1 | 294,912/294,912 | 294,912/294,912 | 1,188,365.9 | 2,234,978.3 |
| events_empty | 0/0/0 | 0/0/0 | 0/0 | 0/0 | 78,161,638.3 | 75,585,789.9 |
| events_tiny | 1/0/1 | 1/0/1 | 72/72 | 72/72 | 6,470,900.4 | 8,070,243.4 |
| events_single | 1/0/1 | 1/0/1 | 73,728/73,728 | 73,728/73,728 | 6,313,193.6 | 17,481,264.0 |
| events_chords | 1/0/1 | 1/0/1 | 294,912/294,912 | 147,456/147,456 | 10,280,479.4 | 47,138,977.1 |
| events_wide | 1/0/1 | 1/0/1 | 737,280/737,280 | 294,912/294,912 | 12,536,536.8 | 66,675,348.4 |
| events_dense | 1/0/1 | 1/0/1 | 1,179,648/1,179,648 | 589,824/589,824 | 127,801.0 | 11,987,452.1 |
| events_separate | 1/0/1 | 1/0/1 | 294,912/294,912 | 245,760/245,760 | 300,103.1 | 9,087,833.2 |
| events_explicit | 1/0/1 | 1/0/1 | 294,912/294,912 | 294,912/294,912 | 273,026.9 | 48,482,935.5 |
| events_ambiguous | 1/0/1 | 1/0/1 | 294,912/294,912 | 147,456/147,456 | 479,177.8 | 2,407,659.1 |
| events_reverse | 1/0/1 | 1/0/1 | 294,912/294,912 | 147,456/147,456 | 491,800.8 | 2,329,075.8 |
| events_ignored | 1/0/1 | 0/0/0 | 294,912/294,912 | 0/0 | 674,210,937.8 | 1,603,444,901.2 |

## Tradeoffs and limits

- `storage_single`: 6.06% more cycles; elapsed 48286.0 to 51162.0 ns/op. Single-note rows pay the count scan without saving output storage.
- `storage_chords`: 4.64% more cycles; elapsed 72976.0 to 76358.5 ns/op. The count scan trades construction CPU for less retained buffer storage.
- `storage_wide`: 12.65% more cycles; elapsed 117739.0 to 132557.0 ns/op. The count scan trades construction CPU for less retained buffer storage.
- `timing_tiny`: 7.14% more cycles; elapsed 43.4 to 46.5 ns/op. Setup/check overhead is visible in this small control.
- `events_empty`: 2.85% more cycles; elapsed 12.8 to 13.2 ns/op. Setup/check overhead is visible in this small control.

Per-round paired cycle savings for representative complete-builder cases:

- `events_tiny`: 5.24%, 16.88%, 21.19%.
- `events_chords`: 77.87%, 78.18%, 79.24%.
- `events_dense`: 98.93%, 98.94%, 98.97%.
- `events_separate`: 96.69%, 96.71%, 96.70%.
- `events_ignored`: 58.84%, 58.35%, 52.49%.

The complete builder retains an owned song-lifetime event vector: nonempty charts generally still allocate once, as before. Four-note shared chords reserve half as many bytes, ten-note chords reserve 60% fewer bytes, and all-fake charts allocate nothing. Single-note generated rows cannot save output storage. Timing-filtered regions can still overreserve, since counting them exactly would repeat timing work. A preliminary orderedness scan can add work before sorting distinct-source or reversed streams. The measured timing and fusion gains outweigh reservation counting in the tested complete nonempty workloads; this does not imply every possible chart improves, or that cache-unsupported timing gains the same speedup as compatible dense timing.

Raw runs, per-run medians, source hashes, commands and logs remain in ignored `target/cabinet-light-perf/`. Committed tests and frozen baselines reproduce the fixtures and comparisons. All four user-excluded files remain outside the commit.
