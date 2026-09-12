# SRPG shop refresh and download completion - 0.5.1156

Baseline: `b35f21fb0` (0.5.1155). This pass follows the supplied
`rust-performance.md` guidance on measured CPU and allocation costs (M-HOTPATH),
reusing storage (M-MEM-REUSE), capacity planning (M-INITIAL-CAPACITY), and batch
processing (M-THROUGHPUT). The guide itself is excluded from the commit.

Large (2,048-item) order fixtures use 80.37-92.61% fewer thread cycles.
Bulk known/new/mixed merges use 49.78-81.30% fewer cycles. The all-new batch
reduces requested byte churn from 584,010 to 480,218 bytes and removes eight
reallocations. Unique completion with 1,024 items drops from 5,124 allocations
to zero. Nine of 46 control/comparison cases use more median cycles, by at most
3.62%; their full results and memory tradeoffs are listed below.

## Changes

1. **Restore shop order with an in-place permutation.** An unchanged ID sequence
   returns immediately. Otherwise, a borrowed-ID index links duplicate
   occurrences, assigns their destinations in previous-display order, and swaps
   records into place. This replaces repeated linear searches and large-record
   shifts. Unmatched/new records retain their incoming order; refreshed metadata
   stays attached to the correct occurrence. The item vector and owned strings
   are reused. Nontrivial reorders allocate two smaller scratch buffers.
2. **Resolve bulk download merge targets before moving payloads.** A borrowed-ID
   index retains the first existing occurrence and assigns one append position
   per new ID. A target vector allows the borrowed index to be dropped before
   mutating the records. The output reserves the exact required additional
   length once. Repeated downloads update the first matching record and preserve
   append order. Batches of at most 16 downloads, or at most 32 combined records,
   use linear lookup without indexing allocations.
3. **Avoid copying shop snapshots when completion changes nothing.** A read-only
   check finds an undownloaded matching URL in the selected destination before
   requesting mutable access. Missing, already-completed and wrong-folder
   updates allocate nothing. Arc::make_mut updates a unique snapshot in place
   and clones only when necessary to protect an existing reader. Publication
   still increments the display generation once per actual change, under the
   existing runtime mutex.

## Method

Windows x86_64 MSVC; Intel Xeon E5-2696 v4 @ 2.20 GHz; rustc 1.98.0
(88d9e12ae, 2026-08-18), LLVM 22.1.8. Cargo release uses optimization level 3
and full LTO. The frozen order and merge implementations match the baseline
commit. The old completion implementation only adapts the global runtime state
to an isolated Arc and explicit folder argument, returning its changed flag.
Both completion measurements exclude the unchanged config read, mutex and
generation publication.

All 46 old/new pairs run in the same release binary with black-boxed inputs.
Three invocations alternate old/new, new/old, old/new; each uses three warmups,
seven timing batches and a separate allocation-counted operation. These tables
use the median of the three per-invocation medians. No Cargo build runs beside
the benchmarks. Allocation counts are identical across the three runs.

Windows QueryThreadCycleTime counts calling-thread cycles. The existing scoped
allocator delegates to System and records allocation calls, reallocations,
frees and requested bytes separately from timing. Results are dropped inside
each measured operation. Requested byte churn includes replacement buffers;
it is not peak RSS, committed heap size, cache misses or a measurement of the
production allocator. No end-to-end game, frame-rate or network claim follows
from these isolated CPU measurements.

Order fixtures have 0, 1, 128 or 2,048 items and unchanged, reversed, rotated or
changed IDs. Changed fixtures include repeated IDs and new/missing occurrences.
**Both order measurements include cloning the current input snapshot and
dropping the result.** Previous order is borrowed. Batches contain 256 operations
for up to 128 items and eight for 2,048 items. The zero-churn unchanged-order
claim refers to the helper itself, separately enforced by an allocation test;
the table includes the common fixture clone.

Merge fixtures use item/download counts: empty 0/0, tiny 4/4, eight 32/8,
nine 64/9, sixteen 64/16, seventeen 64/17, few_in_large 2,048/17,
small_existing_new 8/1,024, known 1,024/1,024, new 0/1,024,
mixed 512/1,024, and duplicates 128/1,024. Known IDs arrive in reverse order;
mixed input repeats some new IDs. Duplicate fixtures have 32 distinct IDs.
**Both merge measurements include cloning the item vector and all download
strings, applying updates and dropping the result.** Batches contain 16
operations above 128 downloads, otherwise 256. These totals include required
output strings and shared fixture costs as well as temporary index costs.

