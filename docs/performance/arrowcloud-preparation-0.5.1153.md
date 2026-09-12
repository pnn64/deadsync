# ArrowCloud payload preparation — 0.5.1153

Baseline: `4b4edc786` (0.5.1152). This pass applies the supplied
`rust-performance.md` guidance on measured hot paths (M-HOTPATH), initial
capacity (M-INITIAL-CAPACITY), reusable resources (M-MEM-REUSE), and throughput
(M-THROUGHPUT). The guide itself is excluded from the commit.

Across 34 paired cases repeated three times, complete payload preparation used
40.96–43.62% fewer thread cycles and 49.80–99.52% fewer requested bytes in the
three full-payload fixtures. Thirty cases used fewer cycles; the four exceptions
are listed below with their absolute timings.

## Changes

1. **Stream judged rows into timing data.** `visit_scatter_points` shares the
   original row scanner with `build_scatter_points`, and lets ArrowCloud
   consume points without a temporary scatter vector. Timing output reuses
   the judged-row count already present in score statistics as its capacity
   hint, only allocating after the first accepted datum. This also avoids a
   second chart traversal just to count rows.
   Empty, unjudged, and completely filtered timing results allocate nothing.
2. **Prepare life-history sampling once.** Reuse the starting life and the
   common history lower bound across all samples. Find the lower bound lazily
   so graphs that never interpolate do not pay for a search. Each sample keeps
   its original binary search and floating-point operation order.
3. **Borrow fixed modifier labels.** Fixed labels use static strings, and the
   three mask lists reserve their exact enabled-label counts. Only the
   configurable noteskin name needs an owned string. JSON field names, label
   order, scroll priority and omitted fields remain unchanged.

## Method

Windows x86_64 MSVC; Intel Xeon E5-2696 v4 @ 2.20 GHz; rustc 1.98.0
(88d9e12ae, 2026-08-18), LLVM 22.1.8. Cargo release uses optimization level 3
and full LTO. Old implementations are frozen in the test-only baseline, with
shared unchanged types/helpers and calls redirected to the frozen scanner.
Both payloads use the current engine version to isolate preparation behavior.

Each comparison ran in the same release binary, with opaque function pointers
and black-boxed inputs where appropriate. Three invocations alternated
old/new, new/old, old/new. Each invocation uses three warmups and seven timing
batches, then separately counts one operation. Tables report the median of
the three per-invocation medians. Iterations range from 128 for large charts
to 10,000 for small operations. No benchmark ran alongside a Cargo build.

CPU counts use Windows QueryThreadCycleTime for the calling thread. The
scoped allocator delegates to System and counts allocations, reallocations,
frees and requested bytes. Returned values are dropped inside the measured
operation. Bytes describe allocation churn, not process peak RSS, committed
heap pages, cache misses or the production allocator. These are local
preparation benchmarks, not network latency or overall game frame rate.

Timing/scatter fixtures contain 0 or 16 rows, 8,192 single taps, 4,096 four-note
chords, 8,192 two-note rows with a 3-second fail cutoff, or 8,192 mixed four-note
rows with skipped notes and invalid offsets. Unjudged/filtered cases contain
2,048 four-note rows. Full payload and combined preparation cases contain
8,192 rows; combined excludes metadata and NPS construction, while payload
includes those plus owned metadata strings. Life fixtures request 100 points
except the one-point case; histories contain 0, 1, 64 or 8,192 records.
Modifier JSON benchmarks reuse a caller-owned serialization buffer.

Timing/payload fixtures precompute score statistics with the existing score
builder before timing, matching the runtime input contract. Both full payload
implementations receive the same cached statistics. Statistic computation is
outside these preparation benchmarks; the new code only sums six cached
judgment counters. It does not scan notes to obtain a capacity hint.

## Timing, CPU and throughput

Throughput below is complete operations per second (not note count); positive
CPU savings mean fewer cycles. Raw harness output also reports input rows/s,
requested life points/s or modifier operations/s as appropriate.

