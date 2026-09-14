# Player score preparation - 0.5.1225

Baseline: `608e7ae31` (0.5.1224). This pass applies the supplied
`rust-performance.md` guidance on measured hot paths (M-HOTPATH) and reusing
owned allocations (M-MEM-REUSE). The guide is excluded from the commit.

The final five-run medians show **67.7% fewer thread cycles** for a 128-row
name-fallback lookup and **18.3% fewer cycles** for complete conversion of
512 ITG scores. A selected nonempty comment is moved with **zero allocations**
in place of one copy allocation. The response fixture with 4 KiB comments uses
**51.7% less requested byte churn**.
Whole-response CPU gains are smaller and workload-dependent; all controls are below.

## Changes

1. Move the selected GrooveStats comment into the imported score. Both leaderboard
   loading and score import now retain the response's existing string buffer.
   The borrowing API remains available. Display leaderboards still prefer the
   first `is_self` row over a username match; imports retain their distinct
   endpoint-specific, first-matching-row rule.
2. Find the ITL player once and derive presence, score, and rank from that row.
   Previously both response converters searched for it three times. Presence
   stays true for a failed/non-finite score or zero rank, even when those values
   are absent. These lookups were already allocation-free and remain so.
3. Parse fixed-width ITG import timestamps directly into a validated Chrono date
   and time. Other formats continue through the old format parser; canonical but
   out-of-range dates return `None` directly. Both paths use the same `Local.from_local_datetime` conversion and DST ambiguity
   policy. This speeds up the complete `local_score_from_itg` conversion as well.

## Method

Windows x86_64 MSVC; Intel Xeon E5-2696 v4 @ 2.20 GHz; rustc 1.98.0
(88d9e12ae, 2026-08-18), LLVM 22.1.8. Release optimization level 3 and full
LTO. The Windows timezone is Romance Standard Time (Paris), including DST.
Seven frozen production function bodies match baseline Git sources, ignoring
formatting. Unchanged helpers and types are shared.

Five fresh processes run the same old/new release binary, pinned to logical
CPU 4. Order alternates old-first/new-first/old-first/new-first/old-first.
Each comparison has three warmups and seven timing samples; tables give the
median of the five per-process medians. Allocation accounting is a separate
single operation. Source hashes were checked against the final benchmark build.
No benchmark overlaps Cargo builds or tests.

Lookup batches contain 10,000 operations, timestamp batches 4,000, and bulk ITG
batches 100 conversions of 512 scores. Comment and whole-response samples
contain 1,000 operations. Every input and function pointer is opaque to the
optimizer. Whole-response fixtures contain GS, EX, SRPG, and ITL leaderboards.
Comments include judgment counts, EX evidence, and padding to 128 or 4,096 bytes.
Small, empty, missing-player, name-fallback, and large-list controls are included.

Consuming operations receive identically cloned fixtures outside timing and
allocation accounting. Whole-response measurements include input consumption
and output destruction. Isolated comment measurements include returned-score
destruction, but exclude destruction of the remaining input rows. Thus that
benchmark still frees one comment on both sides; the new transfer itself is
verified to perform no heap operations when its output remains alive. Imported
score preparation can have zero allocations while still freeing consumed input
buffers. Existing APIs that return owned panes still allocate those panes.

CPU counts use Windows `QueryThreadCycleTime` for the current thread. The
setup-excluding helper queries clocks around each individual operation, so its
cycle counts include substantial clock overhead; tiny controls must not be
interpreted as raw instruction costs. Date and lookup batches query clocks
only around each batch. Throughput counts input leaderboard rows, timestamps,
ITG scores, or selected comments as appropriate; empty row workloads have zero
rows per second by definition.

The counting allocator forwards to System and records allocations,
reallocations, frees, and requested/freed byte churn. Timings include the
allocator's disabled-accounting TLS checks. These are not peak RSS,
committed heap pages, cache misses, total application CPU, network latency, or
frame rate. The timestamp and lookup paths have zero measured heap churn in
both versions. The CSV contains all five runs, sample ranges, and counters.

## Behavior coverage

Five new differential/ownership tests cover 9,000 player-summary comparisons,
7,200 complete response comparisons across three endpoints and both pane
orders, original comment-buffer identity, and reduced allocation churn.
Date checks cover 9,408 calendar/time combinations, 2,432 single-byte ASCII
substitutions, 2,048 random Unicode strings, and explicit relaxed-width,
whitespace, signed/extended-year, leap-second, DST gap/fold, and invalid inputs.
Another 6,144 full ITG score comparisons exercise failure state, judgments,
rate modifiers, and finite/non-finite percentages. The date conversion uses
the current local timezone; these tests do not enumerate every world timezone.

## Reproduce

