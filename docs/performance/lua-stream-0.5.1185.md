# Lua numeric indexing, line geometry and method calls — 0.5.1185

Date: 2026-09-13. Parent: `56a0f78eb48cb5af19d2b33667585fcdbcb79ffb`
(0.5.1184). This pass increments the workspace patch version once to 0.5.1185
and updates the three workspace-version entries in Cargo.lock.

## Representative results

- **1,024 distinct integers:** 25.23% fewer CPU cycles; allocation calls 15 → 9; requested bytes 107,976 → 71,592.
- **1,024-point zigzag line strip:** 42.11% fewer CPU cycles; allocation calls 2 → 1; requested bytes 204,608 → 196,416.
- **BPM fallback query with prebuilt callback results:** 34.34% fewer CPU cycles; allocation calls 4 → 0; requested bytes 496 → 0.

See the complete tables and control discussion below; these figures are not universal speedups.

## Changes

1. **Build the rounded integer index only when floats require it.** The
   deduplication index previously stored every integer twice: as an exact
   integer and as its rounded floating-point representation. Integer-only
   input now builds just the exact set. When the first non-NaN float arrives,
   the rounded set is built once from all accepted integers. A prefix already
   containing floats keeps the existing pre-sized mixed-number path.
   This removes a hash insertion and a set's allocation/growth for integer-only
   input. Standard randomized hashers and the existing indexing thresholds
   remain in place.

2. **Stream line-strip geometry and reuse segment calculations.** Adjacent
   segments share normals and join offsets. The old implementation built a
   temporary offset vector, then walked the vertices again, recalculating
   segment lengths. The new loop computes each segment's length and unit normal
   once, carries the adjacent data forward, and emits triangles directly.
   It allocates only the output vector. Endpoint scaling, miter clamping,
   degenerate-segment handling, triangle order, colors and UVs retain the
   previous behavior.

3. **Pass the receiver directly to Lua method calls.** BPM and author helpers
   previously created a `MultiValue` allocation to carry one table argument.
   Both call helpers now pass `&Table` directly through mlua's argument
   conversion. This removes the argument-vector allocation and avoids cloning
   a table handle solely to call a method. Method lookup, receiver identity,
   argument count, callback order, return conversion and errors remain intact.

The local `rust-performance.md` guide motivated removing repeated calculations
and short-lived allocations (M-HOTPATH), reusing already available data
(M-MEM-REUSE), and reserving storage when its size is known (M-INITIAL-CAPACITY).
No new dependency or unsafe code is introduced. These are measured helper
workloads; no whole-song compile, frame-rate or production-profile claim is made.

## Behavior and allocation checks

The new tests compare production code with frozen implementations from the
parent commit. Nine named function bodies plus the complete previous numeric
index and its finishing loop were checked against that commit. Shared helpers
outside the changed functions remain unchanged; an audit reconstructs both
parent production files from the final diff.

Thirteen new regression tests cover:

- The first float before, at and after index transitions; signed zero;
  extrema; non-transitive integer/float comparisons above 2^53; all six orders
  of integer/float aliases; a rejected first float followed by new integers;
  NaNs; infinities; invalid UTF-8; opaque values; nil sequence boundaries; and
  deterministic mixed-value sequences.
- Bit-for-bit geometry results over straight lines, corners, reversals,
  clamped miters, repeated points, degenerate segments, signed zero, very small
  and large values, and seeded finite coordinates. Nonfinite coordinates and
  widths preserve both output bits and existing panic behavior.
- Lua receiver identity and argument count; first-return/nil behavior;
  callback mutation and errors; text coercion and UTF-8 errors; metatable
  lookup; author deduplication; and BPM fallback lookup order.
- A 512-integer index needs at most one allocation, no reallocations and
  10,000 requested bytes. Line-strip generation needs only its output
  allocation. A warmed scalar Lua method call and disabled line generation
  have zero allocation, reallocation, free or byte churn.

Validation:

