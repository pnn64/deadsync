# Owned download rows, purchase projection, and catalog completion — 0.5.1709

Original: `5d780220d` (`0.5.1708`). Current: `0.5.1709`.

## Changes

1. Move string IDs from parsed download rows, and reuse their clean song/detail
   buffers. `String → borrowed cleanup → new String` becomes an ownership move
   when the cell already has clean text. Trailing spaces truncate the reused
   buffer. Capacity is shrunk when needed so retained strings never exceed the
   original footprint. Dirty cells keep the existing cleanup state machine;
   non-string IDs keep the existing scalar conversion.
2. Project purchase responses directly into their required name-only result.
   The removed full-download helper used to format/copy an item ID, normalize a
   URL, and construct a complete record whose caller immediately discarded
   everything except the name. URL eligibility and alias/name rules remain;
   unused normalization and fields are eliminated. Existing object tests now
   compare the observable purchase name/eligibility contract. Full retained
   download fields are covered separately by the download-row tests.
3. Finish catalog requests with `Arc::make_mut` instead of rebuilding a cloned
   snapshot. Unique snapshots reuse install history, including its message strings;
   immutable readers still receive copy-on-write isolation. Stale-generation
   rejection, revision wrapping, success/error phases, catalogs, messages, and
   logging remain unchanged. Install capacity is trimmed to match the original
   clone's footprint.

## Method

Windows x86_64, Intel Xeon E5-2696 v4 (22 cores / 44 logical processors),
Rust/Cargo 1.98.1, release optimization level 3 with full LTO. Both variants run
in the same test executable, through opaque function pointers at measured entry
points. Frozen function bodies and original inline attributes match `5d780220d`
apart from test visibility. Shared helpers not changed by this pass remain the
same in both variants. Allocation counting and sampling use tests/support/perf.rs.

Six serial rounds alternate original-first/current-first order. Every measure
has three warmups and seven samples. CPU cycles use Windows QueryThreadCycleTime
on the calling thread. Tables report the median of six paired savings/throughput
ratios and their full observed range, not a confidence interval. Positive savings
mean fewer cycles. All 26 workloads have both variants in all six rounds: 312
primary measurements. Six additional alternating rounds of all workloads add
312 measurements, for 624 total. No compilation or other test suite runs
concurrently with timing.

Download-row and purchase tests measure complete JSON parsing, conversion, and
result destruction. Their input JSON is built outside timing. Catalog completion
uses a fresh fixture per call: fixture construction/restoration/destruction is
outside timing and allocation accounting. The actual function, including its
runtime mutex, is measured. Its helper includes per-operation clock/counter
overhead equally in both variants; no estimated overhead is subtracted. Thus its
cycle savings are conservative. Its throughput is request completions per second,
not network throughput or frame rates.

Timing and allocation counting are separate. Counts include measured result frees
for JSON operations; catalog fixture/result frees occur outside the counted
scope. Requested bytes mean allocation traffic, not RSS or peak live memory.
Retained capacity is checked separately against the original implementation.

## Complete download-row parsing

| Workload | CPU cycles saved, median (range) | Throughput | Allocations + reallocations | Requested bytes |
| --- | ---: | ---: | ---: | ---: |
| empty | -1.10% (-3.70..7.95%) | 0.99x | 0 + 0 → 0 + 0 | 0 → 0 |
| one | 16.44% (-3.16..23.10%) | 1.20x | 10 + 5 → 7 + 5 | 1330 → 1307 |
| sixteen | 12.40% (7.23..14.45%) | 1.14x | 140 + 27 → 92 + 27 | 9340 → 8972 |
| many | 25.28% (21.22..28.18%) | 1.34x | 1111 + 179 → 727 + 179 | 79046 → 76102 |
| numeric | 10.51% (-5.17..13.07%) | 1.12x | 983 + 179 → 727 + 179 | 79302 → 76742 |
| html-numeric | -2.47% (-4.29..0.39%) | 0.98x | 983 + 179 → 983 + 179 | 80582 → 80582 |
| trailing | 2.16% (-0.40..4.64%) | 1.02x | 1111 + 179 → 855 + 307 | 77126 → 76742 |
| long | 2.79% (-14.89..19.00%) | 1.05x | 1111 + 179 → 727 + 179 | 320198 → 196678 |

