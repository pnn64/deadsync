# Course selection performance - 0.5.1148

This pass applies `M-HOTPATH`, `M-INITIAL-CAPACITY`, and `M-THROUGHPUT` from
`rust-performance.md` to course stage resolution. Course browsing resolves
stages for previews and difficulty ratings; endless-course rerolls use the same
resolver. Measurements cover this synchronous selection work, not disk scanning,
full screen loading, gameplay frame rate, RSS, or process-wide peak memory.

## Three changes

1. **Borrow candidate songs and SELECT pools.** Candidate records borrow the
   library's `Arc<SongData>`; only the returned stage clones its selected song.
   SELECT traverses borrowed group slices, eliminating its copied pool and
   atomic reference-count increments/decrements for discarded candidates.
   Exact group lookup, repeated groups, and traversal order remain unchanged.
2. **Apply SELECT metadata predicates before chart construction.** Rejected
   songs contribute no chart scans or chart-index capacity. Surviving songs
   retain their original source indices, preserving ranking ties and seeded
   choices. Metadata is checked once; record storage reserves the pool upper
   bound after its first match, avoiding repeated parsing or growth churn.
   Entries without metadata filters bypass predicate checks entirely.
3. **Scan directly for first and last ranked candidates.** Most/fewest plays
   and top/lowest grades select their endpoints without a ranking vector.
   The same score and source-index comparator determines ties. Interior ranks
   retain nth-element selection; a singleton avoids score lookup altogether.

Production uses safe Rust with no new dependencies, persistent caches, or API
changes. Candidate/chart buffers and lazily materialized song keys still allocate
where needed. Warm endpoint ranking has zero allocator churn; complete ranked
resolution still builds keys. Returned stages retain owned song handles.

## Method

Baseline: `12e4c25ff` / 0.5.1147. Fifteen frozen selection function bodies match
that commit ignoring whitespace; visibility/imports differ. Unchanged song-key,
path/seed hashing, and difficulty-shift helpers are shared and were verified
against that commit. The `filter_*` pairs are explicitly adapted controls: both
borrow candidates and use the new builder, with metadata applied after versus
before chart construction. Other old/new pairs use frozen baseline functions.

Windows x64, Intel Xeon E5-2696 v4 at 2.20 GHz; rustc 1.98.0 (`88d9e12ae`,
LLVM 22.1.8); repository release profile (opt-level 3, full LTO). Both versions
run in one executable using the same fixtures, System allocator wrapper, and
opaque function pointers. Inputs/results are black-boxed. Fixture generation
is excluded; temporary/output creation and disposal are included.

Each measurement warms three times, then times seven batches with allocation
counters disabled. A separate operation records allocations, reallocations,
frees, requested bytes, and freed bytes. Windows `QueryThreadCycleTime` counts
calling-thread CPU cycles; these functions launch no workers. Tables show
medians of three invocation medians, ordered old/new, new/old, old/new, with no
builds or tests during final timings. Small percentage differences may be noise.

The synthetic library contains 2,048 songs across four groups and 12 charts per
song, with six difficulties, invalid note data and another chart type mixed in.
Scores include ties and missing entries; grades span all 19 buckets. Artist
filtering accepts 1/32 of songs. Broad BPM filtering uses 100-200 BPM; default,
fixed-display, ranged-display, and wildcard BPM metadata are mixed. Group SELECT
visits Pack2, Pack0, Pack2 (1,536 pool entries, including duplicates). Empty,
one-song, 32-song/six-chart, and 2,048-song/no-chart controls are included.
Endless random resolution also covers histories of eight and 512 selected songs.

`candidate_build` and `filter_*` time 64 operations per batch. Complete large
stage resolution uses 32; warm-key ranking uses 128; small/empty/no-chart stage
controls use 512. Candidate/filter/ranking throughput counts input songs;
`resolve_*` throughput counts completed resolver calls, including calls returning
None. Ranking candidates and keys are prepared before warm-rank timings.

## Timing and throughput

Positive cycle savings indicate improvement. Throughput units are defined above;
endpoint and whole-resolver gains overlap and must not be added together.

