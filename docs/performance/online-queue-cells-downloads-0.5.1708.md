# Install queuing, shop cell cleanup, and JSON download text — 0.5.1708

Original: `cafbd9e2a` (`0.5.1707`). Current: `0.5.1708`.

## Changes

1. Check install rejections before copying a snapshot, and use `Arc::make_mut`
   for accepted changes. Unique snapshots reuse their install records and
   messages; snapshots held by readers still copy on write. Retry and eviction
   order, error text, catalog identity, ready-song directories, and debug logging
   are preserved. Capacity is trimmed when reuse would retain more install
   storage than the original rebuild.
2. Borrow shop cells that already have clean text. A validated prefix is copied
   in bulk only when its suffix needs HTML/entity/whitespace cleanup. Owned
   catalog/download fields still receive owned strings. Effects can replace pipe
   separators directly on borrowed text, removing the temporary cleaned String.
   The existing entity and tag state machine remains responsible for dirty text.
3. Return borrowed JSON values from text lookup and convert only at use.
   Download URLs and names no
   longer become temporary Strings just before `.zip` validation, normalization,
   and cleanup. Retained IDs still own their strings. Numeric text still gets
   formatted once. Nonempty scalar validation needs no formatting or owned
   result wrapper. Alias priority, case-insensitive key lookup, empty-value
   fallback, scalar conversion, and all retained field ownership are preserved.
   The URL normalizer remains unchanged.

## Method

Windows x86_64, Intel Xeon E5-2696 v4 (22 cores / 44 logical processors),
Rust/Cargo 1.98.1, release optimization level 3 with full LTO. Both variants run
in the same release test executable with opaque function pointers at measured
entry points. Frozen original function bodies and inline attributes match
`cafbd9e2a`, apart from test visibility. The unchanged scoped System allocator
and sampling helpers are in tests/support/perf.rs.

Six serial rounds alternate original-first/current-first order. Each
measurement has three warmups and seven samples. CPU cycles use Windows
QueryThreadCycleTime on the calling thread. Tables report the median of six
paired savings/throughput ratios, and the full paired range, not a confidence
interval. Positive savings mean fewer CPU cycles. All 49 workloads have both
variants in every round: 588 measurements. Six additional alternating rounds of
the 23 shop-cell/catalog workloads add 276 measurements, for 864 in total.
Compilation and other test suites
do not run concurrently with benchmarks.

Cell and JSON download output destruction is included. Install operations require a fresh
mutable fixture for each call, so setup and fixture destruction are excluded;
returned errors are dropped during the measured operation. That helper includes
per-operation clock and cycle-counter overhead equally in both variants. Its
CPU percentages are conservative and its throughput timings include the wall
clock reads; they are not whole-install or download throughput measurements.
Timing and allocation tracking are separate. Allocation counts report work
within the scope, with reallocations separate; requested bytes are allocation
traffic, not RSS or peak live memory. Install results remain live after the
counted operation, so their fixture frees appear outside the CSV scope. Separate
allocation regression tests include fixture destruction.

## Install queue

| Workload | CPU cycles saved, median (range) | Throughput | Allocations + reallocations | Requested bytes |
| --- | ---: | ---: | ---: | ---: |
| empty-unique | 19.61% (18.42..21.10%) | 2.13x | 4 + 0 → 2 + 0 | 376 → 256 |
| append-unique | 48.30% (45.01..49.89%) | 4.59x | 12 + 1 → 1 + 1 | 2600 → 928 |
| append-shared | 3.42% (-1.96..9.97%) | 1.04x | 12 + 1 → 12 + 1 | 2600 → 2600 |
| retry-unique | 50.05% (47.62..51.48%) | 6.50x | 12 + 0 → 1 + 0 | 1704 → 32 |
| retry-shared | 1.81% (-2.80..3.49%) | 1.04x | 12 + 0 → 12 + 0 | 1704 → 1704 |
| reject-queued | 88.41% (85.37..88.89%) | 34.45x | 67 + 0 → 1 + 0 | 12536 → 42 |
| reject-installed | 88.15% (86.71..88.61%) | 34.05x | 67 + 0 → 1 + 0 | 12570 → 76 |
| reject-full | 87.87% (86.11..88.93%) | 31.48x | 67 + 0 → 1 + 0 | 12528 → 34 |
| evict-unique | 86.63% (86.48..88.06%) | 22.77x | 68 + 0 → 1 + 0 | 12622 → 32 |
| evict-shared | 1.63% (-1.64..3.32%) | 1.03x | 68 + 0 → 68 + 0 | 12622 → 12622 |
| oversized-append | 51.53% (47.42..54.49%) | 5.00x | 12 + 1 → 1 + 1 | 2600 → 536 |
| oversized-retry | 48.22% (43.23..51.10%) | 4.15x | 12 + 0 → 1 + 1 | 1704 → 480 |