Each sample parses and destroys 256 responses. Fixtures contain 0/1/16/128 rows,
plain or dirty text, string/numeric IDs, trailing spaces, and long clean names
and IDs. URLs include relative and escaped HTTPS paths. Throughput counts returned
downloads; the empty case counts complete responses. HTML/numeric rows exercise
the fallback with no reusable clean text. Trailing text also checks shrink work.

## Complete purchase-response parsing

| Workload | CPU cycles saved, median (range) | Throughput | Allocations + reallocations | Requested bytes |
| --- | ---: | ---: | ---: | ---: |
| missing | 10.06% (4.08..16.38%) | 1.11x | 0 + 0 → 0 + 0 | 0 → 0 |
| rejected | -2.00% (-5.30..1.52%) | 0.98x | 11 + 0 → 11 + 0 | 759 → 759 |
| relative | 25.76% (24.31..29.19%) | 1.35x | 15 + 1 → 12 + 0 | 904 → 793 |
| https | 12.83% (6.45..14.06%) | 1.15x | 14 + 0 → 12 + 0 | 816 → 784 |
| escaped | 21.57% (18.52..23.26%) | 1.27x | 15 + 4 → 13 + 2 | 914 → 843 |
| html-numeric | 25.75% (18.54..28.34%) | 1.35x | 14 + 1 → 11 + 0 | 896 → 790 |
| large-id | 19.89% (14.65..22.50%) | 1.25x | 15 + 1 → 12 + 0 | 9050 → 4856 |
| long-url | 49.30% (47.76..50.72%) | 1.97x | 14 + 0 → 12 + 0 | 3122 → 1937 |

Each sample parses and destroys 4,096 responses. Fixtures cover absent/rejected
downloads, relative/HTTPS/escaped URLs, HTML names, numeric IDs, an unused 4 KiB ID,
and a long Unicode URL. Both versions return the same purchase name and errors;
the original still builds and then destroys its unused normalized URL and ID.

## Catalog completion

| Workload | CPU cycles saved, median (range) | Throughput | Allocations + reallocations | Requested bytes |
| --- | ---: | ---: | ---: | ---: |
| empty-success | 20.41% (17.63..25.58%) | 2.05x | 3 + 0 → 1 + 0 | 1168 → 368 |
| success-unique | 50.06% (48.80..51.79%) | 5.45x | 12 + 0 → 1 + 0 | 2720 → 368 |
| success-shared | -0.13% (-6.79..2.71%) | 1.01x | 12 + 0 → 12 + 0 | 2720 → 2720 |
| failure-unique | 45.94% (36.78..48.22%) | 3.53x | 12 + 1 → 1 + 1 | 2451 → 99 |
| failure-shared | 1.84% (-1.78..3.55%) | 1.02x | 12 + 1 → 12 + 1 | 2451 → 2451 |
| many-success | 87.69% (87.40..88.33%) | 29.32x | 68 + 0 → 1 + 0 | 13638 → 368 |
| many-failure | 86.05% (85.11..86.52%) | 19.00x | 68 + 1 → 1 + 1 | 13369 → 99 |
| oversized-success | 45.85% (43.63..47.61%) | 3.65x | 12 + 0 → 1 + 1 | 2720 → 816 |
| oversized-failure | 42.82% (36.23..43.89%) | 2.85x | 12 + 1 → 1 + 2 | 2451 → 547 |
| stale | -0.71% (-4.07..2.87%) | 1.01x | 0 + 0 → 0 + 0 | 0 → 0 |

Each sample completes 2,048 requests. History fixtures contain 0/8/64 installs,
each with a 128-byte message suffix, plus an old catalog status message. Shared
cases hold an immutable snapshot reader. Oversized cases start with 4,096 install
slots. Success replaces the catalog with two packs; failure formats a catalog
error. Stale requests retain every existing runtime field and discard their input.
All global fixture changes are serialized and restored after each operation.

## Control follow-up and limits

Small negative control medians were rechecked with six additional alternating
rounds of the same executable. Every follow-up result is retained in the separate
CSV, including negative values.

