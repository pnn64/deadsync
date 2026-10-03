# Song bounds, title search, and media paths — 0.5.1705

Original: `68e7d6aae` (`0.5.1704`). Current: `0.5.1705`.

This pass removes unused timing work from song-bound calculations, avoids virtual
title/subtitle traversal, and removes an intermediate media-path copy. The public
outputs and search/overlay ordering stay identical.

## Changes

1. `CachedTimingSegments::elapsed_time_segments` supplies only BPMs, stops, delays,
   and warps to song-bound calculations. Those calculations never use fakes,
   speeds, scrolls, signatures, tick counts, or combos.
   Their old full clone/conversion also built speed and scroll runtime caches
   that were immediately discarded. This removes six unused table copies and
   the derived visual caches from each eligible chart's bound calculation.
2. `joined_contains_ignore_ascii_case` searches the title and subtitle slices
   directly. After those searches miss, it compares only possible matches
   crossing the inserted space. This removes virtual joined-byte traversal and
   preserves case folding, whitespace rules, and cross-boundary hits.
3. `AppDirs::media_roots` appends the media directory to the existing
   `cwd/deadsync` buffer with an exact reservation, avoiding a second path copy
   and excess growth. Deduplication and overlay order still use the final paths.

## Method

Windows x86_64, Intel Xeon E5-2696 v4 (22 cores / 44 logical processors),
Rust/Cargo 1.98.1, release optimization level 3 with full LTO. Both versions run
in the same release test executables. Test-only originals were copied from
`68e7d6aae`, with their function bodies verified against that commit and their
original inline attributes retained. Only visibility/names change for access.
Song-bound and media-root timings call both variants through opaque function
pointers so the harness uses matching call overhead. Elapsed timing preparation
stays outside the caller's inline code to keep its unused/empty path small.

Six serial rounds alternate original-first and current-first execution. Each
measurement uses three warmups and seven samples. Table CPU savings and
throughput ratios are medians of the six paired comparisons; ranges describe
those comparisons, not confidence intervals. CPU cycles come from
`QueryThreadCycleTime` on the calling thread. Throughput is operations/second,
except catalog search, which counts 1,024 input songs per operation. Timing
includes destruction of results. Inputs are prepared outside measurement.

Allocation counts are a separate complete operation using the scoped counting
allocator in `tests/support/perf.rs`. Requested bytes include allocation and
reallocation traffic; they are not resident memory or peak live memory. The CSV
also records frees, freed bytes, nanoseconds, and sample ranges. Each of the 18
workloads has original/current results in all six rounds: 216 measurements.

## Measurements

### Song bounds

| Workload | CPU cycles saved, median (range) | Throughput | Allocations + reallocations | Requested bytes |
| --- | ---: | ---: | ---: | ---: |
| bounds-empty | 1.89% (-4.62..49.17%) | 1.03x | 0 + 0 → 0 + 0 | 0 → 0 |
| bounds-regular | 15.33% (11.84..37.35%) | 1.18x | 12 + 0 → 9 + 0 | 208 → 176 |
| bounds-visual32 | 49.25% (40.14..53.29%) | 1.97x | 20 + 0 → 12 + 0 | 5272 → 1256 |
| bounds-visual256 | 72.64% (71.63..74.43%) | 3.66x | 20 + 0 → 12 + 0 | 35480 → 4712 |
| bounds-dense | 64.18% (61.66..64.97%) | 2.80x | 20 + 0 → 12 + 0 | 35480 → 4712 |

The regular fixture has 8,192 rows, one BPM, no visual segments, and the usual
default signature/tick/combo entries. Visual fixtures contain the named number
of speed/scroll/signature/tick/combo segments and 16 or 64 BPMs with pauses,
warps, and fakes. The dense fixture has 65,536 rows. Empty bounds resolve no
notes and construct no timing data in either implementation.

Cached-chart metadata and gameplay function bodies were verified identical to
the original and tested for identical outputs. A broader metadata optimization
was discarded after a simple-metadata benchmark slowed. The final performance
measurements cover the three changed entry points and their controls.

### Title search

| Workload | CPU cycles saved, median (range) | Throughput | Allocations + reallocations | Requested bytes |
| --- | ---: | ---: | ---: | ---: |
| plain-miss | 21.46% (4.94..25.33%) | 1.27x | 0 + 0 → 0 + 0 | 0 → 0 |
| plain-hit | 35.46% (27.91..36.58%) | 1.55x | 0 + 0 → 0 + 0 | 0 → 0 |
| blank-subtitle | 20.72% (17.61..22.97%) | 1.26x | 0 + 0 → 0 + 0 | 0 → 0 |
| subtitle-miss | 31.86% (30.68..32.55%) | 1.47x | 0 + 0 → 0 + 0 | 0 → 0 |
| subtitle-crossing | 18.23% (8.66..20.13%) | 1.23x | 0 + 0 → 0 + 0 | 0 → 0 |
| catalog-plain-miss | 22.48% (18.33..34.39%) | 1.29x | 2 + 0 → 2 + 0 | 90203 → 90203 |
| catalog-plain-hit | 2.43% (-3.06..6.90%) | 1.03x | 6148 + 0 → 6148 + 0 | 352373 → 352373 |
| catalog-subtitle-miss | 24.65% (23.63..25.70%) | 1.33x | 2 + 0 → 2 + 0 | 90203 → 90203 |