Completion fixtures have 0, 1 or 1,024 items. Setup is outside measurement.
Changed unique/shared cases reset the first item's flag on both sides before
each operation. Shared cases acquire and drop a reader during every measured
operation. Missing, already-completed and wrong-folder cases make no change;
all zero-item cases are misses. Batches contain 512 operations for 0/1 items
and 32 for 1,024 items. Weak-reader semantics are tested separately.

## Timing, CPU and throughput

Throughput below is complete operations per second, calculated from ns/op.
Raw benchmark output also reports item/download units per second; for completion
these represent fixture size, not a promise that every record is inspected.
Positive CPU savings mean fewer cycles. Very short empty cases are especially
sensitive to timer, harness and code-layout overhead.

| Case | Old ns/op | New ns/op | Old cycles/op | New cycles/op | CPU saved | Old -> new ops/s |
|---|---:|---:|---:|---:|---:|---:|
| order_0_same | 141.8 | 135.2 | 317.2 | 302.7 | 4.57% | 7,052,186 -> 7,396,450 |
| order_0_reverse | 142.6 | 143.4 | 318.1 | 320.7 | -0.82% | 7,012,623 -> 6,973,501 |
| order_0_rotate | 137.9 | 131.2 | 307.0 | 292.4 | 4.76% | 7,251,632 -> 7,621,951 |
| order_0_changed | 137.9 | 135.2 | 307.8 | 301.8 | 1.95% | 7,251,632 -> 7,396,450 |
| order_1_same | 776.2 | 514.5 | 1,709.7 | 1,134.4 | 33.65% | 1,288,328 -> 1,943,635 |
| order_1_reverse | 516.4 | 499.2 | 1,138.7 | 1,100.9 | 3.32% | 1,936,483 -> 2,003,205 |
| order_1_rotate | 512.1 | 424.2 | 1,129.2 | 936.3 | 17.08% | 1,952,744 -> 2,357,379 |
| order_1_changed | 484.8 | 425.0 | 1,068.3 | 937.2 | 12.27% | 2,062,706 -> 2,352,941 |
| order_128_same | 54,022.3 | 37,678.5 | 118,428.0 | 82,589.4 | 30.26% | 18,511 -> 26,540 |
| order_128_reverse | 59,060.2 | 44,753.1 | 129,473.3 | 98,084.8 | 24.24% | 16,932 -> 22,345 |
| order_128_rotate | 56,887.9 | 44,664.8 | 124,687.2 | 97,898.7 | 21.48% | 17,578 -> 22,389 |
| order_128_changed | 55,483.6 | 44,722.3 | 121,631.3 | 98,035.9 | 19.40% | 18,023 -> 22,360 |
| order_2048_same | 11,714,837.5 | 865,950.0 | 25,674,200.9 | 1,898,071.4 | 92.61% | 85 -> 1,155 |
| order_2048_reverse | 5,088,662.5 | 1,001,912.5 | 11,149,173.6 | 2,188,332.8 | 80.37% | 197 -> 998 |
| order_2048_rotate | 7,322,250.0 | 1,035,575.0 | 16,038,096.4 | 2,268,505.0 | 85.86% | 137 -> 966 |
| order_2048_changed | 9,650,787.5 | 1,044,775.0 | 21,150,196.8 | 2,288,507.1 | 89.18% | 104 -> 957 |
| merge_empty | 21.1 | 21.5 | 50.6 | 52.3 | -3.36% | 47,393,365 -> 46,511,628 |
| merge_tiny | 2,902.0 | 2,700.8 | 6,374.1 | 5,921.4 | 7.10% | 344,590 -> 370,261 |
| merge_eight | 13,784.0 | 13,946.9 | 30,232.7 | 30,606.5 | -1.24% | 72,548 -> 71,701 |
| merge_nine | 25,238.3 | 25,354.3 | 55,362.9 | 55,611.5 | -0.45% | 39,622 -> 39,441 |
| merge_sixteen | 29,392.2 | 29,896.9 | 64,460.1 | 65,525.9 | -1.65% | 34,023 -> 33,448 |
| merge_seventeen | 29,813.3 | 30,370.3 | 65,318.4 | 66,462.2 | -1.75% | 33,542 -> 32,927 |
| merge_few_in_large | 758,415.2 | 728,110.9 | 1,661,789.1 | 1,595,123.6 | 4.01% | 1,319 -> 1,373 |
| merge_small_existing_new | 3,069,793.8 | 675,925.0 | 6,724,986.0 | 1,482,118.8 | 77.96% | 326 -> 1,479 |
| merge_known | 2,105,300.0 | 878,143.8 | 4,614,356.5 | 1,921,763.8 | 58.35% | 475 -> 1,139 |
| merge_new | 3,037,150.0 | 568,162.5 | 6,655,775.1 | 1,244,839.3 | 81.30% | 329 -> 1,760 |
| merge_mixed | 1,415,712.5 | 711,212.5 | 3,103,112.7 | 1,558,532.2 | 49.78% | 706 -> 1,406 |
| merge_duplicates | 502,943.8 | 521,512.5 | 1,102,891.4 | 1,142,840.4 | -3.62% | 1,988 -> 1,917 |
| mark_0_unique | 142.6 | 15.0 | 315.1 | 35.6 | 88.70% | 7,012,623 -> 66,666,667 |
| mark_0_shared | 158.6 | 26.0 | 350.7 | 60.0 | 82.89% | 6,305,170 -> 38,461,538 |
| mark_0_missing | 140.6 | 16.4 | 310.8 | 39.0 | 87.45% | 7,112,376 -> 60,975,610 |
| mark_0_shared_missing | 160.9 | 26.0 | 355.4 | 60.0 | 83.12% | 6,215,040 -> 38,461,538 |
| mark_0_already | 140.4 | 15.4 | 310.8 | 36.4 | 88.29% | 7,122,507 -> 64,935,065 |
| mark_0_wrong_folder | 140.0 | 7.8 | 309.5 | 19.7 | 93.63% | 7,142,857 -> 128,205,128 |
| mark_1_unique | 490.8 | 50.0 | 1,079.5 | 112.3 | 89.60% | 2,037,490 -> 20,000,000 |
| mark_1_shared | 503.7 | 519.5 | 1,108.2 | 1,142.9 | -3.13% | 1,985,309 -> 1,924,928 |
| mark_1_missing | 423.0 | 15.4 | 931.6 | 36.4 | 96.09% | 2,364,066 -> 64,935,065 |
| mark_1_shared_missing | 436.9 | 27.7 | 961.2 | 63.9 | 93.35% | 2,288,853 -> 36,101,083 |
| mark_1_already | 436.3 | 27.3 | 959.9 | 63.0 | 93.44% | 2,292,001 -> 36,630,037 |
| mark_1_wrong_folder | 413.9 | 7.6 | 910.6 | 19.3 | 97.88% | 2,416,043 -> 131,578,947 |
| mark_1024_unique | 306,450.0 | 1,037.5 | 672,150.1 | 2,318.5 | 99.66% | 3,263 -> 963,855 |
| mark_1024_shared | 299,759.4 | 302,056.2 | 657,100.7 | 661,943.3 | -0.74% | 3,336 -> 3,311 |
| mark_1024_missing | 305,850.0 | 1,012.5 | 670,215.8 | 2,256.8 | 99.66% | 3,270 -> 987,654 |
| mark_1024_shared_missing | 304,134.4 | 1,084.4 | 666,450.0 | 2,414.5 | 99.64% | 3,288 -> 922,169 |
| mark_1024_already | 304,162.5 | 1,065.6 | 666,655.8 | 2,380.2 | 99.64% | 3,288 -> 938,438 |
| mark_1024_wrong_folder | 299,384.4 | 9.4 | 656,119.8 | 61.7 | 99.99% | 3,340 -> 106,382,979 |