| Case | Old ns/op | New ns/op | Old cycles/op | New cycles/op | CPU saved | Old → new ops/s |
|---|---:|---:|---:|---:|---:|---:|
| timing_empty | 25.6 | 17.2 | 56.3 | 38.0 | 32.50% | 39,062,500 → 58,139,535 |
| scatter_empty | 18.7 | 21.6 | 41.2 | 47.5 | -15.29% | 53,475,936 → 46,296,296 |
| timing_short | 327.1 | 225.3 | 717.7 | 494.4 | 31.11% | 3,057,169 → 4,438,526 |
| scatter_short | 256.3 | 249.1 | 561.7 | 546.4 | 2.72% | 3,901,678 → 4,014,452 |
| timing_taps | 309,661.7 | 115,012.5 | 678,489.9 | 251,509.3 | 62.93% | 3,229 → 8,695 |
| scatter_taps | 132,411.7 | 124,670.3 | 289,688.6 | 272,648.2 | 5.88% | 7,552 → 8,021 |
| timing_chords | 164,819.5 | 106,207.0 | 360,472.2 | 232,735.2 | 35.44% | 6,067 → 9,416 |
| scatter_chords | 139,813.3 | 136,914.1 | 304,480.8 | 298,911.0 | 1.83% | 7,152 → 7,304 |
| timing_failed | 146,337.5 | 99,833.6 | 320,216.2 | 218,297.9 | 31.83% | 6,834 → 10,017 |
| scatter_failed | 162,981.2 | 160,894.5 | 357,258.5 | 352,048.9 | 1.46% | 6,136 → 6,215 |
| timing_mixed | 247,740.6 | 187,917.2 | 542,406.8 | 411,742.6 | 24.09% | 4,036 → 5,321 |
| scatter_mixed | 244,843.8 | 243,128.9 | 536,101.3 | 532,364.7 | 0.70% | 4,084 → 4,113 |
| timing_unjudged | 42,516.4 | 34,071.1 | 93,054.3 | 74,052.1 | 20.42% | 23,520 → 29,350 |
| scatter_unjudged | 40,085.2 | 40,473.4 | 87,210.1 | 88,741.5 | -1.76% | 24,947 → 24,708 |
| timing_filtered | 60,320.3 | 44,558.6 | 132,183.6 | 97,602.0 | 26.16% | 16,578 → 22,442 |
| scatter_filtered | 66,566.4 | 66,850.8 | 145,967.5 | 146,602.0 | -0.43% | 15,023 → 14,959 |
| life_empty | 14.3 | 14.7 | 32.2 | 32.8 | -1.86% | 69,930,070 → 68,027,211 |
| life_single | 829.0 | 680.5 | 1,818.1 | 1,491.3 | 17.97% | 1,206,273 → 1,469,508 |
| life_short | 3,390.6 | 1,394.3 | 7,437.3 | 3,059.4 | 58.86% | 294,933 → 717,206 |
| life_dense | 8,799.6 | 3,875.4 | 19,257.6 | 8,464.9 | 56.04% | 113,642 → 258,038 |
| life_trimmed | 8,381.0 | 3,657.1 | 18,349.3 | 8,019.7 | 56.29% | 119,318 → 273,441 |
| life_one_point | 85.0 | 70.0 | 188.2 | 154.2 | 18.07% | 11,764,706 → 14,285,714 |
| modifiers_default | 305.9 | 94.4 | 671.2 | 207.1 | 69.14% | 3,269,042 → 10,593,220 |
| modifiers_json_default | 630.0 | 369.1 | 1,382.4 | 809.8 | 41.42% | 1,587,302 → 2,709,293 |
| modifiers_mixed | 905.7 | 291.1 | 1,986.7 | 637.1 | 67.93% | 1,104,118 → 3,435,246 |
| modifiers_json_mixed | 1,267.5 | 662.2 | 2,762.5 | 1,452.2 | 47.43% | 788,955 → 1,510,118 |
| modifiers_full | 2,695.9 | 304.0 | 5,897.8 | 658.8 | 88.83% | 370,934 → 3,289,474 |
| modifiers_json_full | 3,516.5 | 815.4 | 7,709.3 | 1,789.2 | 76.79% | 284,374 → 1,226,392 |
| payload_taps | 154,644.5 | 88,401.6 | 338,385.0 | 193,146.3 | 42.92% | 6,466 → 11,312 |
| combined_taps | 144,357.8 | 89,941.4 | 316,428.1 | 196,956.7 | 37.76% | 6,927 → 11,118 |
| payload_chords | 332,776.6 | 196,526.6 | 728,776.0 | 430,242.3 | 40.96% | 3,005 → 5,088 |
| combined_chords | 269,183.6 | 199,963.3 | 588,561.8 | 438,216.3 | 25.54% | 3,715 → 5,001 |
| payload_failed | 178,910.2 | 100,834.4 | 392,123.0 | 221,070.8 | 43.62% | 5,589 → 9,917 |
| combined_failed | 150,252.3 | 96,866.4 | 328,812.7 | 212,362.8 | 35.42% | 6,655 → 10,323 |

