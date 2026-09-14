# Online result preparation performance pass: 0.5.1224

Baseline: `231caabac` / 0.5.1223. This pass follows the local
`rust-performance.md` guidance on profiling CPU/allocation costs (M-HOTPATH),
reusing owned storage (M-MEM-REUSE), reserving known output sizes
(M-INITIAL-CAPACITY), and avoiding unnecessary work (M-THROUGHPUT).
The guide is excluded from the commit.

## Three changes

1. **ArrowCloud page merging.** Borrow user IDs for classification and duplicate
   checks. Transfer the existing String into the identity set only when the
   merge needs to retain it, trimming it in place. Previously every nonblank ID
   was copied, including discarded non-personalized rows and duplicate rows.
   First-page order/duplicates, later-page filtering, empty-ID behavior, Unicode
   whitespace trimming and self/rival flags are preserved.
2. **Submission event preparation.** Copy only displayed fields from borrowed
   GrooveStats leaderboard rows, avoiding the API-vector clone and discarded
   comments. New consuming score APIs move the resulting leaderboard buffers
   into overlay pages, removing the second copy of names, dates and machine
   tags. Existing borrowing APIs remain available. Reserve exact event/page
   counts and skip ineligible ITL data before constructing intermediate events.
   Responses with no eligible events now allocate nothing. Event ordering,
   summary/reward text, metadata, deltas and eligibility rules are preserved.
3. **Bulk score timestamps.** Parse the canonical UTC forms
   `YYYY-MM-DDTHH:MM:SSZ` and `YYYY-MM-DDTHH:MM:SS.sssZ` directly through checked
   Chrono date/time constructors. An inline helper rejects noncanonical shapes
   early; the caller retains the original RFC3339 parser for offsets, different
   fractional precision, leap seconds and other forms. Both implementations
   allocate nothing for timestamp parsing; this change targets CPU work.

## Method

Windows x86-64, Intel Xeon E5-2696 v4 @ 2.20 GHz; rustc 1.98.0
(88d9e12ae, 2026-08-18), LLVM 22.1.8, x86_64-pc-windows-msvc. Release uses
optimization level 3 and full LTO. Five independent release executable
invocations were pinned to logical CPU 4, old first in runs 1/3/5 and new first
in runs 2/4. No tests or builds ran alongside the measurements. The initial
diagnostic invocation is excluded.

Each case uses three warmups and seven timing batches: 128 operations per
batch for page merges and bulk conversion, 256 for event preparation, and
20,000 for scalar timestamp conversion. Function pointers and inputs pass
through `black_box`. Allocations, reallocations, frees and byte traffic are
counted in a separate operation. The allocator's TLS checks remain present
while timing, with counters disabled. CPU cycles use Windows
`QueryThreadCycleTime`, not elapsed TSC ticks.

Page merges consume fresh cloned inputs. The shared setup-aware benchmark
helper excludes input cloning from timing and allocation counts, but includes
input consumption/destruction and output destruction. Its per-operation clock
cost is included equally in old/new cycle measurements and is visible in the
empty/single-row controls. Other cases borrow prebuilt fixtures and include
output destruction. No HTTP requests, JSON decoding, disk I/O or rendering
are timed. The 23 frozen baseline functions match the previous commit after
whitespace normalization. SmallVec is added only as a dev dependency to run
the original reward formatting code unchanged; no runtime dependency is added.

Page cases cover zero/one/128 rows, five pages of 128 rows with selected
self/rivals, heavy duplicates, and Unicode-padded IDs. Event cases cover empty,
small (five rows per event), two 128-row events with quests/achievements,
4-KiB unused comments, and an ineligible ITL event. Bulk conversion processes
512 charts, each with the three global leaderboard IDs and millisecond UTC
timestamps. Throughput counts input rows for merging, complete calls for event
preparation/scalar timestamps, and scores (1,536 per call) for bulk conversion.
Empty merge throughput counts calls.

## Results

The table reports medians of five per-invocation medians. The
[raw CSV](online-results-0.5.1224.csv) retains all 180 rows, including the range
of each invocation's seven timing batches.