```powershell
cargo test -p deadsync-online -p deadsync-score --locked
cargo test -p deadsync-online --lib --release --locked player_responses
cargo test -p deadsync-online --lib --release --locked groovestats::player_responses::benchmark_player_responses -- --ignored --exact --nocapture --test-threads=1
```

For the recorded method, build first and run the emitted `deadsync_online-*.exe`
directly, pinned to one logical CPU. Repeat five times, setting
`DEADSYNC_PERF_REVERSE=1` for runs 2 and 4 and removing it for the others.

## Validation

- 553 score/online unit and integration tests passed; 16 manual benchmarks ignored.
- All five new differential/ownership tests also passed in release mode.
- `cargo check --locked` passed for the application.
- `cargo clippy -p deadsync-online -p deadsync-score --lib --locked -- -D clippy::perf`
  passed; existing non-performance warnings remain.
- Scoped Rust formatting and Git whitespace checks passed.
- All seven frozen function bodies and final source hashes were verified.
- Every allocation/reallocation/free/byte counter was identical across all five runs.

## Timing and throughput

| Workload | ns old -> new | Thread cycles old -> new | Fewer cycles | Units/s old -> new |
|---|---:|---:|---:|---:|
| `datetime/canonical` | 1,321.8 -> 1,070.6 | 2,895.0 -> 2,349.0 | 18.9% | 756,558.4 -> 934,033.9 |
| `datetime/trimmed` | 1,331.4 -> 967.4 | 2,918.2 -> 2,118.9 | 27.4% | 751,089.1 -> 1,033,698.6 |
| `datetime/relaxed` | 1,337.3 -> 1,217.1 | 2,932.6 -> 2,668.4 | 9.0% | 747,747.4 -> 821,625.2 |
| `datetime/leap-second` | 1,354.3 -> 1,297.0 | 2,970.4 -> 2,846.4 | 4.2% | 738,361.6 -> 771,024.9 |
| `datetime/invalid` | 42.0 -> 44.4 | 92.6 -> 95.6 | -3.2% | 23,809,523.8 -> 22,522,522.5 |
| `datetime/invalid-date` | 264.6 -> 19.7 | 580.1 -> 43.7 | 92.5% | 3,779,646.6 -> 50,697,084.9 |
| `itg-import/512` | 749,236.0 -> 611,887.0 | 1,641,952.1 -> 1,341,087.9 | 18.3% | 683,362.8 -> 836,755.8 |
| `summary/0-missing` | 21.9 -> 13.2 | 48.8 -> 29.3 | 40.0% | 0.0 -> 0.0 |
| `summary/1-self` | 24.6 -> 16.8 | 54.2 -> 37.1 | 31.5% | 40,600,893.2 -> 59,382,422.8 |
| `summary/10-self` | 40.5 -> 20.1 | 89.0 -> 44.3 | 50.2% | 247,157,686.6 -> 498,504,486.5 |
| `summary/128-self` | 272.6 -> 96.8 | 597.9 -> 212.1 | 64.5% | 469,535,233.5 -> 1,322,314,049.6 |
| `summary/128-name` | 2,615.9 -> 845.9 | 5,736.0 -> 1,855.3 | 67.7% | 48,930,972.9 -> 151,310,967.7 |
| `summary/128-missing` | 2,410.9 -> 866.3 | 5,287.3 -> 1,898.9 | 64.1% | 53,092,866.9 -> 147,754,819.3 |
| `summary/1024-name` | 21,825.0 -> 7,425.2 | 47,833.5 -> 16,278.9 | 66.0% | 46,918,563.8 -> 137,908,557.5 |
| `comment/0` | 86.9 -> 81.1 | 1,725.3 -> 1,707.5 | 1.0% | 11,507,479.9 -> 12,330,456.2 |
| `comment/128` | 258.0 -> 205.4 | 2,131.1 -> 2,024.9 | 5.0% | 3,875,969.0 -> 4,868,549.2 |
| `comment/4096` | 403.7 -> 225.7 | 2,438.0 -> 2,006.5 | 17.7% | 2,477,086.9 -> 4,430,660.2 |
| `response/0-missing-0` | 282.9 -> 268.8 | 2,187.1 -> 2,123.0 | 2.9% | 0.0 -> 0.0 |
| `import/0-missing-0` | 134.8 -> 134.1 | 1,804.3 -> 1,774.7 | 1.6% | 0.0 -> 0.0 |
| `response/10-self-128` | 4,350.0 -> 4,514.2 | 11,093.5 -> 11,520.0 | -3.8% | 9,195,402.3 -> 8,860,927.7 |
| `import/10-self-128` | 2,947.9 -> 2,737.3 | 8,009.8 -> 7,384.2 | 7.8% | 13,568,981.3 -> 14,612,939.8 |
| `response/128-self-128` | 46,189.1 -> 46,159.7 | 103,653.0 -> 103,518.6 | 0.1% | 11,084,866.3 -> 11,091,926.5 |
| `import/128-self-128` | 35,858.3 -> 35,616.2 | 80,644.1 -> 80,042.2 | 0.7% | 14,278,423.7 -> 14,375,480.8 |
| `response/128-name-128` | 49,361.8 -> 48,837.3 | 110,330.3 -> 109,056.6 | 1.2% | 10,372,393.2 -> 10,483,790.1 |
| `import/128-name-128` | 37,119.7 -> 34,119.6 | 83,451.7 -> 76,717.4 | 8.1% | 13,793,214.9 -> 15,006,037.6 |
| `response/128-missing-0` | 45,593.8 -> 46,084.1 | 102,154.0 -> 103,250.6 | -1.1% | 11,229,597.0 -> 11,110,122.6 |
| `import/128-missing-0` | 32,553.9 -> 31,091.5 | 73,172.3 -> 70,051.4 | 4.3% | 15,727,762.3 -> 16,467,523.3 |
| `response/10-self-4096` | 4,894.5 -> 4,701.0 | 12,438.2 -> 11,922.4 | 4.1% | 8,172,438.5 -> 8,508,827.9 |
| `import/10-self-4096` | 3,314.4 -> 2,915.3 | 8,950.1 -> 7,869.3 | 12.1% | 12,068,549.4 -> 13,720,714.8 |