Each sample performs 4,096 operations. Append and retry fixtures have eight
history records, each with a 128-byte message suffix. Full/rejection/eviction
fixtures have 64 records. Shared cases keep an immutable snapshot reader alive;
oversized cases start with 4,096 slots of install capacity. Unique retries remove
the historical message copies, and duplicate/full rejections avoid them entirely.
Shared accepted mutations retain the original allocations and required copies.
Oversized retries add one shrink reallocation while removing eleven allocations
and retaining no more install capacity than the original.

## Shop cells and complete catalogs

| Workload | CPU cycles saved, median (range) | Throughput | Allocations + reallocations | Requested bytes |
| --- | ---: | ---: | ---: | ---: |
| clean-empty | 5.63% (-3.11..25.41%) | 1.05x | 0 + 0 → 0 + 0 | 0 → 0 |
| effect-empty | 27.19% (15.12..34.49%) | 1.41x | 0 + 0 → 0 + 0 | 0 → 0 |
| clean-plain | 54.04% (48.52..59.85%) | 2.14x | 1 + 0 → 1 + 0 | 23 → 23 |
| effect-plain | 64.11% (58.98..66.69%) | 2.79x | 2 + 0 → 1 + 0 | 46 → 23 |
| clean-pipe | 52.42% (50.80..53.97%) | 2.12x | 1 + 0 → 1 + 0 | 34 → 34 |
| effect-pipe | 47.03% (45.36..55.20%) | 1.91x | 2 + 1 → 1 + 1 | 136 → 102 |
| clean-html | 1.48% (-9.07..7.14%) | 1.01x | 1 + 0 → 1 + 0 | 54 → 54 |
| effect-html | 2.75% (1.97..7.75%) | 1.03x | 2 + 1 → 2 + 1 | 162 → 162 |
| clean-entities | -1.85% (-7.29..3.05%) | 0.99x | 1 + 0 → 1 + 0 | 48 → 48 |
| effect-entities | -1.88% (-4.23..5.01%) | 0.98x | 2 + 0 → 2 + 0 | 63 → 63 |
| clean-whitespace | 1.54% (-6.20..5.13%) | 1.01x | 1 + 0 → 1 + 0 | 42 → 42 |
| effect-whitespace | -5.50% (-14.98..1.98%) | 0.95x | 2 + 1 → 2 + 1 | 150 → 150 |
| clean-unicode | 50.26% (48.18..54.24%) | 2.03x | 1 + 0 → 1 + 0 | 27 → 27 |
| effect-unicode | 46.78% (38.52..48.54%) | 1.89x | 2 + 1 → 1 + 1 | 108 → 81 |
| clean-late-html | 78.18% (77.20..78.93%) | 4.56x | 1 + 0 → 1 + 0 | 1223 → 1223 |
| effect-late-html | 76.08% (73.85..77.09%) | 4.17x | 2 + 0 → 2 + 0 | 2442 → 2442 |
| clean-late-space | 77.68% (76.51..78.00%) | 4.49x | 1 + 0 → 1 + 0 | 1220 → 1220 |
| effect-late-space | 76.85% (75.51..77.75%) | 4.30x | 2 + 0 → 2 + 0 | 2439 → 2439 |
| clean-long | 77.77% (76.50..79.42%) | 4.51x | 1 + 0 → 1 + 0 | 2435 → 2435 |
| effect-long | 76.69% (76.05..77.03%) | 4.29x | 2 + 1 → 1 + 1 | 9740 → 7305 |
| catalog-plain | 15.02% (-3.27..33.76%) | 1.18x | 2567 + 389 → 2439 + 389 | 343028 → 338676 |
| catalog-html | -0.84% (-11.00..6.45%) | 0.99x | 2567 + 389 → 2567 + 389 | 350708 → 350708 |
| catalog-censored | -0.03% (-10.59..3.05%) | 1.00x | 2439 + 261 → 2439 + 261 | 334068 → 334068 |