| Workload | Old ns/op | New ns/op | Old cycles/op | New cycles/op | Cycles saved | Old units/s | New units/s |
|---|---:|---:|---:|---:|---:|---:|---:|
| filter_artist | 1,123,020.3 | 336,464.1 | 2,455,220.7 | 735,335.3 | 70.1% | 1,823,653.6 | 6,086,831.3 |
| filter_bpm | 1,195,620.3 | 1,187,668.8 | 2,616,546.3 | 2,600,543.4 | 0.6% | 1,712,918.4 | 1,724,386.5 |
| candidate_build | 813,834.4 | 774,590.6 | 1,782,185.7 | 1,695,236.3 | 4.9% | 2,516,482.5 | 2,643,977.3 |
| resolve_random | 809,459.4 | 789,318.8 | 1,771,227.9 | 1,723,404.3 | 2.7% | 1,235.4 | 1,266.9 |
| resolve_select_bpm | 1,318,125.0 | 1,208,134.4 | 2,887,810.5 | 2,645,997.0 | 8.4% | 758.7 | 827.7 |
| resolve_select_all | 1,140,171.9 | 818,468.8 | 2,491,277.0 | 1,790,159.7 | 28.1% | 877.1 | 1,221.8 |
| resolve_select_artist | 1,121,531.2 | 339,306.2 | 2,455,635.7 | 743,179.0 | 69.7% | 891.6 | 2,947.2 |
| resolve_select_none | 1,123,421.9 | 310,978.1 | 2,458,166.9 | 682,720.5 | 72.2% | 890.1 | 3,215.7 |
| resolve_select_groups | 1,044,881.2 | 737,078.1 | 2,287,025.4 | 1,614,676.3 | 29.4% | 957.0 | 1,356.7 |
| resolve_most_plays | 1,293,009.4 | 1,282,703.1 | 2,825,356.0 | 2,810,381.9 | 0.5% | 773.4 | 779.6 |
| resolve_top_grades | 1,354,412.5 | 1,338,490.6 | 2,967,057.0 | 2,923,794.8 | 1.5% | 738.3 | 747.1 |
| resolve_middle | 1,331,328.1 | 1,293,918.8 | 2,915,488.2 | 2,835,055.2 | 2.8% | 751.1 | 772.8 |
| resolve_endless_8 | 1,220,871.9 | 1,170,206.2 | 2,673,372.8 | 2,562,484.1 | 4.1% | 819.1 | 854.6 |
| resolve_endless_512 | 1,279,503.1 | 1,247,484.4 | 2,801,444.2 | 2,729,256.2 | 2.6% | 781.6 | 801.6 |
| rank_most_first | 106,371.1 | 104,742.2 | 233,062.7 | 229,074.0 | 1.7% | 19,253,351.0 | 19,552,770.9 |
| rank_fewest_last | 106,893.8 | 102,695.3 | 233,844.7 | 224,510.8 | 4.0% | 19,159,211.8 | 19,942,487.6 |
| rank_top_first | 142,656.2 | 141,412.5 | 312,629.7 | 309,712.8 | 0.9% | 14,356,188.4 | 14,482,453.8 |
| rank_lowest_last | 139,006.2 | 133,957.0 | 304,196.1 | 292,987.9 | 3.7% | 14,733,150.5 | 15,288,484.5 |
| rank_middle | 116,714.8 | 116,485.9 | 255,889.0 | 255,432.8 | 0.2% | 17,547,039.7 | 17,581,521.4 |
| resolve_empty | 33.0 | 25.6 | 75.0 | 58.7 | 21.7% | 30,295,858.0 | 39,083,969.5 |
| resolve_single | 732.0 | 703.7 | 1,609.4 | 1,549.8 | 3.7% | 1,366,061.9 | 1,421,038.0 |
| resolve_small | 4,886.5 | 4,559.2 | 10,720.3 | 10,001.0 | 6.7% | 204,644.5 | 219,337.7 |
| resolve_no_charts | 49,778.9 | 1,816.0 | 109,049.5 | 3,989.6 | 96.3% | 20,088.8 | 550,656.1 |

## Allocation churn

Counts include result disposal. Bytes mean allocator requests, not live heap or
peak memory. `A/R/F` means allocations/reallocations/frees per operation.

