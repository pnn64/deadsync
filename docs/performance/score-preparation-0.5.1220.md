# Score preparation performance, 0.5.1220

Baseline: `bb0126368` / 0.5.1219. This pass applies the supplied
`rust-performance.md` guide's M-HOTPATH, M-INITIAL-CAPACITY, M-MEM-REUSE and
M-THROUGHPUT guidance to score presentation and library import preparation.

## Three optimizations

1. **Reject leaderboard candidates outside the retained prefix.** When a
   priority phase is full, an equal or worse neighbor key cannot displace its
   last entry. Reject it before comparing duplicate names or searching for an
   insertion position. Candidates better than the first retained entry prepend
   directly, avoiding a binary search on descending ranks/distances. Equal-key
   entries retain their original first-winner behavior. World record, self and rival priorities, duplicate identity,
   neighbor distance, final ordering and short-list behavior are unchanged.
   Selection runs in a separate function so its workspace does not increase
   the cost of returning an already-short list.
2. **Allocate date labels once.** The existing permissive date parser is kept;
   the formatter reserves space from the year and bounded day length, writes
   the month/day, and appends the year. Normal labels no longer grow a String
   during formatting. Unrecognized dates and empty/placeholder behavior remain.
3. **Avoid owned pack names during import filtering.** A single nonempty pack
   filter compares borrowed group/display names without constructing a hash
   table or lowercase strings. Multiple filters retain hashed lookup but reuse
   one lowercase buffer across the library. Group matches skip normalization
   of the display alias; identical group/display names avoid a second lookup.
   The scratch buffer starts at 64 bytes and can grow for longer names. Hash
   deduplication, case-sensitive chart hashes, output ordering, existing-score
   exclusions and ownership of returned strings are preserved.

Leaderboard selection prepares gameplay scorebox rows and evaluation records
panes. Date labels are used in Select Music, replay selection, evaluation
records and event progress. Pack filtering prepares the profile's score-import
requests. These changes reduce preparation work when snapshots/screens or
imports are built; they do not establish a whole-game frame-time improvement.

No dependencies, production allocator changes or unsafe code were added.

## Measurement

Windows x86-64, Intel Xeon E5-2696 v4 @ 2.20 GHz, Rust 1.98.0
(`88d9e12ae`, LLVM 22.1.8), `x86_64-pc-windows-msvc`. Release, full LTO,
no additional CPU-feature flags. Benchmark processes inherit affinity to
logical CPU 4. Final measurements run without compilation or tests alongside.

The three original source blocks in
`crates/deadsync-score/tests/score_preparation/baseline.rs` were checked against
the baseline commit, including inline attributes. The integration harness calls
the actual public production APIs and the frozen baseline through opaque
function pointers in the same executable, with input/output barriers. Fixture
construction and equivalence assertions are outside the measured operations;
result allocation and destruction are included.

Five invocations alternate variant order: new first on runs 2 and 4. Each uses
the shared `tests/support/perf.rs` harness: three warmups, seven timing batches
and a separate allocation-counted operation. Results below are medians of the
five invocation medians. Windows `QueryThreadCycleTime` measures calling-thread
CPU cycles; throughput is derived from wall time. Loop/dispatch overhead is
included. Heap counters measure requested/freed bytes, not process RSS or
hardware memory traffic.

- Selection retains five entries from lists of 5, 10, 100 or 1,024 entries,
  with self in the middle and a rival every 19 entries. Orders include sorted,
  reversed, shuffled and equal ranks. A separate 1,024-entry sorted fixture has
  no rivals. `top` uses rank priority; `near` fills by distance to self. Each
  timing batch has 1,000 calls. Units are input entries.
- Dates use batches of 64 labels, with 1,000 operations per timing batch.
  Normal inputs vary month/day and include a timestamp. The fallback control
  mixes empty and unrecognized dates. Units are labels.
- Imports scan 512 packs with four unique charts each, except `one-pack-miss`
  which scans one pack. Tests cover no filter, blank filter, missing filters,
  one matching group/display alias, and 128 matching group filters. Existing
  scores are empty. Each timing batch has 150 calls. Units are input packs.

## Results

Times/cycles are per operation; throughput is millions of input units per
second. Negative values in the "fewer cycles" column mean more CPU work.
The [raw CSV](score-preparation-0.5.1220.csv) retains all 260 measurements,
including timing ranges and allocation/reallocation/free/byte counters.