## Allocation churn per operation

All datetime, ITG conversion, and player-summary counters are zero before and after.

| Workload | Allocations old -> new | Reallocations old -> new | Frees old -> new | Requested bytes old -> new | Freed bytes old -> new |
|---|---:|---:|---:|---:|---:|
| `comment/0` | 0 -> 0 | 0 -> 0 | 0 -> 0 | 0 -> 0 | 0 -> 0 |
| `comment/128` | 1 -> 0 | 0 -> 0 | 1 -> 1 | 128 -> 0 | 128 -> 128 |
| `comment/4096` | 1 -> 0 | 0 -> 0 | 1 -> 1 | 4,096 -> 0 | 4,096 -> 4,096 |
| `response/0-missing-0` | 1 -> 1 | 0 -> 0 | 3 -> 3 | 280 -> 280 | 291 -> 291 |
| `import/0-missing-0` | 0 -> 0 | 0 -> 0 | 2 -> 2 | 0 -> 0 | 11 -> 11 |
| `response/10-self-128` | 10 -> 9 | 0 -> 0 | 122 -> 121 | 3,962 -> 3,834 | 10,861 -> 10,733 |
| `import/10-self-128` | 1 -> 0 | 0 -> 0 | 113 -> 112 | 128 -> 0 | 7,027 -> 6,899 |
| `response/128-self-128` | 10 -> 9 | 0 -> 0 | 1,340 -> 1,339 | 45,498 -> 45,370 | 134,161 -> 134,033 |
| `import/128-self-128` | 1 -> 0 | 0 -> 0 | 1,331 -> 1,330 | 128 -> 0 | 88,791 -> 88,663 |
| `response/128-name-128` | 10 -> 9 | 0 -> 0 | 1,340 -> 1,339 | 45,498 -> 45,370 | 134,145 -> 134,017 |
| `import/128-name-128` | 1 -> 0 | 0 -> 0 | 1,331 -> 1,330 | 128 -> 0 | 88,775 -> 88,647 |
| `response/128-missing-0` | 9 -> 9 | 0 -> 0 | 1,211 -> 1,211 | 45,370 -> 45,370 | 117,649 -> 117,649 |
| `import/128-missing-0` | 0 -> 0 | 0 -> 0 | 1,202 -> 1,202 | 0 -> 0 | 72,279 -> 72,279 |
| `response/10-self-4096` | 10 -> 9 | 0 -> 0 | 122 -> 121 | 7,930 -> 3,834 | 54,509 -> 50,413 |
| `import/10-self-4096` | 1 -> 0 | 0 -> 0 | 113 -> 112 | 4,096 -> 0 | 50,675 -> 46,579 |

## Controls and limits

All behavior tests pass. These workloads have higher median cycle counts:

- `datetime/invalid`: 3.2% more cycles; 42.0 -> 44.4 ns/op.
- `response/10-self-128`: 3.8% more cycles; 4,350.0 -> 4,514.2 ns/op.
- `response/128-missing-0`: 1.1% more cycles; 45,593.8 -> 46,084.1 ns/op.

The complete response measurements include destruction of all input and output
buffers. They do not show the same proportional improvement as the isolated
lookup. Small controls and setup-excluding cycle measurements are sensitive
to clock overhead and overlapping sample ranges; the CSV preserves those ranges.
The measurements cover local score preparation; whole-application performance
was not measured.

Raw data: [player-responses-0.5.1225.csv](player-responses-0.5.1225.csv).