## Allocation churn

Every allocation count equals its free count, and allocated bytes equal freed
bytes, in every measured operation. Columns therefore show alloc/free calls,
reallocation calls, and allocated/freed bytes, respectively (old -> new).

| Case | Allocations / frees | Reallocations | Allocated / freed bytes |
|---|---:|---:|---:|
| order_0_same | 2 -> 2 | 0 -> 0 | 51 -> 51 |
| order_0_reverse | 2 -> 2 | 0 -> 0 | 51 -> 51 |
| order_0_rotate | 2 -> 2 | 0 -> 0 | 51 -> 51 |
| order_0_changed | 2 -> 2 | 0 -> 0 | 51 -> 51 |
| order_1_same | 9 -> 8 | 0 -> 0 | 442 -> 282 |
| order_1_reverse | 9 -> 8 | 0 -> 0 | 442 -> 282 |
| order_1_rotate | 9 -> 8 | 0 -> 0 | 442 -> 282 |
| order_1_changed | 9 -> 8 | 0 -> 0 | 446 -> 286 |
| order_128_same | 644 -> 643 | 0 -> 0 | 50,829 -> 30,349 |
| order_128_reverse | 644 -> 645 | 0 -> 0 | 50,829 -> 40,861 |
| order_128_rotate | 644 -> 645 | 0 -> 0 | 50,829 -> 40,861 |
| order_128_changed | 644 -> 645 | 0 -> 0 | 50,797 -> 40,829 |
| order_2048_same | 10,244 -> 10,243 | 0 -> 0 | 825,989 -> 498,309 |
| order_2048_reverse | 10,244 -> 10,245 | 0 -> 0 | 825,989 -> 666,261 |
| order_2048_rotate | 10,244 -> 10,245 | 0 -> 0 | 825,989 -> 666,261 |
| order_2048_changed | 10,244 -> 10,245 | 0 -> 0 | 824,603 -> 664,875 |
| merge_empty | 0 -> 0 | 0 -> 0 | 0 -> 0 |
| merge_tiny | 42 -> 42 | 0 -> 0 | 1,764 -> 1,764 |
| merge_eight | 202 -> 202 | 0 -> 0 | 8,936 -> 8,936 |
| merge_nine | 367 -> 367 | 0 -> 0 | 16,404 -> 16,404 |
| merge_sixteen | 402 -> 402 | 0 -> 0 | 17,985 -> 17,985 |
| merge_seventeen | 407 -> 409 | 0 -> 0 | 18,164 -> 21,516 |
| merge_few_in_large | 10,327 -> 10,329 | 0 -> 0 | 481,562 -> 584,114 |
| merge_small_existing_new | 6,186 -> 6,195 | 8 -> 1 | 911,538 -> 534,178 |
| merge_known | 10,242 -> 10,244 | 0 -> 0 | 467,484 -> 526,892 |
| merge_new | 6,146 -> 6,148 | 8 -> 0 | 584,010 -> 480,218 |
| merge_mixed | 7,938 -> 7,940 | 1 -> 1 | 520,112 -> 512,960 |
| merge_duplicates | 5,762 -> 5,764 | 0 -> 0 | 259,014 -> 273,622 |
| mark_0_unique | 2 -> 0 | 0 -> 0 | 51 -> 0 |
| mark_0_shared | 2 -> 0 | 0 -> 0 | 51 -> 0 |
| mark_0_missing | 2 -> 0 | 0 -> 0 | 51 -> 0 |
| mark_0_shared_missing | 2 -> 0 | 0 -> 0 | 51 -> 0 |
| mark_0_already | 2 -> 0 | 0 -> 0 | 51 -> 0 |
| mark_0_wrong_folder | 2 -> 0 | 0 -> 0 | 51 -> 0 |
| mark_1_unique | 9 -> 0 | 0 -> 0 | 344 -> 0 |
| mark_1_shared | 9 -> 9 | 0 -> 0 | 344 -> 344 |
| mark_1_missing | 8 -> 0 | 0 -> 0 | 272 -> 0 |
| mark_1_shared_missing | 8 -> 0 | 0 -> 0 | 272 -> 0 |
| mark_1_already | 8 -> 0 | 0 -> 0 | 272 -> 0 |
| mark_1_wrong_folder | 8 -> 0 | 0 -> 0 | 272 -> 0 |
| mark_1024_unique | 5,124 -> 0 | 0 -> 0 | 236,237 -> 0 |
| mark_1024_shared | 5,124 -> 5,124 | 0 -> 0 | 236,237 -> 236,237 |
| mark_1024_missing | 5,123 -> 0 | 0 -> 0 | 236,165 -> 0 |
| mark_1024_shared_missing | 5,123 -> 0 | 0 -> 0 | 236,165 -> 0 |
| mark_1024_already | 5,123 -> 0 | 0 -> 0 | 236,165 -> 0 |
| mark_1024_wrong_folder | 5,123 -> 0 | 0 -> 0 | 236,165 -> 0 |

