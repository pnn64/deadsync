# Owned URLs, reused title lines, and pack indexing — 0.5.1710

Original: `e314fc18d` (`0.5.1709`). Current: `0.5.1710`.

## Changes

1. Consume each already-owned download URL instead of copying it through a
   borrowed normalizer. Unescaped absolute URLs retain their allocation;
   relative URLs skip an intermediate copy. URLs containing backslashes use
   their underlying byte vector without allocation or copying: compact each
   ASCII backslash/slash pair in place, then validate UTF-8 when returning the
   `String`.
   The vector access enables safe in-place mutation; no unsafe code is added.
   Absolute buffers shrink when needed so retained storage never exceeds the
   original. Escaped and ordinary URLs both avoid the original replacement copy.
2. Reuse a single `String` for the first 64 simfile lines instead of allocating
   and freeing a fresh line on every iteration. Stop at the same first title,
   EOF, invalid UTF-8, or I/O error. LF/CRLF stripping matches `BufRead::lines`,
   including the final line without a newline.
3. Copy ASCII pack names and metadata directly into the search index, then
   lowercase each copied ASCII segment in bulk. This removes character decoding
   and Unicode case-iterator work for ASCII fields. Non-ASCII fields still use the original
   character mapping, preserving case expansions and non-contextual Greek sigma.
   Name boundaries, separators, IDs, and allocation capacity remain identical.

## Method

Windows x86_64, Intel Xeon E5-2696 v4 (22 cores / 44 logical processors),
Rust/Cargo 1.98.1, release optimization level 3 with full LTO. Both variants run
in the same executable via opaque function pointers. Original function bodies
are frozen from `e314fc18d`; test visibility/names differ only where needed to
wire the old pack constructor to the old index. Older performance baselines
retain a frozen borrowed URL normalizer so this pass cannot alter their reference.

Six serial rounds alternate original-first/current-first order. Every measurement
has three warmups and seven timed samples. CPU cycles are Windows
`QueryThreadCycleTime` on the calling thread. Tables show the median of six paired
savings/throughput ratios and the full observed range, not a confidence interval.
All 23 workloads have both variants in all six rounds (276 measurements).
Another six alternating rounds repeat all workloads (276 measurements), for
552 measurements total. No compilation or other test suite runs during timing.

Timing and scoped System allocator counting are separate, using
`tests/support/perf.rs`. Counts include result destruction. Requested bytes
measure allocation traffic, not RSS or peak live memory. Byte tables use medians;
temporary filesystem path lengths can vary with process IDs. Allocation/free
call counts are constant across rounds. URL retained capacities
are checked against the original; pack index capacities match exactly.

## Complete download response parsing

| Workload | CPU cycles saved, median (range) | Throughput | Allocations + reallocations | Requested bytes, median |
| --- | ---: | ---: | ---: | ---: |
| empty | -0.03% (-1.60..24.91%) | 1.00x | 0 + 0 → 0 + 0 | 0 → 0 |
| rejected | 1.69% (-19.07..10.75%) | 1.02x | 513 + 6 → 513 + 6 | 61610 → 61610 |
| one-https | 7.71% (-0.32..15.31%) | 1.11x | 6 + 1 → 5 + 1 | 1207 → 1178 |
| https | 12.71% (8.60..16.18%) | 1.15x | 641 + 6 → 513 + 6 | 68010 → 64298 |
| relative | 9.10% (5.22..10.86%) | 1.10x | 769 + 134 → 641 + 134 | 74154 → 73130 |
| rooted | 10.03% (7.04..12.19%) | 1.11x | 769 + 134 → 641 + 134 | 74410 → 73258 |
| escaped-https | 18.42% (17.33..20.97%) | 1.23x | 642 + 264 → 514 + 136 | 73442 → 68450 |
| dense-escapes | 38.40% (38.04..40.52%) | 1.62x | 770 + 1036 → 642 + 268 | 291106 → 161058 |
| long-https | 32.09% (29.49..34.63%) | 1.47x | 641 + 6 → 513 + 6 | 361898 → 211242 |
| unicode-escapes | 19.74% (19.21..21.08%) | 1.25x | 642 + 1038 → 514 + 142 | 902306 → 415522 |