- Fresh parent debug library suite: **553 passed, 5 failed, 45 ignored**.
- Final debug and release library suites: **566 passed, 5 failed, 48 ignored**.
- All 13 new tests pass in both profiles. Three new manual benchmarks are
  ignored in ordinary test runs. All 39 paired fixtures also passed a debug
  smoke run before release benchmarking.
- `cargo check --all-targets --offline`: passed for the root application.
- `cargo clippy -p deadsync-song-lua --all-targets --locked -- -D clippy::perf`:
  passed. Existing non-performance warnings remain.
- Rustfmt checks on all eight changed Rust files, the baseline/source/version
  audit, and Git whitespace checks: passed.

The five failures have the same assertions as the freshly tested parent:

- `compile_song_lua_extracts_actorproxy_targets`: visibility assertion.
- `compile_song_lua_layers_share_init_globals_and_actor_refs`: 0 versus 123.
- `compile_song_lua_resolves_local_and_hidden_screen_proxy_targets`: visibility assertion.
- `compile_song_lua_runs_cmd_queuecommand_builders`: visibility assertion.
- `compile_song_lua_supports_notefield_column_api`: `-96:-135` versus `-96:-125`.

There was no separate parent release build. The frozen parent implementations
were compiled and exercised alongside the new code in the final release binary.
The full suite is not green; these five failures are unchanged by this pass.

## Benchmark method

- Five serial release runs of all **39 old/new pairs**, each using seven timing
  samples after three warmups. Runs 2 and 4 reverse old/new order. The tables
  report medians across runs, plus all five per-run cycle comparisons.
- The existing `tests/support/perf.rs` harness measures elapsed time and Windows
  `QueryThreadCycleTime` for the calling thread. Allocation counting is a
  separate warmed operation through the scoped global allocator. It records
  allocation/reallocation/free calls and requested/freed bytes, including
  reallocation sizes. Timing samples do not enable allocation counting.
- Fixtures, input vectors, Lua tables and callback creation are outside the
  measured operation. Output creation and Rust output destruction are inside.
  Lua garbage collection runs before each pair and is then stopped. Deferred
  Lua collection and VM teardown are excluded. Byte totals are allocator
  requests, not peak live memory, retained RSS or hardware memory traffic.
- Numeric cases use 32 iterations per sample; geometry 256; method calls 512.
  Throughput units are input values for deduplication, input segments for
  geometry and complete helper invocations for method calls. Geometry counts
  input segments even for degenerate input or disabled output; empty inputs
  use one nominal unit. These controls do not measure emitted-segment throughput.
- No Rust compiler or linker process was running when a benchmark run started.
  The benchmark binary and relevant source files were hashed before the runs
  and checked afterwards.

The geometry output capacity is unchanged, including its upper-bound allocation
for degenerate input. Mixed-number input still needs two numeric representations;
the lazy projection is created even if the first float is subsequently rejected.
The complete author helper still allocates strings and its output table.
The BPM fixture callbacks return prebuilt tables; callbacks that construct new
tables can still allocate even though argument dispatch itself has no churn.
Zero churn applies only to the explicitly tested paths above.

## Reproduction

```powershell
cargo test -p deadsync-song-lua --lib lua_stream_pass_ --locked
cargo test -p deadsync-song-lua --lib --release --locked
cargo test -p deadsync-song-lua --lib --release lua_stream_pass_bench --locked -- --ignored --test-threads=1 --nocapture
$env:DEADSYNC_PERF_REVERSE = '1'
cargo test -p deadsync-song-lua --lib --release lua_stream_pass_bench --locked -- --ignored --test-threads=1 --nocapture
Remove-Item Env:DEADSYNC_PERF_REVERSE
```

The full library test command reports the five known failures listed above.
The filtered regression and manual benchmark commands pass. For repeated runs,
the measurements below invoke the completed release test executable directly.

## Environment

- CPU: Intel(R) Xeon(R) CPU E5-2696 v4 @ 2.20GHz.
- Windows x86_64, Rust release profile (`opt-level = 3`, full LTO).
- `rustc 1.98.0 (88d9e12ae 2026-08-18)`, LLVM 22.1.8.
- Final release test binary SHA-256: `c1bae1fccde8442c3259ecee6b9a1661217d3c81f15ae06c58bf7305f69ea5c1`.