## Tradeoffs and limits

Reordered inputs exchange one large record-vector allocation for two smaller
scratch allocations. Bulk merge indexing adds temporary allocations and can
increase requested bytes for known records, repeated IDs, or a small batch in
a large shop. Its benefit is fewer repeated comparisons; new records also avoid
repeated output-vector growth. These paths are not allocation-free.

Completion with an active reader still requires a complete copy. Its preflight
scan and copy-on-write bookkeeping can add overhead to changed shared cases;
unchanged shared snapshots benefit from skipping the copy entirely. Reader
isolation is retained. Large relative differences in empty cases reflect only
nanoseconds of absolute work.

All cases with increased median thread cycles in the final run are listed here:

- `order_0_reverse`: 0.82% more cycles; 142.6 -> 143.4 ns/op. Allocation and byte costs are shown above.
- `merge_empty`: 3.36% more cycles; 21.1 -> 21.5 ns/op. Allocation and byte costs are shown above.
- `merge_eight`: 1.24% more cycles; 13,784.0 -> 13,946.9 ns/op. Allocation and byte costs are shown above.
- `merge_nine`: 0.45% more cycles; 25,238.3 -> 25,354.3 ns/op. Allocation and byte costs are shown above.
- `merge_sixteen`: 1.65% more cycles; 29,392.2 -> 29,896.9 ns/op. Allocation and byte costs are shown above.
- `merge_seventeen`: 1.75% more cycles; 29,813.3 -> 30,370.3 ns/op. Allocation and byte costs are shown above.
- `merge_duplicates`: 3.62% more cycles; 502,943.8 -> 521,512.5 ns/op. Allocation and byte costs are shown above.
- `mark_1_shared`: 3.13% more cycles; 503.7 -> 519.5 ns/op. Allocation and byte costs are shown above.
- `mark_1024_shared`: 0.74% more cycles; 299,759.4 -> 302,056.2 ns/op. Allocation and byte costs are shown above.