| Workload | Primary CPU savings, median (range) | Follow-up CPU savings, median (range) |
| --- | ---: | ---: |
| download-rows/empty | -1.10% (-3.70..7.95%) | 0.61% (-8.94..9.09%) |
| download-rows/html-numeric | -2.47% (-4.29..0.39%) | 0.91% (-2.93..8.36%) |
| download-rows/trailing | 2.16% (-0.40..4.64%) | 3.19% (-3.32..6.61%) |
| download-rows/long | 2.79% (-14.89..19.00%) | 5.42% (-10.85..21.53%) |
| purchase/rejected | -2.00% (-5.30..1.52%) | -1.59% (-6.98..3.33%) |
| catalog-completion/success-shared | -0.13% (-6.79..2.71%) | -1.50% (-2.89..7.78%) |
| catalog-completion/failure-shared | 1.84% (-1.78..3.55%) | -3.31% (-6.34..3.40%) |
| catalog-completion/stale | -0.71% (-4.07..2.87%) | -0.44% (-2.25..1.91%) |

HTML/numeric rows change from -2.47% to +0.91%; empty rows and shared failure
also change sign. Rejected purchases, shared success, and stale completion retain
small negative medians with ranges crossing zero in both runs. Their allocation
counts and requested bytes are unchanged. No CPU gain is claimed for these
controls, and the negative medians remain visible above. The targeted allocation
reductions and CPU gains in plain rows, accepted purchases, and unique completion
hold in both runs. No behavioral difference or retained-capacity increase was
observed.

Trailing text reduces allocations from 1,111 to 855 but adds 128 shrink
reallocations: total allocation/reallocation calls still fall from 1,290 to 1,162.
Long rows save 384 allocations and 123,520 requested bytes (38.58%); their CPU
range crosses zero, so their CPU timing improvement is uncertain. Shared
completion preserves the required history copies and shows no allocation gain.

## Behavioral validation

- All 304 online tests pass in debug and release with `--test-threads=1`; fourteen
  tests remain ignored, including manual benchmarks and the live download test.
  Serial execution avoids the unchanged local HTTP socket test's previously
  documented Windows `WouldBlock` race.
- Six new regression/allocation tests compare complete download/purchase fields,
  returned errors, rejected URLs, all alias priorities, differently cased duplicate
  keys, string/numeric/boolean/null IDs and names, missing fields, HTML/entities,
  Unicode whitespace, pipe text, decoded strings, and retained capacities.
- Clean owned-cell checks verify exact text, compact and 4,096-byte capacities,
  pointer reuse for already-clean strings, and strictly reduced allocation churn.
  Complete 128-row responses and accepted purchase responses reduce churn too.
- Catalog checks cover 0/1/8/64 install records, compact/doubled/4,096-slot
  capacity, unique/shared snapshots, weak readers, success/error/stale results,
  immutable observers, catalog identity, revision wrap, generation, and ready
  directories. Unique completion strictly reduces allocation churn.
- Architecture: 141 pass and the exact same 12 failures as `0.5.1708`, with no
  new failure. Their parent reproduction is documented in
  [the 0.5.1699 report](actor-song-timing-0.5.1699.md#behavioral-validation).
- Clippy completes with the existing online warning counts: 11 library / 21
  library-test. No warning points to changed code or new tests. Changed Rust
  files pass rustfmt checks and git diff --check passes. Cargo.toml and Cargo.lock
  contain only the exact 0.5.1708 → 0.5.1709 version changes.

## Reproduction

```powershell
cargo test -p deadsync-online -- --test-threads=1
cargo test --release -p deadsync-online -- --test-threads=1
cargo clippy -p deadsync-online --all-targets
cargo test --release -p deadsync-online srpg_shop::owned_download_rows_perf::owned_download_rows_benchmark -- --exact --ignored --nocapture --test-threads=1
cargo test --release -p deadsync-online srpg_shop::purchase_projection_perf::purchase_projection_benchmark -- --exact --ignored --nocapture --test-threads=1
cargo test --release -p deadsync-online stepmaniaonline::catalog_completion_perf::catalog_completion_benchmark -- --exact --ignored --nocapture --test-threads=1
```

Run each benchmark serially for six rounds, alternating DEADSYNC_BENCH_NEW_FIRST
between unset and 1. Recorded runs invoke the already-built test executable to
keep binaries and feature graphs identical between variants and rounds.

[Primary raw measurements](downloads-purchases-catalog-0.5.1709.csv) ·
[Follow-up measurements](downloads-purchases-catalog-0.5.1709-controls.csv).