## Paired release results

Medians across five runs. Positive cycle savings indicate an improvement for this workload on this machine. All allocation counts and byte totals were identical across the five repetitions.

| Scenario | Old µs/op | New µs/op | Old cycles/op | New cycles/op | Fewer cycles |
|---|---:|---:|---:|---:|---:|
| line_segments_straight_0_4 | 0.0145 | 0.0141 | 36.9 | 36.0 | 2.44% |
| line_segments_straight_1_4 | 0.0137 | 0.0137 | 34.3 | 34.3 | 0.00% |
| line_segments_straight_2_4 | 0.2566 | 0.0820 | 568.5 | 185.2 | 67.42% |
| line_segments_straight_8_4 | 0.5121 | 0.2621 | 1,128.4 | 580.5 | 48.56% |
| line_segments_straight_128_4 | 22.7883 | 6.9172 | 49,913.1 | 15,171.2 | 69.60% |
| line_segments_straight_1024_4 | 64.0996 | 41.2500 | 140,468.0 | 90,403.1 | 35.64% |
| line_segments_zigzag_8_4 | 0.3934 | 0.2582 | 869.4 | 571.9 | 34.22% |
| line_segments_zigzag_128_4 | 6.5098 | 4.6039 | 14,248.6 | 10,113.3 | 29.02% |
| line_segments_zigzag_1024_4 | 57.4844 | 33.5148 | 126,011.0 | 72,952.9 | 42.11% |
| line_segments_zigzag_4096_4 | 230.8633 | 137.0590 | 506,009.2 | 300,407.2 | 40.63% |
| line_segments_repeated_128_4 | 4.4047 | 3.1133 | 9,674.3 | 6,822.5 | 29.48% |
| line_segments_degenerate_128_4 | 2.5492 | 1.6332 | 5,599.8 | 3,590.0 | 35.89% |
| line_segments_zigzag_128_0 | 0.0129 | 0.0129 | 33.4 | 33.4 | 0.00% |
| lazy_numbers_integer_0_1 | 0.1469 | 0.1344 | 356.7 | 336.1 | 5.78% |
| lazy_numbers_integer_8_8 | 1.8844 | 1.9094 | 4,177.3 | 4,232.2 | -1.31% |
| lazy_numbers_integer_32_32 | 6.3625 | 6.2438 | 14,000.0 | 13,746.2 | 1.81% |
| lazy_numbers_integer_127_127 | 43.2812 | 43.3969 | 94,906.3 | 95,146.4 | -0.25% |
| lazy_numbers_integer_128_128 | 27.7594 | 21.6812 | 60,911.2 | 47,569.8 | 21.90% |
| lazy_numbers_integer_256_256 | 52.6719 | 39.9281 | 115,498.2 | 87,539.3 | 24.21% |
| lazy_numbers_integer_1024_1024 | 204.9750 | 153.5156 | 449,275.3 | 335,944.8 | 25.23% |
| lazy_numbers_integer_4096_4096 | 813.8344 | 601.7031 | 1,783,341.5 | 1,317,946.5 | 26.10% |
| lazy_numbers_integer_1024_8 | 60.2094 | 59.6031 | 132,084.1 | 130,732.8 | 1.02% |
| lazy_numbers_integer_1024_32 | 105.8938 | 108.2719 | 232,354.5 | 237,464.7 | -2.20% |
| lazy_numbers_integer_1024_33 | 66.7312 | 67.0688 | 146,379.1 | 147,119.9 | -0.51% |
| lazy_numbers_integer_1024_64 | 69.8875 | 69.3906 | 153,313.9 | 152,223.2 | 0.71% |
| lazy_numbers_number_1024_1024 | 148.1969 | 149.6781 | 324,812.0 | 328,097.6 | -1.01% |
| lazy_numbers_mixed_1024_1024 | 198.7625 | 201.0906 | 435,227.4 | 440,989.2 | -1.32% |
| lazy_numbers_late_float_1024_1024 | 207.0438 | 175.3531 | 453,768.2 | 384,097.5 | 15.35% |
| lazy_numbers_rejected_float_1024_1024 | 211.0156 | 178.0375 | 462,726.6 | 390,332.7 | 15.65% |
| lazy_numbers_string_128_128 | 84.0562 | 84.0281 | 184,380.0 | 184,338.9 | 0.02% |
| lazy_numbers_table_512_512 | 99.3938 | 99.7094 | 217,853.8 | 218,779.8 | -0.43% |
| table_calls_scalar | 0.2203 | 0.0840 | 486.2 | 186.5 | 61.64% |
| table_calls_string_short | 0.4883 | 0.4076 | 1,073.9 | 896.9 | 16.48% |
| table_calls_string_long | 1.3547 | 1.1611 | 2,976.1 | 2,537.5 | 14.74% |
| table_calls_string_missing | 0.0621 | 0.0627 | 139.3 | 140.2 | -0.65% |
| table_calls_bpm_direct | 0.4410 | 0.3424 | 970.6 | 754.1 | 22.31% |
| table_calls_bpm_fallback | 1.2262 | 0.8029 | 2,687.6 | 1,764.6 | 34.34% |
| table_calls_bpm_missing | 0.1344 | 0.1332 | 297.1 | 295.0 | 0.71% |
| table_calls_authors | 1.9258 | 1.6008 | 4,220.7 | 3,516.7 | 16.68% |