Cell samples perform 16,384 operations. Fixtures cover empty/plain/pipe text,
HTML tags, entity-heavy text, Unicode, repeated whitespace, long clean text, and
late HTML or repeated spaces after a long clean prefix. The `clean-*` cases
convert the result into a String to exercise callers requiring ownership;
`effect-*` cases include pipe replacement and destruction. Catalog samples parse
and destroy 64 complete 128-item catalogs; throughput counts catalog items.
Censored rows bypass cell cleanup. Plain catalogs remove one temporary effect
allocation per item. Dirty cells and censored catalogs are controls, with
unchanged allocation traffic.

Small negative control medians were checked with six additional alternating
rounds using the same binary. All follow-up measurements are retained in the
separate controls CSV, including negative values.

| Workload | Primary CPU savings, median (range) | Follow-up CPU savings, median (range) |
| --- | ---: | ---: |
| clean-empty | 5.63% (-3.11..25.41%) | 5.04% (-3.94..11.08%) |
| clean-html | 1.48% (-9.07..7.14%) | -3.15% (-6.41..3.88%) |
| effect-html | 2.75% (1.97..7.75%) | 2.58% (-7.72..14.39%) |
| clean-entities | -1.85% (-7.29..3.05%) | -0.83% (-13.10..10.82%) |
| effect-entities | -1.88% (-4.23..5.01%) | 1.83% (-9.02..7.97%) |
| clean-whitespace | 1.54% (-6.20..5.13%) | 2.58% (-15.68..6.25%) |
| effect-whitespace | -5.50% (-14.98..1.98%) | -0.66% (-17.33..18.48%) |
| catalog-plain | 15.02% (-3.27..33.76%) | 9.79% (-0.16..26.72%) |
| catalog-html | -0.84% (-11.00..6.45%) | -0.87% (-6.25..3.41%) |
| catalog-censored | -0.03% (-10.59..3.05%) | 1.09% (-1.70..8.86%) |

The whitespace-effect median changes from -5.50% to -0.66%; HTML cleanup changes
from +1.48% to -3.15%, and censored catalogs change from -0.03% to +1.09%.
Entity-cleanup and HTML-catalog medians remain slightly negative with ranges
crossing zero in both runs. These variable controls establish no CPU improvement
or stable CPU regression; their allocation traffic is unchanged. Plain catalog
allocation reductions and the clean-text/prefix CPU improvements hold in both
runs. No behavioral difference or retained-capacity increase was observed.

## JSON download text and complete download objects

| Workload | CPU cycles saved, median (range) | Throughput | Allocations + reallocations | Requested bytes |
| --- | ---: | ---: | ---: | ---: |
| missing | 9.11% (6.85..19.05%) | 1.10x | 0 + 0 → 0 + 0 | 0 → 0 |
| empty | 14.15% (5.06..25.75%) | 1.17x | 0 + 0 → 0 + 0 | 0 → 0 |
| rejected | 45.77% (43.92..53.03%) | 1.85x | 1 + 0 → 0 + 0 | 18 → 0 |
| relative | 24.26% (22.19..26.77%) | 1.32x | 6 + 1 → 4 + 1 | 157 → 125 |
| rooted | 23.26% (20.55..30.50%) | 1.31x | 6 + 1 → 4 + 1 | 139 → 116 |
| https | 26.95% (21.95..33.83%) | 1.38x | 5 + 0 → 3 + 0 | 69 → 36 |
| escaped | 21.22% (15.29..25.63%) | 1.26x | 5 + 2 → 3 + 2 | 111 → 75 |
| html | 11.69% (8.47..20.17%) | 1.13x | 6 + 1 → 4 + 1 | 172 → 135 |
| fallback | 14.43% (5.14..17.54%) | 1.16x | 6 + 1 → 5 + 1 | 121 → 113 |
| default-name | 10.20% (7.03..15.37%) | 1.12x | 4 + 1 → 3 + 1 | 119 → 111 |
| long | 72.03% (68.48..72.63%) | 3.57x | 5 + 0 → 3 + 0 | 4031 → 2016 |
| lookup-string | 84.28% (81.86..87.39%) | 6.45x | 1 + 0 → 0 + 0 | 8 → 0 |
| lookup-number | 7.41% (0.52..11.90%) | 1.08x | 1 + 0 → 1 + 0 | 8 → 8 |
| lookup-missing | 50.00% (47.44..50.43%) | 2.01x | 0 + 0 → 0 + 0 | 0 → 0 |