## Allocation churn

Each allocation and free count matches, and each allocated-byte and
freed-byte count matches, for every row below. All counts are per operation.

| Case | Allocs/frees old → new | Reallocs old → new | Allocated/freed bytes old → new |
|---|---:|---:|---:|
| timing_empty | 0 → 0 | 0 → 0 | 0 → 0 |
| scatter_empty | 0 → 0 | 0 → 0 | 0 → 0 |
| timing_short | 2 → 1 | 0 → 0 | 768 → 384 |
| scatter_short | 1 → 1 | 0 → 0 | 384 → 384 |
| timing_taps | 2 → 1 | 0 → 0 | 393,216 → 196,608 |
| scatter_taps | 1 → 1 | 0 → 0 | 196,608 → 196,608 |
| timing_chords | 2 → 1 | 0 → 0 | 491,520 → 98,304 |
| scatter_chords | 1 → 1 | 0 → 0 | 393,216 → 393,216 |
| timing_failed | 2 → 1 | 0 → 0 | 589,824 → 984 |
| scatter_failed | 1 → 1 | 0 → 0 | 393,216 → 393,216 |
| timing_mixed | 2 → 1 | 0 → 0 | 972,672 → 186,240 |
| scatter_mixed | 1 → 1 | 0 → 0 | 786,432 → 786,432 |
| timing_unjudged | 1 → 0 | 0 → 0 | 196,608 → 0 |
| scatter_unjudged | 1 → 1 | 0 → 0 | 196,608 → 196,608 |
| timing_filtered | 2 → 0 | 0 → 0 | 245,760 → 0 |
| scatter_filtered | 1 → 1 | 0 → 0 | 196,608 → 196,608 |
| life_empty | 0 → 0 | 0 → 0 | 0 → 0 |
| life_single | 1 → 1 | 0 → 0 | 1,600 → 1,600 |
| life_short | 1 → 1 | 0 → 0 | 1,600 → 1,600 |
| life_dense | 1 → 1 | 0 → 0 | 1,600 → 1,600 |
| life_trimmed | 1 → 1 | 0 → 0 | 1,600 → 1,600 |
| life_one_point | 1 → 1 | 0 → 0 | 16 → 16 |
| modifiers_default | 4 → 1 | 0 → 0 | 19 → 3 |
| modifiers_json_default | 4 → 1 | 0 → 0 | 19 → 3 |
| modifiers_mixed | 13 → 4 | 0 → 0 | 343 → 99 |
| modifiers_json_mixed | 13 → 4 | 0 → 0 | 343 → 99 |
| modifiers_full | 27 → 4 | 4 → 0 | 1,381 → 323 |
| modifiers_json_full | 27 → 4 | 4 → 0 | 1,381 → 323 |
| payload_taps | 24 → 14 | 0 → 0 | 395,320 → 198,468 |
| combined_taps | 16 → 6 | 0 → 0 | 395,159 → 198,307 |
| payload_chords | 24 → 14 | 0 → 0 | 985,144 → 198,468 |
| combined_chords | 16 → 6 | 0 → 0 | 984,983 → 198,307 |
| payload_failed | 24 → 14 | 0 → 0 | 591,928 → 2,844 |
| combined_failed | 16 → 6 | 0 → 0 | 591,767 → 2,683 |