A/R/F means allocation/reallocation/free calls. Requested/freed bytes include reallocations. Lua garbage collection is excluded from timing; see methodology.

| Scenario | Old A/R/F | New A/R/F | Old requested/freed B | New requested/freed B | Old units/s | New units/s |
|---|---:|---:|---:|---:|---:|---:|
| line_segments_straight_0_4 | 0/0/0 | 0/0/0 | 0/0 | 0/0 | 69,189,189.2 | 71,111,111.1 |
| line_segments_straight_1_4 | 0/0/0 | 0/0/0 | 0/0 | 0/0 | 73,142,857.1 | 73,142,857.1 |
| line_segments_straight_2_4 | 2/0/2 | 1/0/1 | 208/208 | 192/192 | 3,896,499.2 | 12,190,476.2 |
| line_segments_straight_8_4 | 2/0/2 | 1/0/1 | 1,408/1,408 | 1,344/1,344 | 13,668,955.0 | 26,706,408.3 |
| line_segments_straight_128_4 | 2/0/2 | 1/0/1 | 25,408/25,408 | 24,384/24,384 | 5,573,039.9 | 18,360,063.2 |
| line_segments_straight_1024_4 | 2/0/2 | 1/0/1 | 204,608/204,608 | 196,416/196,416 | 15,959,535.6 | 24,800,000.0 |
| line_segments_zigzag_8_4 | 2/0/2 | 1/0/1 | 1,408/1,408 | 1,344/1,344 | 17,795,432.0 | 27,110,438.7 |
| line_segments_zigzag_128_4 | 2/0/2 | 1/0/1 | 25,408/25,408 | 24,384/24,384 | 19,509,150.9 | 27,585,270.7 |
| line_segments_zigzag_1024_4 | 2/0/2 | 1/0/1 | 204,608/204,608 | 196,416/196,416 | 17,796,140.3 | 30,523,788.4 |
| line_segments_zigzag_4096_4 | 2/0/2 | 1/0/1 | 819,008/819,008 | 786,240/786,240 | 17,737,770.9 | 29,877,647.3 |
| line_segments_repeated_128_4 | 2/0/2 | 1/0/1 | 25,408/25,408 | 24,384/24,384 | 28,832,919.5 | 40,792,973.7 |
| line_segments_degenerate_128_4 | 2/0/2 | 1/0/1 | 25,408/25,408 | 24,384/24,384 | 49,819,184.8 | 77,761,301.1 |
| line_segments_zigzag_128_0 | 0/0/0 | 0/0/0 | 0/0 | 0/0 | 9,852,121,212.1 | 9,852,121,212.1 |
| lazy_numbers_integer_0_1 | 1/0/0 | 1/0/0 | 56/0 | 56/0 | 6,808,510.6 | 7,441,860.5 |
| lazy_numbers_integer_8_8 | 3/4/1 | 3/4/1 | 776/592 | 776/592 | 4,245,439.5 | 4,189,852.7 |
| lazy_numbers_integer_32_32 | 3/8/1 | 3/8/1 | 3,464/2,896 | 3,464/2,896 | 5,029,469.5 | 5,125,125.1 |
| lazy_numbers_integer_127_127 | 3/12/1 | 3/12/1 | 14,216/12,112 | 14,216/12,112 | 2,934,296.0 | 2,926,478.0 |
| lazy_numbers_integer_128_128 | 9/10/7 | 6/10/4 | 14,696/12,592 | 10,616/8,512 | 4,611,054.8 | 5,903,718.7 |
| lazy_numbers_integer_256_256 | 11/11/9 | 7/11/5 | 28,040/23,888 | 19,336/15,184 | 4,860,278.8 | 6,411,520.7 |
| lazy_numbers_integer_1024_1024 | 15/13/13 | 9/13/7 | 107,976/91,536 | 71,592/55,152 | 4,995,731.2 | 6,670,330.8 |
| lazy_numbers_integer_4096_4096 | 19/15/17 | 11/15/9 | 427,528/361,936 | 280,520/214,928 | 5,032,965.1 | 6,807,343.7 |
| lazy_numbers_integer_1024_8 | 3/4/1 | 3/4/1 | 776/592 | 776/592 | 17,007,318.2 | 17,180,307.2 |
| lazy_numbers_integer_1024_32 | 3/8/1 | 3/8/1 | 3,464/2,896 | 3,464/2,896 | 9,670,070.2 | 9,457,673.1 |
| lazy_numbers_integer_1024_33 | 5/9/3 | 4/9/2 | 5,672/4,592 | 5,080/4,000 | 15,345,134.4 | 15,267,915.4 |
| lazy_numbers_integer_1024_64 | 7/9/5 | 5/9/3 | 8,008/6,928 | 6,248/5,168 | 14,652,119.5 | 14,757,036.7 |
| lazy_numbers_number_1024_1024 | 9/13/7 | 9/13/7 | 71,592/55,152 | 71,592/55,152 | 6,909,727.3 | 6,841,347.1 |
| lazy_numbers_mixed_1024_1024 | 21/13/19 | 21/13/19 | 89,928/73,488 | 89,928/73,488 | 5,151,877.2 | 5,092,231.4 |
| lazy_numbers_late_float_1024_1024 | 16/13/14 | 11/13/9 | 108,028/91,588 | 90,092/73,652 | 4,945,814.6 | 5,839,645.0 |
| lazy_numbers_rejected_float_1024_1024 | 15/13/13 | 10/13/8 | 107,976/91,536 | 90,040/73,600 | 4,852,721.2 | 5,751,597.3 |
| lazy_numbers_string_128_128 | 134/10/132 | 134/10/132 | 19,832/17,728 | 19,832/17,728 | 1,522,789.8 | 1,523,299.5 |
| lazy_numbers_table_512_512 | 40/12/38 | 40/12/38 | 37,272/29,024 | 37,272/29,024 | 5,151,229.3 | 5,134,923.4 |
| table_calls_scalar | 1/0/1 | 0/0/0 | 160/160 | 0/0 | 4,539,007.1 | 11,906,976.7 |
| table_calls_string_short | 3/0/3 | 2/0/2 | 184/184 | 24/24 | 2,048,000.0 | 2,453,282.2 |
| table_calls_string_long | 3/0/3 | 2/0/2 | 1,072/1,072 | 912/912 | 738,177.6 | 861,227.9 |
| table_calls_string_missing | 0/0/0 | 0/0/0 | 0/0 | 0/0 | 16,100,628.9 | 15,950,155.8 |
| table_calls_bpm_direct | 1/0/1 | 0/0/0 | 160/160 | 0/0 | 2,267,493.4 | 2,920,707.4 |
| table_calls_bpm_fallback | 4/0/4 | 0/0/0 | 496/496 | 0/0 | 815,546.4 | 1,245,439.1 |
| table_calls_bpm_missing | 0/0/0 | 0/0/0 | 0/0 | 0/0 | 7,441,860.5 | 7,507,331.4 |
| table_calls_authors | 12/0/10 | 9/0/7 | 720/648 | 240/168 | 519,269.8 | 624,695.0 |