| Workload | Old A/R/F | New A/R/F | Old requested bytes | New requested bytes | Old freed bytes | New freed bytes |
|---|---:|---:|---:|---:|---:|---:|
| filter_artist | 2/0/2 | 2/0/2 | 311,296 | 120,832 | 311,296 | 120,832 |
| filter_bpm | 2/0/2 | 2/0/2 | 311,296 | 311,296 | 311,296 | 311,296 |
| candidate_build | 2/0/2 | 2/0/2 | 311,296 | 311,296 | 311,296 | 311,296 |
| resolve_random | 4/0/4 | 4/0/4 | 311,322 | 311,322 | 311,322 | 311,322 |
| resolve_select_bpm | 5/0/5 | 4/0/4 | 327,706 | 311,322 | 327,706 | 311,322 |
| resolve_select_all | 5/0/5 | 4/0/4 | 327,706 | 311,322 | 327,706 | 311,322 |
| resolve_select_artist | 5/0/5 | 4/0/4 | 327,706 | 120,858 | 327,706 | 120,858 |
| resolve_select_none | 3/0/3 | 0/0/0 | 327,680 | 0 | 327,680 | 0 |
| resolve_select_groups | 5/2/5 | 4/0/4 | 262,170 | 233,498 | 262,170 | 233,498 |
| resolve_most_plays | 2052/0/2052 | 2051/0/2051 | 371,638 | 338,870 | 371,638 | 338,870 |
| resolve_top_grades | 2052/0/2052 | 2051/0/2051 | 371,638 | 338,870 | 371,638 | 338,870 |
| resolve_middle | 2052/0/2052 | 2052/0/2052 | 371,638 | 371,638 | 371,638 | 371,638 |
| resolve_endless_8 | 2051/0/2051 | 2051/0/2051 | 338,870 | 338,870 | 338,870 | 338,870 |
| resolve_endless_512 | 2052/0/2052 | 2052/0/2052 | 356,294 | 356,294 | 356,294 | 356,294 |
| rank_most_first | 1/0/1 | 0/0/0 | 32,768 | 0 | 32,768 | 0 |
| rank_fewest_last | 1/0/1 | 0/0/0 | 32,768 | 0 | 32,768 | 0 |
| rank_top_first | 1/0/1 | 0/0/0 | 32,768 | 0 | 32,768 | 0 |
| rank_lowest_last | 1/0/1 | 0/0/0 | 32,768 | 0 | 32,768 | 0 |
| rank_middle | 1/0/1 | 1/0/1 | 32,768 | 32,768 | 32,768 | 32,768 |
| resolve_empty | 0/0/0 | 0/0/0 | 0 | 0 | 0 | 0 |
| resolve_single | 5/0/5 | 4/0/4 | 191 | 175 | 191 | 175 |
| resolve_small | 4/0/4 | 4/0/4 | 3,352 | 3,352 | 3,352 | 3,352 |
| resolve_no_charts | 2/0/2 | 0/0/0 | 131,072 | 0 | 131,072 | 0 |

## Interpretation

Borrowed candidate construction saves 4.9% of cycles with unchanged buffer
allocation; unfiltered/group SELECT calls save 28.1%/29.4% of cycles and remove
the copied pool. Artist-filtered resolution saves 69.7% of cycles and reduces
requested/freed bytes from 327,706 to 120,858 (63.1%). Its adapted predicate-placement
control saves 70.1% of cycles. Broad BPM filtering is approximately flat in that
isolated control (+0.6%), while complete resolution improves 8.4% through the
combined changes. The rejected two-metadata-pass draft was slower for broad
filters; only the once-per-song implementation is included here.

Warm first/last ranking removes one 32,768-byte allocation/free per operation.
Its measured cycle savings are modest (0.9-4.0%); complete most-played/top-grade
resolution is near flat (0.5-1.5%) because key creation and chart scanning still
dominate. The allocation reduction is deterministic. Interior-rank scratch is
unchanged and its isolated timing is effectively flat (+0.2%). Endless random
controls improve 2.6-4.1%, with unchanged key/history allocation costs. Small
percentage differences are not evidence of a universal CPU improvement.

A fully rejected SELECT and the no-chart SELECT control allocate nothing. No-chart
resolution also avoids building empty records and improves 96.3% in this synthetic
control; that is not representative of a populated song library.

## Behavior and validation

Six new behavior/allocation tests cover candidate ownership and chart order;
metadata filtering, exact group lookup, duplicates and source indices; 42,048
complete old/new resolver comparisons across seeds, all four course types,
six difficulties and repeat histories (including the eight/nine-key lookup
threshold); every rank for sizes 0, 1, 2, 3, 9, 64 and 257 with reordered
candidates, ties and missing scores; allocation budgets; and stage ownership
after the library is dropped. Comparisons preserve song identity, chart index,
modifiers, gain-seconds bits and gain-lives. NaN/reversed ranges and failed
matches are covered.

- `cargo test -p deadsync-simfile --locked`: 194 passed, 3 manual benchmarks ignored; doc-tests passed.
- `cargo test -p deadsync-simfile --lib --release --locked`: 194 passed, 3 ignored.
- `cargo check -p deadsync --all-targets --locked`: passed, including the course UI consumer.
- `cargo clippy -p deadsync-simfile --all-targets --locked -- -D clippy::perf`: passed; existing style warnings remain.
- `rustfmt --edition 2024 --check crates/deadsync-simfile/src/course.rs`: passed, including its test modules.
- Frozen baseline body verification, final source-hash verification across all three benchmark rounds, and `git diff --check`: passed.

Reproduce behavior and the benchmark:

```powershell
cargo test -p deadsync-simfile --locked
cargo test -p deadsync-simfile --lib --release --locked
cargo test -p deadsync-simfile --lib --release course_selection_bench --locked -- --ignored --nocapture --test-threads=1
```

Repeat the benchmark three times, setting `DEADSYNC_PERF_REVERSE=1` only for the
middle invocation. Run without concurrent builds/tests. Timing assertions are
intentionally absent; deterministic allocation budgets are enforced by tests.