| Workload | ns old -> new | Thread cycles old -> new | Fewer cycles | Units/s old -> new |
|---|---:|---:|---:|---:|
| `merge/empty` | 184.4 -> 186.7 | 1,906.9 -> 1,865.8 | 2.2% | 5,423,728.8 -> 5,355,648.5 |
| `merge/one` | 396.1 -> 348.4 | 2,219.0 -> 2,345.9 | -5.7% | 2,524,654.8 -> 2,869,955.2 |
| `merge/page` | 23,978.9 -> 13,393.8 | 54,093.0 -> 30,280.7 | 44.0% | 5,338,025.0 -> 9,556,696.2 |
| `merge/paged` | 138,152.3 -> 86,096.9 | 304,954.2 -> 190,675.2 | 37.5% | 4,632,567.1 -> 7,433,487.0 |
| `merge/duplicates` | 134,279.7 -> 85,364.1 | 295,961.5 -> 189,205.6 | 36.1% | 4,766,171.4 -> 7,497,300.2 |
| `merge/spaces` | 141,728.9 -> 94,106.2 | 312,744.6 -> 208,447.8 | 33.3% | 4,515,663.2 -> 6,800,823.5 |
| `events/empty` | 301.6 -> 13.3 | 681.7 -> 34.3 | 95.0% | 3,316,062.2 -> 75,294,117.6 |
| `events/small` | 9,863.3 -> 7,235.5 | 21,637.9 -> 15,666.0 | 27.6% | 101,386.1 -> 138,206.6 |
| `events/events` | 155,345.7 -> 68,127.3 | 340,380.2 -> 149,292.6 | 56.1% | 6,437.3 -> 14,678.4 |
| `events/comments` | 226,005.9 -> 51,114.1 | 494,328.6 -> 111,999.9 | 77.3% | 4,424.7 -> 19,564.1 |
| `events/ineligible` | 41,393.0 -> 13.3 | 90,690.4 -> 35.2 | 99.96% | 24,158.7 -> 75,294,117.6 |
| `timestamp/milliseconds` | 71.3 -> 52.9 | 156.7 -> 115.5 | 26.3% | 14,018,364.1 -> 18,916,107.1 |
| `timestamp/seconds` | 63.3 -> 49.2 | 138.8 -> 108.0 | 22.2% | 15,801,532.7 -> 20,341,741.3 |
| `timestamp/offset` | 69.7 -> 69.3 | 153.1 -> 152.2 | 0.6% | 14,346,173.2 -> 14,425,851.1 |
| `timestamp/nanoseconds` | 73.2 -> 75.2 | 160.6 -> 164.9 | -2.7% | 13,664,002.2 -> 13,299,640.9 |
| `timestamp/leap` | 70.2 -> 73.3 | 154.0 -> 160.5 | -4.2% | 14,255,167.5 -> 13,650,945.3 |
| `timestamp/invalid` | 39.1 -> 38.1 | 85.7 -> 83.7 | 2.3% | 25,542,784.2 -> 26,257,056.6 |
| `bulk/512` | 149,732.0 -> 106,565.6 | 328,123.3 -> 233,549.7 | 28.8% | 10,258,326.1 -> 14,413,653.6 |

Paged merging uses 37.5% fewer cycles and increases throughput from 4.63 to
7.43 million input rows/s. Event preparation with quests/achievements uses
56.1% fewer cycles; throughput rises from 6,437 to 14,678 responses/s. Bulk
conversion uses 28.8% fewer cycles and increases from 10.26 to 14.41 million
scores/s. Scalar canonical timestamps use 22.2-26.3% fewer cycles.

Controls: the one-row merge reports 5.7% more cycles even though wall time
falls by 47.7 ns, with overlapping timing ranges and per-operation clock
costs larger than much of the work. No CPU gain is claimed for this control.
Empty merging rises by 2.3 ns. Nanosecond timestamps add 2.0 ns (2.7% cycles),
and the leap-second fallback adds 3.1 ns (4.2% cycles); sample ranges overlap.
Offset and invalid-date controls are approximately unchanged. These costs
are retained in the raw data; this is not a claim that every input is faster.
The earlier diagnostic's larger fallback regressions were reduced by keeping
the generic parser in the caller and using an inline canonical-format helper.

## Allocation traffic

All counters match across the five invocations. Requested/freed bytes count
allocator traffic, including reallocations; they do not measure peak RSS or
retained process memory. Page-merge inputs are allocated outside measurement
but freed inside it, so their freed bytes exceed newly requested bytes.