Fixtures are built outside timing. Each sample parses and destroys 512 JSON
responses containing 0/1/128 rows. Useful throughput counts accepted rows, except
empty and one-row controls use one operation; rejected rows count rows examined.
These results include deserialization, eligibility checks, conversion, and frees.
They measure parsing throughput, not network speed.

## Simfile title scanning

| Workload | CPU cycles saved, median (range) | Throughput | Allocations + reallocations | Requested bytes, median |
| --- | ---: | ---: | ---: | ---: |
| first | 0.46% (-1.90..5.54%) | 1.00x | 11 + 2 → 11 + 2 | 9075 → 9075 |
| middle | 0.17% (-0.93..2.28%) | 1.00x | 42 + 2 → 11 + 2 | 9602 → 9078 |
| last | 5.18% (1.47..5.45%) | 1.05x | 74 + 2 → 11 + 2 | 10146 → 9078 |
| missing | 2.48% (-0.21..4.97%) | 1.02x | 72 + 2 → 9 + 2 | 10025 → 8954 |
| long-unicode | 3.85% (1.76..5.28%) | 1.04x | 74 + 11 → 11 + 3 | 84389 → 10239 |

Each fixture is a real directory containing one `.ssc` file. Fixture writes and
cleanup are outside timing; each sample runs 128 complete calls including
directory traversal, file opening, buffered reads, title normalization, and
set/result destruction. Warmups populate the OS file cache. Useful throughput
counts complete scans, not disk bandwidth. Headers precede the title by
0/31/63 lines; the missing control scans all 64 lines. The long Unicode case
uses 63 long header lines. This is a warm-cache measurement and does not
predict cold-storage latency.

## Pack search index construction and complete row parsing

| Workload | CPU cycles saved, median (range) | Throughput | Allocations + reallocations | Requested bytes, median |
| --- | ---: | ---: | ---: | ---: |
| empty | 0.58% (-4.34..6.98%) | 1.01x | 1 + 0 → 1 + 0 | 21 → 21 |
| ascii-lower | 40.22% (37.62..51.45%) | 1.66x | 1 + 0 → 1 + 0 | 49 → 49 |
| ascii-upper | 44.18% (41.44..52.71%) | 1.80x | 1 + 0 → 1 + 0 | 75 → 75 |
| long-ascii | 93.57% (93.01..94.20%) | 15.56x | 1 + 0 → 1 + 0 | 1327 → 1327 |
| unicode | -1.19% (-9.84..1.24%) | 0.99x | 1 + 0 → 1 + 0 | 61 → 61 |
| long-unicode | 0.42% (-4.40..4.08%) | 1.00x | 1 + 1 → 1 + 1 | 9708 → 9708 |
| row-ascii | 17.99% (11.46..19.99%) | 1.22x | 6 + 0 → 6 + 0 | 125 → 125 |
| row-unicode | 6.90% (2.41..10.18%) | 1.07x | 6 + 0 → 6 + 0 | 119 → 119 |

Each sample runs 8192 operations, including output destruction. Direct indexing
isolates the changed function; row workloads include parsing all eight catalog
columns, owned fields, the original/current constructor, and index creation.
Inputs are built outside timing. Throughput counts indexes or rows per second.

## Follow-up results

