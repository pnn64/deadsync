# Configuration, writability, and background parsing — 0.5.1706

Original: `462b31ec5` (`0.5.1705`). Current: `0.5.1706`.

## Changes

1. Write additional song folders and NeverCacheList directly into the final
   configuration String. This removes temporary joins, their copies into the
   output, and generic formatting for these fixed keys. The private unused
   NeverCacheList joining helper is removed. The public folder-list formatter
   keeps its existing output and shares its append loop with the writer.
2. Return true immediately for writability checks with no additional-folder
   rules. Previously the function canonicalized the target even though no rule
   could use that result. With rules present, canonicalization still runs in
   the original order; failures now borrow the raw path rather than copying it.
   Longest-root precedence and last-rule tie breaking stay unchanged.
3. Check background fields for .ini and .xml in one traversal instead of two
   substring scans. Remove the shared substring helper. ASCII case folding,
   substring matches (including suffixes), and short/Unicode input behavior
   stay unchanged. Delimiter parsing keeps its original implementation.

## Method

Windows x86_64, Intel Xeon E5-2696 v4 (22 cores / 44 logical processors),
Rust/Cargo 1.98.1, release optimization level 3 with full LTO. Both variants
run in the same release test executables through opaque function pointers.
Test-only originals are copied from `462b31ec5`; function bodies and original
inline attributes were verified against that commit. Input fixtures are built
outside measurement. Outputs are black-boxed, and temporary/result destruction
is included in timing.

Six serial rounds alternate original-first and current-first order. Each
measurement uses three warmups and seven samples. CPU cycles use Windows
QueryThreadCycleTime on the calling thread. The table reports the median of
six paired CPU savings/throughput ratios and their observed range, not a
confidence interval. Throughput is complete operations per second. All 24
workloads have original/current results in all six rounds: 288 CSV measurements.

The scoped allocator records allocation/reallocation requests and frees.
Requested bytes describe allocation traffic, not RSS or peak live memory.
Reused configuration output buffers are allocated before measurement; growing
buffers start empty and are destroyed within each measured operation.

## Configuration list writes

| Workload | CPU cycles saved, median (range) | Throughput | Allocations + reallocations | Requested bytes |
| --- | ---: | ---: | ---: | ---: |
| empty | 89.84% (89.51..90.35%) | 9.95x | 0 + 0 → 0 + 0 | 0 → 0 |
| one | 92.90% (90.26..93.06%) | 14.24x | 2 + 0 → 0 + 0 | 34 → 0 |
| eight | 88.77% (85.94..89.69%) | 8.94x | 3 + 6 → 0 + 0 | 1719 → 0 |
| sixty-four | 71.02% (69.04..72.28%) | 3.45x | 3 + 12 → 0 + 0 | 16975 → 0 |
| long | 70.80% (68.80..71.50%) | 3.43x | 3 + 12 → 0 + 0 | 88207 → 0 |
| growing | 57.45% (50.34..75.29%) | 2.35x | 4 + 17 → 1 + 8 | 151662 → 84968 |

Fixtures contain 0, 1, 8, or 64 folders and NeverCacheList entries. Writable
and read-only folders alternate. Names include Unicode; the long fixture has
256-byte suffixes. All except growing reuse enough existing output capacity,
as the application's configuration builder does for ordinary settings.

## Song writability

| Workload | CPU cycles saved, median (range) | Throughput | Allocations + reallocations | Requested bytes |
| --- | ---: | ---: | ---: | ---: |
| empty-existing | >99.99% (all six rounds) | 23362.44x | 2 + 0 → 0 + 0 | 250 → 0 |
| empty-missing | 99.97% (99.96..99.97%) | 3539.47x | 2 + 0 → 0 + 0 | 278 → 0 |
| existing-one | -0.77% (-3.62..2.36%) | 0.99x | 4 + 0 → 4 + 0 | 458 → 458 |
| existing-eight | 0.45% (-2.54..1.61%) | 1.00x | 18 + 0 → 18 + 0 | 1914 → 1914 |
| missing-one | 2.47% (-1.00..9.86%) | 1.03x | 4 + 0 → 2 + 0 | 514 → 344 |
| missing-eight | 1.09% (-0.95..5.72%) | 1.01x | 18 + 0 → 9 + 0 | 2310 → 1546 |
| missing-sixty-four | 2.11% (0.58..7.53%) | 1.02x | 130 + 0 → 65 + 0 | 16696 → 11174 |

Existing fixtures use real files/directories below target/perf-1706-paths.
Missing fixtures use an absent sibling with 1, 8, or 64 rules. Existing-rule
controls exercise successful canonicalization, where ownership is still
required. The no-rule cases remove filesystem queries; the remaining cases
retain every query and do not cache results across operations. Filesystem
benchmarks use the same paths and process for both variants and measure warm
metadata, so absolute timings depend on the host/filesystem cache.

## Background media field filtering