Direct predicates are allocation-free in both versions. Catalog measurements
exercise the complete candidate builder and stable sort. Misses visit both
display and transliterated titles; hits retain the same candidate/string
allocations and song handles. Subtitle cases also exercise misses and matches
crossing the title/space/subtitle boundary.

### Media roots

| Workload | CPU cycles saved, median (range) | Throughput | Allocations + reallocations | Requested bytes |
| --- | ---: | ---: | ---: | ---: |
| portable | 0.27% (-4.54..5.67%) | 1.00x | 3 + 4 → 3 + 4 | 226 → 226 |
| portable-cwd | 22.14% (13.62..28.87%) | 1.29x | 6 + 10 → 5 + 9 | 436 → 357 |
| installed | 2.18% (-3.00..14.12%) | 1.01x | 3 + 4 → 3 + 4 | 226 → 226 |
| installed-cwd | 7.24% (-0.16..26.12%) | 1.08x | 6 + 8 → 5 + 8 | 396 → 373 |
| long-portable | 12.61% (10.39..13.43%) | 1.15x | 6 + 5 → 5 + 4 | 11357 → 9092 |

Portable layouts have identical data/executable roots. Installed layouts have
distinct roots; the no-CWD case controls for unchanged allocation work. The long
portable fixture uses 24 repeated Unicode directory components. No filesystem
query is removed or cached by this change; media roots are generated afresh.

## Behavioral validation

- All **491 tests pass in debug and release**: 235 config library tests,
  242 simfile library tests, 12 loading-diagnostics tests, and the library-reload
  and pack-reload integration tests. The three manual benchmark tests also ran
  successfully in each of the six rounds.
- Song bounds match the frozen original's complete serialized song bytes across
  180 parameter combinations covering empty/missing rows, hold tails, fake
  notes, edits, cabinet-light charts, unordered notes/BPMs, negative pauses and
  warps, empty timing tables, NaN rows, malformed unused speeds, hints, and
  offsets. Cached-chart metadata also retains identical encoded bytes, and
  inputs retain their original bytes. Allocation assertions cover ordinary and
  visual timing, including complete destruction of temporary timing objects.
- Title matching is compared at every character boundary for all title/subtitle
  pairs in the ASCII, Unicode, and whitespace fixtures. Full catalog comparisons
  cover matching/rejected difficulty and BPM filters, pack searches, subtitle
  hits, result order, and original song handles. The corrected catalog fixtures
  contain matching charts. Direct matching remains allocation-free.
- Media-root tests compare exact `OsStr` outputs, order, and deduplication for
  empty/relative/Unicode paths, dot segments, rooted and drive-relative names,
  UNC/verbatim roots, and an unpaired Windows UTF-16 surrogate. Checkout paths
  have strictly fewer allocations and requested bytes.
- The cached-chart and gameplay function bodies match `68e7d6aae` exactly.
  Frozen-original bodies and the single patch increment in both Cargo files
  were independently checked against that commit.
- `cargo clippy -p deadsync-config -p deadsync-simfile --all-targets` completes
  with existing warnings: config has 4 library/16 test warnings; simfile has
  5 library/13 test warnings. None originates in the new code or benchmark files.
  Targeted rustfmt and `git diff --check` pass.
- The architecture suite reports **141 passed and the same 12 existing
  failures** as the previous pass. The failure-name sets were compared exactly;
  the baseline reproduction is recorded in
  [the 0.5.1699 report](actor-song-timing-0.5.1699.md#behavioral-validation).

Every nonempty song-bound workload, direct title predicate, and catalog-miss
workload improves in all six paired rounds. Media paths with a portable CWD and
the long Unicode fixture also improve in every round. Installed-CWD CPU and
catalog-hit CPU results vary around modest gains. The no-CWD media controls and
empty bounds retain their allocation counts and show no repeatable slowdown;
their small CPU differences are not claimed as improvements.

## Reproduce

```powershell
cargo test -p deadsync-config -p deadsync-simfile
cargo test --release -p deadsync-config -p deadsync-simfile
$filters = @(
  'dirs::media_roots_perf::media_roots_benchmark',
  'cache::tests::metadata_timing_perf::metadata_timing_benchmark',
  'song_search::tests::title_search_perf::title_search_benchmark'
)
foreach ($round in 1..6) {
  if ($round % 2 -eq 0) { $env:DEADSYNC_BENCH_NEW_FIRST = '1' }
  else { Remove-Item Env:DEADSYNC_BENCH_NEW_FIRST -ErrorAction SilentlyContinue }
  foreach ($testFilter in $filters) {
    cargo test --release -p deadsync-config -p deadsync-simfile $testFilter -- --exact --ignored --nocapture --test-threads=1
  }
}
```

Raw results: [metadata-search-media-0.5.1705.csv](metadata-search-media-0.5.1705.csv).