## Tradeoffs and interpretation

- `scatter_empty` used 15.29% more cycles (18.7 → 21.6 ns/op).
- `scatter_unjudged` used 1.76% more cycles (40,085.2 → 40,473.4 ns/op).
- `scatter_filtered` used 0.43% more cycles (66,566.4 → 66,850.8 ns/op).
- `life_empty` used 1.86% more cycles (14.3 → 14.7 ns/op).

Capacity follows the existing score counters, which may include rows whose
offsets are subsequently filtered. Oversized initial hints are capped at note
count; underestimated or missing counts let Vec grow normally. Tests cover
zero, small, accurate and oversized hints, so allocation sizing cannot drop
timing data. Such inaccurate hints can reallocate; the measured runtime-like
fixtures use accurate cached counts and have no timing reallocations. No
output allocation is made if every row is rejected.

The initial iterator design regressed the existing scatter builder and was
replaced by the inlined visitor before these final measurements. The initial
eager life-history preparation also added a search to one-point graphs;
lazy setup removes that unnecessary search. Counting rows in a separate
chart traversal reduced churn but slowed mixed charts; reusing existing
score counters removes that traversal. Only final measurements appear above.

Fixed-label Rust fields now use static string references rather than String.
All workspace callers compile; serialized payload compatibility is checked
byte-for-byte. The configurable noteskin retains ownership for worker safety.

## Behavioral validation

- Frozen old/new comparisons cover row grouping and misses,
  representative timestamps, missing time-cache entries, ignored fake/mine/
  unjudgeable notes, player-column ranges, fail boundaries and nonfinite data.
- Scatter comparisons check every field, including parity joins, directions
  and held misses; the visitor itself has a zero-churn assertion.
- Life comparisons check float bits for empty/single/dense histories,
  duplicates, short intervals, unusual ordering, negative times, invalid
  rates, NaNs/infinities and 0/1/100 requested samples.
- All 65,536 modifier masks have identical label lists. Every valid mask and
  all scalar label choices have JSON comparisons, including scroll priority.
- Complete payload JSON bytes match across 48 chart/profile/fail combinations.
- Allocation assertions cover row-sized timing capacity, a failed chart with
  41 submitted rows, inaccurate capacity hints, zero-output paths,
  the one-allocation life output and one/four-allocation modifier outputs.

Checks and reproduction:

```powershell
cargo test -p deadsync-rules -p deadsync-gameplay -p deadsync-online --lib --locked -- --test-threads=1
cargo test -p deadsync-online --release --lib --locked -- --test-threads=1
cargo test -p deadsync-online --release --lib --locked arrowcloud_preparation_bench -- --ignored --test-threads=1 --nocapture
$env:DEADSYNC_PERF_REVERSE = "1" # repeat benchmark in reverse order
Remove-Item Env:DEADSYNC_PERF_REVERSE # repeat once more in normal order
cargo clippy -p deadsync-online -p deadsync-rules --all-targets --locked -- -D clippy::perf
cargo check --all-targets --locked
```

Debug: 1,139 tests passed (772 gameplay, 258 online, 109 rules). Release:
258 online tests passed. The performance lint check and root all-targets
check passed. Live network tests remain ignored. Benchmark logs, per-round
measurements and source hashes are local ignored artifacts under
`target/arrowcloud-preparation-perf/`; the frozen baseline and harness are
committed so comparisons can be reproduced.