| Workload | CPU cycles saved, median (range) | Throughput | Allocations + reallocations | Requested bytes |
| --- | ---: | ---: | ---: | ---: |
| empty | 0.00% (-2.63..4.65%) | 1.00x | 0 + 0 → 0 + 0 | 0 → 0 |
| short | 5.32% (-44.74..28.57%) | 1.07x | 0 + 0 → 0 + 0 | 0 → 0 |
| ini-early | 4.55% (-10.34..16.81%) | 1.05x | 0 + 0 → 0 + 0 | 0 → 0 |
| xml-early | 25.66% (21.24..41.36%) | 1.34x | 0 + 0 → 0 + 0 | 0 → 0 |
| movie | 44.40% (35.66..47.87%) | 1.80x | 0 + 0 → 0 + 0 | 0 → 0 |
| image | 48.72% (40.89..56.79%) | 1.95x | 0 + 0 → 0 + 0 | 0 → 0 |
| ini-path | -2.17% (-8.79..3.60%) | 0.98x | 0 + 0 → 0 + 0 | 0 → 0 |
| xml-path | 51.58% (50.47..53.38%) | 2.07x | 0 + 0 → 0 + 0 | 0 → 0 |
| long-media | 49.50% (47.21..50.05%) | 1.98x | 0 + 0 → 0 + 0 | 0 → 0 |
| long-xml | 49.91% (48.26..50.69%) | 2.00x | 0 + 0 → 0 + 0 | 0 → 0 |
| many-dots | 50.07% (46.28..50.53%) | 2.00x | 0 + 0 → 0 + 0 | 0 → 0 |

Each operation applies the public field predicate to one complete path.
Fixtures cover empty/short strings, early INI/XML matches, normal movies and
images, INI/XML paths, long relative paths, and repeated dots. All cases remain
allocation-free. Both implementations use 65,536 operations per sample to
amortize clock/counter overhead for the smallest controls.

## Behavioral validation

- 496 tests passed in debug and release: 238 configuration unit tests, 243
  simfile unit tests, 13 loading-diagnostics tests, and the library/pack reload
  integration tests. All three manual benchmarks passed in all six combined
  rounds. The background-only probe also passed in six rounds.
- Five new normal test definitions compare exact configuration output across
  512 entry/flag combinations and existing prefixes, enforce reduced churn for
  list writes and writability checks, exercise real/missing paths, dot paths,
  invalid Windows UTF-16, longest-root precedence and last-rule ties, and check
  3,735 background field cases with case variants, Unicode and substring
  suffixes. The field tests also run in the loading-diagnostics executable.
  The removed private joining helper's narrow test is replaced by the writer's
  complete byte comparisons. The public folder-list formatter remains covered.
- Existing background parser tests still cover directory-entry precedence,
  newlines, Unicode, truncations and randomized records. Its delimiter-search
  body was verified identical to the parent after the tested vector variants
  were discarded for short-record/newline regressions.
- Successful canonicalization controls retain exactly the original allocation
  counts and show no clear CPU change (-0.77% and +0.45% median, with ranges
  spanning zero). Failed canonicalizations retain all filesystem queries and
  halve allocation calls; 64 missing rules improve CPU in all six rounds.
- Empty/short field controls take only a few cycles; their relative ranges are
  noisy. INI-path matches have a -2.17% median CPU saving in the combined run
  (range -8.79..3.60%), while the same final binary's separate six-round
  background probe measured +3.73% (range -11.47..10.06%). There is no stable
  gain or regression for that case. Movie, image, XML and long-path gains are
  positive in every combined round and reproduced in the probe. The probe's
  132 additional measurements are saved separately without replacing any of
  the combined run's controls.
- Clippy completed with existing warnings only; no new test module or modified
  production code emitted a warning. Targeted rustfmt and git diff --check
  passed. Architecture checks remain 141 passed / 12 failed, with exactly the
  same failure names as the previous pass; their parent reproduction is
  recorded in [the 0.5.1699 report](actor-song-timing-0.5.1699.md#behavioral-validation).
- Cargo.toml and Cargo.lock contain only the exact patch increment
  0.5.1705 → 0.5.1706. No dependencies were added or updated.


## Reproduce

```powershell
cargo test -p deadsync-config -p deadsync-simfile
cargo test --release -p deadsync-config -p deadsync-simfile
$filters = @(
  'list_writes_perf::list_writes_benchmark',
  'folders::writable_paths_perf::writable_paths_benchmark',
  'bgchanges::bg_field_filter_perf::bg_field_filter_benchmark'
)
foreach ($round in 1..6) {
  if ($round % 2 -eq 0) { $env:DEADSYNC_BENCH_NEW_FIRST = '1' }
  else { Remove-Item Env:DEADSYNC_BENCH_NEW_FIRST -ErrorAction SilentlyContinue }
  foreach ($testFilter in $filters) {
    cargo test --release -p deadsync-config -p deadsync-simfile $testFilter -- --exact --ignored --nocapture --test-threads=1
  }
}
Remove-Item Env:DEADSYNC_BENCH_NEW_FIRST -ErrorAction SilentlyContinue
```

[Raw measurements](config-paths-bgchanges-0.5.1706.csv).

[Background-only probe measurements](config-paths-bgchanges-0.5.1706-bg-probe.csv).