## Repeatability and controls

Per-run paired cycle savings; positive is faster. These ratios use each run’s own old/new measurements.

| Scenario | Run 1 | Run 2 | Run 3 | Run 4 | Run 5 |
|---|---:|---:|---:|---:|---:|
| line_segments_straight_0_4 | 6.74% | -7.78% | 7.05% | -2.50% | 7.05% |
| line_segments_straight_1_4 | 2.56% | 0.00% | 2.62% | -2.62% | 0.00% |
| line_segments_straight_2_4 | 68.47% | -30.08% | 69.91% | -33.80% | 67.58% |
| line_segments_straight_8_4 | 52.55% | -4.21% | 52.13% | 6.64% | 52.95% |
| line_segments_straight_128_4 | 70.08% | 1.95% | 71.93% | 83.56% | 60.11% |
| line_segments_straight_1024_4 | 45.10% | 27.77% | 42.50% | 28.83% | 38.22% |
| line_segments_zigzag_8_4 | 34.32% | 33.96% | 34.03% | 33.43% | 29.41% |
| line_segments_zigzag_128_4 | 30.10% | 23.37% | 36.60% | 35.38% | 29.55% |
| line_segments_zigzag_1024_4 | 40.56% | 51.42% | 40.34% | 43.29% | 36.73% |
| line_segments_zigzag_4096_4 | 40.76% | 40.40% | 40.31% | 39.91% | 36.32% |
| line_segments_repeated_128_4 | 34.73% | 22.16% | 28.44% | 29.77% | 26.09% |
| line_segments_degenerate_128_4 | 35.89% | 36.67% | 36.56% | 35.85% | 44.42% |
| line_segments_zigzag_128_0 | -28.83% | -2.45% | 4.96% | 2.40% | -2.50% |
| lazy_numbers_integer_0_1 | 9.62% | -41.33% | 10.92% | -8.88% | 5.78% |
| lazy_numbers_integer_8_8 | -7.85% | 2.39% | -0.49% | -7.12% | -28.25% |
| lazy_numbers_integer_32_32 | -3.09% | 2.50% | -1.16% | 7.97% | 8.19% |
| lazy_numbers_integer_127_127 | -4.57% | 0.26% | -0.44% | 1.65% | -3.44% |
| lazy_numbers_integer_128_128 | 20.78% | 22.88% | 23.59% | 3.15% | 17.28% |
| lazy_numbers_integer_256_256 | 21.90% | 22.42% | 22.74% | 24.96% | 28.72% |
| lazy_numbers_integer_1024_1024 | 20.85% | 28.12% | 27.25% | 23.95% | 26.51% |
| lazy_numbers_integer_4096_4096 | 26.20% | 28.27% | 27.52% | 25.29% | 27.43% |
| lazy_numbers_integer_1024_8 | -1.95% | 1.17% | 1.00% | 2.07% | 1.02% |
| lazy_numbers_integer_1024_32 | -2.54% | -0.26% | -0.27% | -16.98% | -3.22% |
| lazy_numbers_integer_1024_33 | -0.51% | -2.03% | -2.77% | -1.95% | 0.14% |
| lazy_numbers_integer_1024_64 | -1.31% | 0.48% | 2.45% | 2.57% | 0.71% |
| lazy_numbers_number_1024_1024 | 1.79% | -0.58% | -12.63% | -4.42% | 6.29% |
| lazy_numbers_mixed_1024_1024 | 1.25% | -1.38% | -1.32% | 0.55% | -3.36% |
| lazy_numbers_late_float_1024_1024 | 2.82% | 16.72% | 14.22% | 16.47% | 12.47% |
| lazy_numbers_rejected_float_1024_1024 | 30.22% | 19.85% | 12.23% | 15.60% | 11.79% |
| lazy_numbers_string_128_128 | 11.26% | -1.25% | -0.22% | 27.85% | -0.97% |
| lazy_numbers_table_512_512 | 4.38% | 0.18% | -0.02% | -1.33% | -2.07% |
| table_calls_scalar | 47.72% | 59.36% | 57.84% | 55.03% | 65.17% |
| table_calls_string_short | 20.13% | 18.40% | 21.68% | -2.56% | 20.36% |
| table_calls_string_long | 7.62% | -1.99% | 16.32% | 6.48% | 15.87% |
| table_calls_string_missing | 1.14% | -1.25% | 2.53% | -0.30% | -0.65% |
| table_calls_bpm_direct | 15.12% | 22.31% | -17.33% | 37.21% | 22.90% |
| table_calls_bpm_fallback | 13.66% | 32.63% | 33.72% | 43.95% | 31.33% |
| table_calls_bpm_missing | 16.72% | 0.57% | 1.45% | 0.95% | 0.31% |
| table_calls_authors | 19.85% | 18.19% | 16.68% | 26.32% | 12.66% |

