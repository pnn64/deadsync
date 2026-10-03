# Palette loading, Workshop paths, and search sorting — 0.5.1707

Original: `8e6b38040` (`0.5.1706`). Current: `0.5.1707`.

## Changes

1. Load judgment palettes using the existing borrowed INI parser. Section
   names, keys, and values remain slices of the input, instead of becoming
   owned strings that are immediately discarded. Retained catalog names,
   IDs, and defaults still own their strings. The shared parser preserves
   duplicate-section and duplicate-key rules.
2. Resolve the Workshop pack root directly for asset setup, eliminating
   `assets/noteskins → assets/noteskins/hurg → assets/noteskins`. Workshop
   downloads append `hurg` to the same buffer. Candidate paths are constructed
   lazily without copying their parent roots. Directory queries keep their
   original order and count; fallback, raw path spelling, and overlay order
   are preserved. No filesystem result is cached.
3. Compare title slices directly during search sorting. Equal bytes need no
   ASCII folding, and title differences need no subtitle trim or virtual
   joined iterator. Only an equal title prefix reaches the original-style
   remaining-title/space/subtitle comparison. This retains byte ordering,
   prefix behavior, blank subtitles, and stable ties without new allocations.

## Method

Windows x86_64, Intel Xeon E5-2696 v4 (22 cores / 44 logical processors),
Rust/Cargo 1.98.1, release optimization level 3 with full LTO. The unchanged
scoped System allocator and sampling helpers are in tests/support/perf.rs.
Both implementations run in the same release test executables, with opaque
function pointers at measured entry points. Frozen function bodies and their
original inline attributes were verified against `8e6b38040`; differences
are limited to test visibility/names and qualifying the palette return type.
Fixture construction is outside timing; outputs are black-boxed and output
destruction is included. Timing and allocation counting are separate.

Six serial rounds alternate original-first/current-first order. Each
measurement has three warmups and seven samples. CPU cycles use Windows
QueryThreadCycleTime on the calling thread. Tables give the median of the
six paired savings/throughput ratios, with the full observed paired range
(not a confidence interval). Positive CPU savings mean fewer cycles.
Throughput counts complete operations, except catalogs, where it counts songs.
All 27 workloads have both variants in every round: 324 primary measurements.
An additional six rounds of the twelve Workshop workloads add 144 measurements.
No compilation or other test suite ran concurrently with either benchmark run.

Allocation counts include frees of measured results; requested bytes measure
allocation traffic, not RSS or peak live memory. All measured result allocations
are freed. Reallocations are reported separately. Palette/catalog results have
the same retained contents and ownership in both implementations.

## Palette loading

| Workload | CPU cycles saved, median (range) | Throughput | Allocations + reallocations | Requested bytes |
| --- | ---: | ---: | ---: | ---: |
| empty | 5.05% (-37.87..33.74%) | 1.14x | 4 + 0 → 4 + 0 | 422 → 422 |
| one | 23.67% (16.23..39.06%) | 1.31x | 33 + 2 → 11 + 2 | 5417 → 4895 |
| sixteen | 44.26% (41.54..47.89%) | 1.80x | 366 + 7 → 59 + 7 | 54610 → 47666 |
| many | 40.32% (29.45..50.95%) | 1.72x | 2834 + 13 → 399 + 13 | 504607 → 449194 |
| unrelated | 56.10% (54.68..57.62%) | 2.28x | 1063 + 2 → 145 + 2 | 68563 → 48631 |

Fixtures contain 0, 1, 16, or 128 custom palettes, each with seven color keys,
a name, and order. The unrelated case has one palette and 128 unrelated INI
sections with three keys each. Each sample loads and destroys 256 catalogs.
The sixteen-palette case saves 44.26% of CPU cycles and 307 allocations.
The empty control has identical allocation counts and a wide CPU range; no
CPU gain or regression is established for that control.

## Workshop and complete asset path preparation