Each sample performs 16,384 lookups or complete object conversions, including
output destruction. Lookup-string measures the third optimization directly;
lookup-number and lookup-missing are controls. Complete download objects include
all text lookups, URL validation, name cleanup, URL normalization, retained IDs,
and destruction. Cases cover missing/empty/rejected URLs, relative/rooted/HTTPS
paths, escaped URLs, HTML names, alias/scalar fallbacks, a default name, and long
Unicode paths/names. Their CPU improvements include the cleaner from change 2;
the removed temporary allocations come from borrowing the URL and name in
change 3. Rejected URLs also stop copying text that will immediately be discarded.
Every retained field has no greater capacity than its original counterpart.

An attempted URL-normalizer optimization was rejected: conditional `contains`
followed by `replace` lost about 4% on escaped controls, a `split` implementation
lost about 32% on long unescaped HTTPS URLs, and a backslash-first implementation
lost 5.54% on dense escapes and 7.69% on escaped HTTPS URLs. The final third
optimization removes the copies before the unchanged normalizer instead.

## Behavioral validation

- All 298 online tests pass in both debug and release; eleven tests remain
  ignored, including manual benchmarks and the live download test. Eight new
  regression/allocation tests accompany these changes.
  Parallel debug runs hit `WouldBlock` in the unchanged local HTTP
  encoding test's socket read. That test passes in isolation, and the full debug
  suite passes with `--test-threads=1`.
- Install equivalence checks cover all five phases, counts 0/1/8/64/65, compact
  and oversized capacity, first/last/absent IDs, shared and unique snapshots,
  weak readers, first-terminal eviction, returned errors, catalog Arc identity,
  generation, ready directories, and retained install capacity.
- Cell equivalence checks cover all 13,824 triples of 24 tokens, including
  nested entities, unmatched tags, Unicode whitespace, unknown entities,
  apostrophes/quotes, and pipe effects. Already-clean borrowed cells allocate
  nothing. Complete catalogs compare all fields and retained string capacities
  across row counts 0/1/16/128, HTML/plain effects, censoring, and balance values.
- JSON text equivalence checks cover all alias priorities, differently cased
  duplicate keys, empty/missing values, strings, numbers, booleans, null, arrays,
  and objects. Borrowed strings point into the source JSON and allocate nothing.
  Complete download equivalence checks compare every field and retained capacity
  across all aliases, rejected URLs, names with HTML/entities/Unicode whitespace,
  numeric/empty IDs, relative/rooted/scheme URLs, escaped/double backslashes,
  repeated dots, NULs, queries, and fragments. Accepted and rejected downloads
  strictly reduce allocation churn in regression fixtures.
- Architecture suite: 141 pass and the exact same 12 failures as the previous
  pass, with no new failure. Their parent reproduction is documented in
  [the 0.5.1699 report](actor-song-timing-0.5.1699.md#behavioral-validation).
- Clippy completes with the existing online warning counts, 11 library / 21
  library-test; no warning points to changed code or new tests. Changed Rust
  files pass rustfmt checks; git diff --check passes. Cargo.toml and Cargo.lock
  contain only the exact 0.5.1707 → 0.5.1708 version changes.

## Reproduction

```powershell
cargo test -p deadsync-online -- --test-threads=1
cargo test --release -p deadsync-online
cargo clippy -p deadsync-online --all-targets
cargo test --release -p deadsync-online stepmaniaonline::install_queue_perf::install_queue_benchmark -- --exact --ignored --nocapture --test-threads=1
cargo test --release -p deadsync-online srpg_shop::shop_effects_perf::shop_effects_benchmark -- --exact --ignored --nocapture --test-threads=1
cargo test --release -p deadsync-online srpg_shop::shop_objects_perf::shop_objects_benchmark -- --exact --ignored --nocapture --test-threads=1
```

Run each benchmark serially for six rounds, alternating DEADSYNC_BENCH_NEW_FIRST
between unset and 1. Recorded runs invoke the already-built test executable
directly to keep binaries and feature graphs identical between rounds.

[Primary raw measurements](online-queue-cells-downloads-0.5.1708.csv) ·
[Shop-cell control measurements](online-queue-cells-downloads-0.5.1708-controls.csv).