| Workload | Allocations old -> new | Reallocations old -> new | Frees old -> new | Requested bytes old -> new | Freed bytes old -> new |
|---|---:|---:|---:|---:|---:|
| `merge/empty` | 1 -> 1 | 0 -> 0 | 2 -> 2 | 10 -> 10 | 16 -> 16 |
| `merge/one` | 3 -> 2 | 0 -> 0 | 8 -> 7 | 104 -> 98 | 236 -> 230 |
| `merge/page` | 130 -> 2 | 0 -> 0 | 516 -> 388 | 12,130 -> 11,274 | 28,498 -> 27,642 |
| `merge/paged` | 647 -> 7 | 1 -> 1 | 2,578 -> 1,938 | 41,290 -> 36,982 | 123,414 -> 119,106 |
| `merge/duplicates` | 647 -> 7 | 0 -> 0 | 2,578 -> 1,938 | 18,762 -> 14,454 | 100,886 -> 96,578 |
| `merge/spaces` | 647 -> 7 | 1 -> 1 | 2,578 -> 1,938 | 41,290 -> 36,982 | 126,614 -> 122,306 |
| `events/empty` | 2 -> 0 | 0 -> 0 | 2 -> 0 | 363 -> 0 | 363 -> 0 |
| `events/small` | 91 -> 61 | 3 -> 1 | 91 -> 61 | 5,162 -> 2,792 | 5,162 -> 2,792 |
| `events/events` | 1,707 -> 807 | 23 -> 17 | 1,707 -> 807 | 124,926 -> 34,970 | 124,926 -> 34,970 |
| `events/comments` | 1,575 -> 675 | 3 -> 1 | 1,575 -> 675 | 1,133,774 -> 29,034 | 1,133,774 -> 29,034 |
| `events/ineligible` | 518 -> 0 | 0 -> 0 | 518 -> 0 | 46,628 -> 0 | 46,628 -> 0 |
| `timestamp/milliseconds` | 0 -> 0 | 0 -> 0 | 0 -> 0 | 0 -> 0 | 0 -> 0 |
| `timestamp/seconds` | 0 -> 0 | 0 -> 0 | 0 -> 0 | 0 -> 0 | 0 -> 0 |
| `timestamp/offset` | 0 -> 0 | 0 -> 0 | 0 -> 0 | 0 -> 0 | 0 -> 0 |
| `timestamp/nanoseconds` | 0 -> 0 | 0 -> 0 | 0 -> 0 | 0 -> 0 | 0 -> 0 |
| `timestamp/leap` | 0 -> 0 | 0 -> 0 | 0 -> 0 | 0 -> 0 | 0 -> 0 |
| `timestamp/invalid` | 0 -> 0 | 0 -> 0 | 0 -> 0 | 0 -> 0 | 0 -> 0 |
| `bulk/512` | 0 -> 0 | 0 -> 0 | 0 -> 0 | 0 -> 0 | 0 -> 0 |

The paged case removes all 640 user-ID copies (647 -> 7 allocations); remaining
allocations belong to the result and identity collections. In the event case,
allocations fall from 1,707 to 807, reallocations from 23 to 17, and requested
bytes from 124,926 to 34,970 (72.0% less). With 4-KiB comments, byte traffic falls
from 1,133,774 to 29,034 (97.4% less). Empty/ineligible responses have zero heap
churn. Timestamp conversion already allocated nothing and remains allocation-free.

## Behavior and validation

- `cargo test -p deadsync-online -p deadsync-score --locked`: 548 tests passed;
  15 manual benchmark tests ignored.
- `cargo test -p deadsync-online --lib online_results --release --locked -- --nocapture`:
  all five new regression tests passed; one manual benchmark ignored.
- Differential page tests cover first-page duplicates, later-page deduplication,
  empty/blank IDs, Unicode whitespace, case-sensitive IDs, API/context flags,
  ordering and special score values.
- Event comparisons check the complete output across event eligibility,
  small/large/empty leaderboards, absent progress, Unicode and blank names,
  score-added handling, extreme deltas, grouped rewards, quests and achievements.
  Both borrowing and consuming score APIs match the old output. Pointer checks
  verify that consuming conversion retains the original leaderboard buffers.
- Timestamp comparisons cover 92,400 calendar/time/fraction combinations,
  leap-year boundaries including years 0000 and 9999, leap seconds, offsets,
  extended fractional precision, noncanonical casing/separators, whitespace,
  every ASCII byte substitution in a canonical timestamp, and 4,096 generated
  ASCII/Unicode strings.
- Allocation assertions verify zero-cost empty/ineligible event results,
  unchanged allocation-free timestamp parsing and reduced complete-operation
  churn for event preparation and page merging. The first allocation test used
  unequal input capacities; both variants now receive identically cloned
  fixtures, including during measurement.
- Source hashes were unchanged during the final measurements.
- `cargo check --locked`: passed, including the root application and its callers.
- `cargo clippy -p deadsync-online -p deadsync-score --lib --locked -- -D clippy::perf`:
  passed; existing non-performance warnings remain.
- Scoped `rustfmt --check` and `git diff --check`: passed.

To reproduce one release comparison:

```powershell
cargo test -p deadsync-online --lib --release --locked online_results::benchmark_online_results -- --ignored --nocapture --test-threads=1
```

Set `$env:DEADSYNC_PERF_REVERSE = "1"` for new-first order; leave it unset for
old-first order. The recorded five-run comparison additionally pinned the
benchmark process to logical CPU 4. Run benchmarks alone for comparable timing.