| Workload | CPU cycles saved, median (range) | Throughput | Allocations + reallocations | Requested bytes |
| --- | ---: | ---: | ---: | ---: |
| workshop-installed | -0.36% (-3.91..9.82%) | 1.00x | 4 + 2 → 2 + 1 | 789 → 426 |
| assets-installed | 4.20% (-0.55..7.60%) | 1.04x | 24 + 16 → 22 + 15 | 4659 → 4296 |
| workshop-portable | -1.01% (-4.61..5.79%) | 0.99x | 4 + 2 → 2 + 1 | 789 → 426 |
| assets-portable | 4.68% (-1.41..8.18%) | 1.05x | 20 + 12 → 18 + 11 | 3730 → 3367 |
| workshop-checkout | 0.76% (-2.56..15.03%) | 1.01x | 6 + 3 → 2 + 1 | 1149 → 446 |
| assets-checkout | 2.67% (-1.06..3.84%) | 1.03x | 31 + 21 → 27 + 19 | 6085 → 5382 |
| workshop-nested | -2.66% (-10.01..7.54%) | 0.97x | 8 + 4 → 4 + 2 | 1633 → 890 |
| assets-nested | -1.37% (-6.74..5.03%) | 0.99x | 33 + 22 → 29 + 20 | 6543 → 5800 |
| workshop-file-first | 2.56% (-0.59..12.00%) | 1.03x | 8 + 4 → 4 + 2 | 1650 → 900 |
| assets-file-first | 2.69% (-0.97..6.91%) | 1.03x | 33 + 22 → 29 + 20 | 6573 → 5823 |
| workshop-fallback | 1.48% (0.35..2.65%) | 1.02x | 11 + 6 → 7 + 4 | 2310 → 1584 |
| assets-fallback | 3.76% (-6.06..8.00%) | 1.04x | 36 + 24 → 32 + 22 | 7281 → 6555 |

Each sample executes 512 complete operations. Installed and portable cases
find the executable's assets; checkout and nested cases find the first and
second CWD candidates. File-first encounters a file where a directory was
expected, then resolves the nested candidate. Fallback misses all candidates.
Real fixtures are below target/perf-1707-workshop. Both variants use the same
paths and warm filesystem metadata, so filesystem costs dominate these timings.

Every workload reduces allocation calls and requested bytes. Workshop checkout
uses 6 allocations + 3 reallocations originally, and 2 + 1 now; requested
bytes fall from 1149 to 446 (61.18%). Complete checkout asset preparation falls
from 31 + 21 to 27 + 19, with requested bytes down from 6085 to 5382 (11.55%).

The small negative medians in the initial run were checked with six additional
alternating rounds using the same binaries. All results, including negative
values, are retained below and in the separate probe CSV.

| Workload | Initial CPU savings, median (range) | Follow-up CPU savings, median (range) |
| --- | ---: | ---: |
| workshop-installed | -0.36% (-3.91..9.82%) | 0.65% (-2.75..9.07%) |
| assets-installed | 4.20% (-0.55..7.60%) | -0.21% (-2.57..7.18%) |
| workshop-portable | -1.01% (-4.61..5.79%) | 2.42% (-7.73..9.08%) |
| assets-portable | 4.68% (-1.41..8.18%) | 6.57% (-0.42..13.25%) |
| workshop-checkout | 0.76% (-2.56..15.03%) | 3.04% (-8.37..10.99%) |
| assets-checkout | 2.67% (-1.06..3.84%) | 2.39% (0.19..4.43%) |
| workshop-nested | -2.66% (-10.01..7.54%) | 3.44% (0.20..14.04%) |
| assets-nested | -1.37% (-6.74..5.03%) | 0.29% (-7.78..15.00%) |
| workshop-file-first | 2.56% (-0.59..12.00%) | 0.63% (-4.68..3.34%) |
| assets-file-first | 2.69% (-0.97..6.91%) | 1.87% (0.71..5.52%) |
| workshop-fallback | 1.48% (0.35..2.65%) | 2.39% (-2.30..9.87%) |
| assets-fallback | 3.76% (-6.06..8.00%) | 2.19% (1.03..6.54%) |

Workshop nested changes from -2.66% in the initial run to +3.44% in the
follow-up; complete nested assets change from -1.37% to +0.29%. Installed and
portable Workshop timings also change sign. These small differences vary
with the filesystem measurements; no stable CPU gain or regression is claimed
for Workshop paths. Their demonstrated improvement is reduced allocation churn.

## Search comparisons