Cases with higher aggregate median CPU cycles:

- `lazy_numbers_integer_8_8`: 1.31% more cycles; wall time 1884.4 → 1909.4 ns/op.
- `lazy_numbers_integer_127_127`: 0.25% more cycles; wall time 43281.2 → 43396.9 ns/op.
- `lazy_numbers_integer_1024_32`: 2.20% more cycles; wall time 105893.8 → 108271.9 ns/op.
- `lazy_numbers_integer_1024_33`: 0.51% more cycles; wall time 66731.2 → 67068.8 ns/op.
- `lazy_numbers_number_1024_1024`: 1.01% more cycles; wall time 148196.9 → 149678.1 ns/op.
- `lazy_numbers_mixed_1024_1024`: 1.32% more cycles; wall time 198762.5 → 201090.6 ns/op.
- `lazy_numbers_table_512_512`: 0.43% more cycles; wall time 99393.8 → 99709.4 ns/op.
- `table_calls_string_missing`: 0.65% more cycles; wall time 62.1 → 62.7 ns/op.

## Interpretation and remaining costs

All five final repetitions improved the representative distinct-integer,
zigzag-geometry and BPM-fallback workloads. Allocation reductions were identical
across repetitions. No measured scenario increased allocation/reallocation/free
calls or requested/freed bytes.