| Workload | Primary CPU savings, median (range) | Follow-up CPU savings, median (range) |
| --- | ---: | ---: |
| owned-urls/empty | -0.03% (-1.60..24.91%) | 0.81% (-4.94..8.36%) |
| owned-urls/rejected | 1.69% (-19.07..10.75%) | 1.21% (-17.02..12.99%) |
| owned-urls/one-https | 7.71% (-0.32..15.31%) | 9.76% (6.33..14.07%) |
| owned-urls/https | 12.71% (8.60..16.18%) | 14.79% (13.18..16.79%) |
| owned-urls/relative | 9.10% (5.22..10.86%) | 9.01% (8.09..11.22%) |
| owned-urls/rooted | 10.03% (7.04..12.19%) | 9.28% (5.43..11.22%) |
| owned-urls/escaped-https | 18.42% (17.33..20.97%) | 20.13% (15.70..21.48%) |
| owned-urls/dense-escapes | 38.40% (38.04..40.52%) | 38.50% (37.36..39.62%) |
| owned-urls/long-https | 32.09% (29.49..34.63%) | 31.32% (10.66..31.87%) |
| owned-urls/unicode-escapes | 19.74% (19.21..21.08%) | 18.80% (16.01..20.91%) |
| title-lines/first | 0.46% (-1.90..5.54%) | -0.00% (-0.80..2.97%) |
| title-lines/middle | 0.17% (-0.93..2.28%) | 1.22% (-3.10..1.70%) |
| title-lines/last | 5.18% (1.47..5.45%) | 2.17% (-0.86..5.51%) |
| title-lines/missing | 2.48% (-0.21..4.97%) | 2.14% (-1.02..5.20%) |
| title-lines/long-unicode | 3.85% (1.76..5.28%) | 3.80% (2.03..7.87%) |
| pack-index/empty | 0.58% (-4.34..6.98%) | 0.75% (-2.80..7.63%) |
| pack-index/ascii-lower | 40.22% (37.62..51.45%) | 42.10% (39.17..43.85%) |
| pack-index/ascii-upper | 44.18% (41.44..52.71%) | 45.71% (42.92..46.83%) |
| pack-index/long-ascii | 93.57% (93.01..94.20%) | 93.64% (93.24..93.92%) |
| pack-index/unicode | -1.19% (-9.84..1.24%) | -2.29% (-4.21..0.39%) |
| pack-index/long-unicode | 0.42% (-4.40..4.08%) | -0.37% (-4.20..4.84%) |
| pack-index/row-ascii | 17.99% (11.46..19.99%) | 17.56% (16.22..18.71%) |
| pack-index/row-unicode | 6.90% (2.41..10.18%) | 4.47% (1.32..6.27%) |

Ranges crossing zero do not establish a CPU gain or slowdown. Negative control
medians are retained in this report; no improvement is claimed for uncertain
controls. Deterministic allocation/capacity checks are separate from timing.

## Behavioral validation

- 309 online tests passed in debug and release, serially. Five new regression
  tests compare complete observable results with frozen originals. Seventeen
  tests are ignored by default (manual benchmarks and one live download).
- URL tests compare every retained field, capacities, 0/1/128 rows, accepted
  and rejected URLs, schemes, roots, relative paths, Unicode, queries/fragments,
  NULs, and all slash/backslash sequences through length eight. Pointer reuse
  and strict complete-parse allocation reductions are tested.
- Title tests cover line 1/2/32/64/65/66, LF/CRLF/missing final newline, invalid
  UTF-8 before/after a title, first-title wins, extension case, unrelated files,
  directories with simfile extensions, and missing directories. A 64-line scan
  strictly reduces allocation churn.
- Pack tests compare name offsets, full text, exact capacity, every Unicode
  scalar, mixed ASCII/Unicode metadata, ID extremes, full row fields, parse
  errors, and relevance-ordered search results.
- Architecture checks: 141 passed, the exact same 12 existing failures as the
  previous pass. No new failure. Clippy: existing 11 library/21 test warnings,
  none new. Changed Rust files pass rustfmt; `git diff --check` passes.
- Cargo.toml and all three inherited Cargo.lock package entries change exactly
  `0.5.1709 → 0.5.1710`. No dependency changes. Excluded user files are not staged.

## Reproduction

```powershell
cargo test -p deadsync-online -- --test-threads=1
cargo test --release -p deadsync-online -- --test-threads=1
cargo clippy -p deadsync-online --all-targets
cargo test --release -p deadsync-online owned_download_urls_benchmark -- --ignored --nocapture --test-threads=1
cargo test --release -p deadsync-online simfile_titles_benchmark -- --ignored --nocapture --test-threads=1
cargo test --release -p deadsync-online pack_index_benchmark -- --ignored --nocapture --test-threads=1
```

Use `DEADSYNC_BENCH_NEW_FIRST=1` on alternating rounds and remove it for the other
rounds. Compile once, then invoke the built test executable directly for timing.
Raw primary and follow-up measurements are in `urls-title-lines-pack-index-0.5.1710.csv` and
`urls-title-lines-pack-index-0.5.1710-controls.csv` alongside this report.