## Behavior and verification

- 274 online library tests pass in debug and release; five tests remain ignored
  by default, including the manually run benchmarks and live download test.
- Four new differential tests compare complete old/new outputs over empty, singleton,
  threshold, duplicate, Unicode, missing-ID and new-ID cases. They include
  512 deterministic generated order cases, 256 generated merge cases and the
  combined merge/order refresh pipeline.
- Completion tests cover all folder policies, multiple matching shops/items,
  wrong destinations, repeated notifications, retained readers, and weak
  references. They compare the changed flag and full snapshot to the baseline.
- A fifth new test enforces zero churn for unchanged order, empty merges,
  unique completion and repeated/missing completion with a reader. A reversed
  128-item order has bounded scratch churn and retains item-vector/string
  storage addresses.
- `cargo clippy -p deadsync-online --all-targets --locked -- -D clippy::perf`
  passes; existing non-performance warnings remain.
- `cargo check --all-targets --locked`, targeted rustfmt checks and
  `git diff --check` pass.
- The baseline/source audit verifies frozen behavior, unchanged shared helpers,
  final benchmark source hashes, main, and exactly one patch increment in
  Cargo.toml and all three version-inheriting Cargo.lock packages.

Reproduce from the repository root (PowerShell):

```powershell
cargo test -p deadsync-online --lib --locked -- --test-threads=1
cargo test -p deadsync-online --release --lib --locked -- --test-threads=1
Remove-Item Env:DEADSYNC_PERF_REVERSE -ErrorAction SilentlyContinue
cargo test -p deadsync-online --release --lib --locked srpg_shop_bench -- --ignored --test-threads=1 --nocapture
$env:DEADSYNC_PERF_REVERSE = '1'
cargo test -p deadsync-online --release --lib --locked srpg_shop_bench -- --ignored --test-threads=1 --nocapture
Remove-Item Env:DEADSYNC_PERF_REVERSE
cargo test -p deadsync-online --release --lib --locked srpg_shop_bench -- --ignored --test-threads=1 --nocapture
```

Raw local runs and source hashes are retained under `target/srpg-shop-perf/`
(ignored). The committed test-only harness and frozen implementations allow
the measurements to be repeated without those local scripts.