CPU savings are not universal:

- The unchanged linear path for 1,024 values with 32 unique integers was slower
  in all five pairs: 2.20% more cycles in the aggregate. There is no allocation
  improvement on this path. The eight-integer case was slower in four pairs;
  its aggregate cost is 1.31%, while the median paired cost is 7.12%. The
  per-run table exposes this variability rather than interpreting these cases
  as improvements.
- The 33-unique-integer indexed case uses fewer allocations and bytes, but was
  slower in four pairs: 0.51% more aggregate cycles (median paired cost 1.95%).
  Float-only and mixed-number controls show small aggregate CPU costs with
  unchanged allocations. Those costs remain part of this pass's tradeoff.
- Straight-line geometry at two points is strongly order-sensitive: old-first
  runs favor the new code, while both reversed-order runs show roughly 30–34%
  more cycles for the new code. The eight-point straight-line case also varies
  with order. Their aggregate percentages are not reliable universal speedup
  estimates. The zigzag workloads improve in every pair, and removing the
  offset-vector allocation is consistent across all measured geometry cases
  with at least two input points and positive width.
- Empty-input, disabled-output and missing-method controls have no allocation
  savings. Their very small absolute times are sensitive to measurement and
  code-layout effects.

The late-float projection reserves based on the accepted integer count. Distinct
large integers can share a rounded float representation, so that count can be
an overestimate of the required projection storage. The tables measure ordinary
integer ranges and late floats, not large alias-heavy performance workloads;
numeric alias correctness is covered by the regression tests. These results do
not establish lower memory usage for every possible input.