| Workload | ns old -> new | Thread cycles old -> new | Fewer cycles | M units/s old -> new |
|---|---:|---:|---:|---:|
| `select/5-sorted-top` | 12.3 -> 13.1 | 28.5 -> 30.3 | -6.3% | 406.504 -> 381.679 |
| `select/5-sorted-near` | 15.1 -> 13.6 | 34.7 -> 31.2 | 10.1% | 331.126 -> 367.647 |
| `select/10-sorted-top` | 166.5 -> 112.4 | 366.6 -> 248.5 | 32.2% | 60.060 -> 88.968 |
| `select/10-sorted-near` | 198.7 -> 165.1 | 437.5 -> 357.6 | 18.3% | 50.327 -> 60.569 |
| `select/100-sorted-top` | 337.9 -> 303.2 | 743.0 -> 660.7 | 11.1% | 295.946 -> 329.815 |
| `select/100-sorted-near` | 308.5 -> 319.5 | 678.7 -> 702.8 | -3.6% | 324.149 -> 312.989 |
| `select/1024-sorted-top` | 3,936.5 -> 3,336.1 | 8,634.0 -> 7,244.6 | 16.1% | 260.130 -> 306.945 |
| `select/1024-sorted-near` | 3,905.1 -> 3,532.1 | 8,565.8 -> 7,747.9 | 9.5% | 262.221 -> 289.913 |
| `select/1024-reverse-top` | 4,078.3 -> 4,164.3 | 8,946.2 -> 9,054.2 | -1.2% | 251.085 -> 245.900 |
| `select/1024-reverse-near` | 4,315.1 -> 4,467.6 | 9,457.8 -> 9,797.8 | -3.6% | 237.306 -> 229.206 |
| `select/1024-shuffle-top` | 4,708.1 -> 4,003.5 | 10,312.5 -> 8,779.3 | 14.9% | 217.498 -> 255.776 |
| `select/1024-shuffle-near` | 4,617.3 -> 4,193.6 | 10,128.4 -> 9,187.0 | 9.3% | 221.775 -> 244.182 |
| `select/1024-ties-top` | 7,803.6 -> 3,690.9 | 17,114.0 -> 8,099.6 | 52.7% | 131.221 -> 277.439 |
| `select/1024-ties-near` | 7,513.3 -> 3,689.8 | 16,456.1 -> 8,091.0 | 50.8% | 136.292 -> 277.522 |
| `select/1024-no-rivals-top` | 15,484.1 -> 8,336.4 | 33,853.7 -> 18,284.1 | 46.0% | 66.132 -> 122.835 |
| `select/1024-no-rivals-near` | 25,501.4 -> 15,086.6 | 55,916.7 -> 33,035.8 | 40.9% | 40.155 -> 67.875 |
| `date/normal` | 23,081.0 -> 13,412.2 | 50,600.9 -> 29,345.4 | 42.0% | 2.773 -> 4.772 |
| `date/fallback` | 6,563.5 -> 7,745.8 | 14,395.5 -> 16,988.9 | -18.0% | 9.751 -> 8.263 |
| `import/none` | 548,294.7 -> 523,892.0 | 1,198,996.8 -> 1,147,428.9 | 4.3% | 0.934 -> 0.977 |
| `import/blank` | 547,950.7 -> 508,538.7 | 1,198,834.4 -> 1,112,635.3 | 7.2% | 0.934 -> 1.007 |
| `import/miss` | 103,748.0 -> 10,584.0 | 227,419.6 -> 23,215.8 | 89.8% | 4.935 -> 48.375 |
| `import/two-miss` | 112,002.7 -> 44,606.7 | 245,403.9 -> 97,825.3 | 60.1% | 4.571 -> 11.478 |
| `import/one-pack-miss` | 430.0 -> 44.7 | 954.1 -> 108.3 | 88.6% | 2.326 -> 22.388 |
| `import/group` | 109,268.0 -> 18,600.7 | 239,546.2 -> 40,781.6 | 83.0% | 4.686 -> 27.526 |
| `import/alias` | 98,600.0 -> 19,902.7 | 215,742.2 -> 43,630.7 | 79.8% | 5.193 -> 25.725 |
| `import/many` | 261,376.7 -> 202,247.3 | 572,780.9 -> 442,769.5 | 22.7% | 1.959 -> 2.532 |

Large sorted/shuffled leaderboards with rivals use **9.3-16.1% fewer cycles**.
The no-rival 1,024-entry fixture saves **46.0%** for rank priority and **40.9%**
for nearest-self priority; equal-rank fixtures save **50.8-52.7%**. Selection
still has zero allocations, reallocations, frees and requested/freed bytes.