| Workload | CPU cycles saved, median (range) | Throughput | Allocations + reallocations | Requested bytes |
| --- | ---: | ---: | ---: | ---: |
| early | 69.47% (67.93..70.48%) | 3.28x | 0 + 0 → 0 + 0 | 0 → 0 |
| short | 64.90% (63.82..67.49%) | 2.85x | 0 + 0 → 0 + 0 | 0 → 0 |
| long | 75.83% (75.49..76.46%) | 4.14x | 0 + 0 → 0 + 0 | 0 → 0 |
| subtitle | 23.66% (17.52..36.23%) | 1.31x | 0 + 0 → 0 + 0 | 0 → 0 |
| prefix | 20.37% (14.61..28.87%) | 1.26x | 0 + 0 → 0 + 0 | 0 → 0 |
| equal | 52.65% (51.78..53.91%) | 2.11x | 0 + 0 → 0 + 0 | 0 → 0 |
| unicode | 67.01% (65.57..68.61%) | 3.04x | 0 + 0 → 0 + 0 | 0 → 0 |

Each sample runs 65,536 comparisons. Cases cover first-byte differences,
short/long shared prefixes, equal titles with differing subtitles, title/space
prefix boundaries, exact ties, and UTF-8 names. All cases remain allocation-free.
The comparator saves 20–76% of cycles across these fixtures.

## Complete search candidate builds

| Workload | CPU cycles saved, median (range) | Throughput | Allocations + reallocations | Requested bytes |
| --- | ---: | ---: | ---: | ---: |
| plain | 27.28% (22.53..34.29%) | 1.38x | 1539 + 0 → 1539 + 0 | 88176 → 88176 |
| subtitles | 29.18% (23.14..32.95%) | 1.41x | 1539 + 0 → 1539 + 0 | 90224 → 90224 |
| varied | 7.93% (6.69..9.66%) | 1.09x | 1539 + 0 → 1539 + 0 | 79984 → 79984 |

Each sample builds and destroys 64 complete candidate lists for 256 matching
songs with dance-single charts. Entries start with a pack header; input order
is a deterministic permutation. Plain and subtitle catalogs have long shared
title prefixes, while varied uses differing first letters and title lengths.
The empty query deliberately retains every song, exercising construction,
formatting, stable sorting, Arc ownership, and destruction together.
Complete builds save 7.93–29.18% of cycles with identical allocation traffic.

## Behavioral validation

- 502 tests passed in both debug and release: 242 configuration unit tests,
  245 simfile unit tests, 13 loading-diagnostics tests, and one test each for
  library reload and pack reload. Existing manual benchmarks remain ignored.
- Six new regression/allocation tests cover duplicate INI keys/sections,
  trimmed/duplicate IDs, invalid/default names/orders/colors, built-in
  protection, retained catalog serialization/default resolution, portable and
  installed paths, directory/file/missing fallbacks, exact raw separator
  spelling, CWD/executable precedence, and non-Unicode Windows paths.
- Search comparisons check all 38,416 pairs of 196 title/subtitle combinations
  against both frozen code and materialized lowercase joined strings. Complete
  searches compare output fields, stable tied order, and Arc song identity;
  difficulty/BPM/title misses, pack matching, and chartless entries remain unchanged.
- Architecture suite: 141 pass and the exact same 12 failures as the previous
  pass; no new failure. Their parent reproduction is documented in
  [the 0.5.1699 report](actor-song-timing-0.5.1699.md#behavioral-validation).
- Clippy completed with the existing warning counts unchanged: configuration
  4 library / 16 library-test warnings, simfile 5 / 13. No warning points to
  the new benchmark/regression files or changed code.
- Changed Rust files pass rustfmt checks; git diff --check passes. Cargo.toml
  and Cargo.lock contain only the exact 0.5.1706 → 0.5.1707 version changes.

## Reproduction

Run the full behavioral suites with:

```powershell
cargo test -p deadsync-config -p deadsync-simfile
cargo test --release -p deadsync-config -p deadsync-simfile
```

Run each manual benchmark serially, alternating the environment variable
DEADSYNC_BENCH_NEW_FIRST between unset and 1 for six rounds:

```powershell
cargo test --release -p deadsync-config judgment_palettes::tests::palette_loading_perf::palette_loading_benchmark -- --exact --ignored --nocapture --test-threads=1
cargo test --release -p deadsync-config dirs::workshop_paths_perf::workshop_paths_benchmark -- --exact --ignored --nocapture --test-threads=1
cargo test --release -p deadsync-simfile song_search::tests::search_sort_perf::search_sort_benchmark -- --exact --ignored --nocapture --test-threads=1
```

The recorded runs invoke Cargo's already-built executables directly to keep
feature graphs and binaries identical between variants and rounds.

[Primary raw measurements](palettes-workshop-search-0.5.1707.csv) ·
[Workshop follow-up measurements](palettes-workshop-search-0.5.1707-workshop-probe.csv).