Normal date labels use **42.0% fewer cycles**. Per 64 labels, reallocations
fall from **64 to zero** and requested/freed bytes from **1,536 to 768**.
Both versions allocate/free 64 owning result strings. The new formatter
eliminates their intermediate growth rather than eliminating owned outputs.

Matching a single pack group uses **83.0% fewer cycles**; matching its display
alias saves **79.8%**. Unmatched single filters save **89.8%** across 512 packs,
with zero heap churn; a one-pack miss also remains allocation-free. Multiple
filters save **60.1%** when none match and **22.7%** when 128 groups match.

| Import operation | Allocations/frees old -> new | Reallocations old -> new | Requested/freed bytes old -> new |
|---|---:|---:|---:|
| Single filter, no match, 512 packs | 1,026 -> 0 | 0 -> 0 | 10,875 -> 0 |
| Two filters, no match, 512 packs | 1,027 -> 4 | 0 -> 0 | 10,882 -> 194 |
| Single filter, matching group | 1,035 -> 9 | 0 -> 0 | 11,477 -> 600 |
| Single filter, matching display alias | 1,035 -> 9 | 0 -> 0 | 11,480 -> 600 |
| 128 matching filters | 1,937 -> 914 | 5 -> 5 | 93,720 -> 83,032 |
| Unfiltered/blank filter | 3,084 -> 3,084 | 7 -> 7 | 276,396 -> 276,396 |

Allocation counters are identical across all five invocations of each
workload/variant. Remaining successful-import allocations own the returned
hashes/names and lookup/output containers. Multiple-filter scratch is local to
the call and released afterward; unusually long names can grow it.

The controls are not uniformly faster. Reverse-ordered 1,024-entry selection
uses **1.2%/3.6% more cycles**, with wall-time increases of 86.0/152.5 ns.
The 100-entry nearest-self case uses 3.6% more cycles (11.0 ns), and the
five-entry rank-priority return uses 6.3% more (0.8 ns). The unchanged date
fallback fixture measures **18.0% more cycles**, or **18.5 ns more per label**;
its allocation counters are identical. No fallback-date speedup is claimed.
Unfiltered/blank imports have unchanged allocations and measure 4.3%/7.2%
fewer cycles, but their collection algorithm is unchanged and no optimization
claim is made for those controls. Timing ranges vary materially between runs;
the full ranges are retained in the CSV rather than treating every input as a
win. These are synchronous synthetic preparation benchmarks, not an end-to-end
profile of screen loading or online request latency.


## Validation and reproduction

Four new regression tests cover:

- Exact selected entry identities and order, limits including 0/1 and
  heap-backed limits, self/rival priority, duplicate names, equal/extreme ranks,
  several input orders and 1,000 deterministic randomized leaderboards.
- Exact date/placeholder strings for valid and malformed timestamps, Unicode,
  whitespace, leading plus signs/zeroes, zero/overflowing month/day values,
  4,545 generated date combinations and 2,000 randomized strings.
- 512 filter combinations with Unicode/ASCII case distinctions, display aliases,
  blank names/filters, duplicate/trimmed/case-sensitive chart hashes, existing
  scores, and growing names up to 3,800 bytes in the multiple-filter path.
- Zero heap churn for five-row selection and unmatched single-filter imports,
  one allocation/no growth per valid date label, and bounded/reduced churn for
  multiple-filter library scans.

Both debug and release runs pass all four new regression tests. The broader
score/profile run passes **470 tests** (seven manual benchmarks ignored),
including existing score ranking, leaderboard, date, import and profile tests.
The locked application check, scoped formatting check and
`cargo clippy -p deadsync-score --lib --locked -- -D clippy::perf` pass.
Clippy reports existing non-performance warnings in unchanged score/rules code.
The workspace version and all three inheriting lockfile package versions move
from 0.5.1219 to 0.5.1220, exactly one patch increment.

Reproduce with:

```powershell
cargo test -p deadsync-score -p deadsync-profile --locked -- --test-threads=1
cargo test -p deadsync-score --release --test score_preparation --locked -- --test-threads=1
cargo test -p deadsync-score --release --test score_preparation --locked -- --ignored --exact benchmark_score_preparation --nocapture --test-threads=1
$env:DEADSYNC_PERF_REVERSE = '1'
cargo test -p deadsync-score --release --test score_preparation --locked -- --ignored --exact benchmark_score_preparation --nocapture --test-threads=1
Remove-Item Env:DEADSYNC_PERF_REVERSE
```

For comparable timing, invoke the built test executable five times with the
same CPU affinity, alternating the environment flag and keeping other work
idle. Compiler/CPU, leaderboard shape, filter selectivity and allocator can
change the measured gains.
