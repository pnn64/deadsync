# Chart metadata paths, 2026-11-04

Three retained optimizations in `deadsync-chart`, following the timing batch
on isolated branch `perf/non-lua-kernels-20261104`. This batch starts at
`e77434c22`. The main worktree's existing edits and Lua crates are outside its
scope.

## Changes

1. `step_type_lanes` rejects impossible lengths and checks the game prefix
   once. A safe four-byte integer load folds the ASCII prefix case in one
   operation. The style determines the lane count after dispatch, replacing
   the two closures and up to seven repeated prefix/style checks. Dance and
   pump aliases, case folding, Unicode trimming and default lane counts stay
   unchanged.
2. `SongData::is_no_cmod` rejects candidate positions with one four-byte
   comparison before checking the remaining `mod` suffix. Its mask clears the
   ASCII case bit in `n`, `o` and `c`, while keeping the space byte exact. This
   replaces repeated character comparisons in the search. Existing subtitle
   handling and matches spanning the title/subtitle boundary are unchanged.
3. `SongData::has_standard_difficulty` rejects unequal chart-type lengths,
   then checks the requested difficulty before comparing chart-type contents.
   Most charts can now skip the longer type comparison. The first matching
   chart, invalid-index behavior and case-insensitive matching are unchanged.

All code is safe Rust. The fixed slices are bounded by the dispatch length
guard or the seven-byte search window. Both word operations use explicit
little-endian conversion, so their semantics do not depend on host endianness.
Production changes are confined to `notes.rs` and `song.rs`: 30 added and 20
removed lines, including comments. They remove repeated execution and dispatch
closures; the bit comparisons require ten net additional source lines.

## Final measurements

Windows x86-64, Rust 1.99.1 / LLVM 23.1.1, release optimization with LTO and
static MSVC runtime. The runner builds byte-identical core/rules/chart sources
in a small workspace, with dependency versions recorded in each manifest.
Fixtures and allocation measurements are outside the timed region.

Each batch contains 256 queries. The shared benchmark helper pins the thread
to CPU 2, alternates original/current implementations over 12 rounds, drops
the warmup, and reports the median of 11 Windows `QueryThreadCycleTime`
measurements. The following values are medians of five final process runs.
CPU throughput is original/current cycles; these are kernel measurements.

| Workload | Before cycles/256 | After cycles/256 | CPU throughput | Fewer cycles |
| --- | ---: | ---: | ---: | ---: |
| Step types, common mixed styles | 9,492.50 | 7,767.23 | 1.222x | 18.2% |
| Step types, fallback/trimmed/Unicode | 7,498.56 | 6,995.47 | 1.072x | 6.7% |
| No-CMod search, ordinary titles | 24,150.27 | 17,097.29 | 1.413x | 29.2% |
| No-CMod search, tagged titles | 10,440.30 | 8,054.33 | 1.296x | 22.9% |
| No-CMod search, long titles | 101,897.60 | 66,818.43 | 1.525x | 34.4% |
| Difficulty presence, standard set | 67,275.87 | 22,895.60 | 2.938x | 66.0% |
| Difficulty presence, mixed types | 53,389.43 | 24,058.08 | 2.219x | 54.9% |
| Difficulty presence, missing type | 27,678.07 | 25,992.75 | 1.065x | 6.1% |

Both variants allocate zero bytes and perform zero allocations, reallocations
or frees in all eight workloads. All five final paired runs improve every
workload. Primary paired throughput ranges are 1.151–1.226x for common step
types, 1.385–1.430x for ordinary titles, and 2.887–3.003x for standard sets.

The `control` stage builds production sources directly from `e77434c22` with
the final integration-test harness. Its original/current median ratios range
from 0.986x to 1.027x, providing a check on harness and code-layout bias.
Absolute cycle counts change with machine load, so comparisons use variants
paired within a process rather than separate stages' absolute timings.

## Stages and rejected candidates

Adjacent TSVs and manifests preserve the sequential before/after evidence:

- `baseline`: unchanged production code, with the initial harness.
- `lanes`: prefix factoring, before the final four-byte prefix refinement.
- `cmod`: rejected `memchr2_iter` substring search. Long titles improved
  6.354x, but ordinary and tagged titles slowed to 0.848x and 0.650x.
- `cmod-word`: retained four-byte candidate rejection instead of that search.
- `cycle`: rejected movie-cycle remainder replacement. Including path cloning,
  ratios were 0.997–1.016x for nonempty cycles; the change was reverted.
- `presence-before` / `presence`: before/after the difficulty predicate change.
- `control`: exact pre-batch production source with the final harness.
- `final`: all three retained changes, final harness, five runs.

Initial stages include movie-cycle controls, and the presence stages add
difficulty queries. The final harness contains only the eight workloads for
the retained changes. Frozen originals, source snapshots, executable hashes,
compiler details and per-run output are archived under
`target/chart-metadata-paths`. The manifests preserve source hashes and the
dependency lockfile; final and debug manifests also record test totals and
correctness-log hashes.

## Correctness and reproduction

All 203 core/rules/chart unit and integration tests pass in release and debug.
The added integration tests compare exact outputs with frozen production
functions from `e77434c22`:

- Every case combination of all eight recognized step types, both separators,
  Unicode whitespace, ASCII-byte mutations and multibyte substitutions.
- Title/subtitle boundary cases, empty/whitespace subtitles, control bytes,
  per-byte keyword mutations, and 100,000 deterministic random title pairs.
- Difficulty/type combinations including Unicode, aliases, duplicate charts,
  unknown types, all standard indices and invalid indices through `usize::MAX`.

The benchmark separately asserts zero allocation churn for both variants.
The existing chart parser and metadata suites also pass. Rustfmt passes for
`notes.rs` and the added Rust files; changed `song.rs` lines are formatted,
while that existing file contains unrelated formatting differences.
`git diff --check` and Python compilation pass. Clippy completes for all chart
targets, with existing warnings outside the changed code.

```powershell
python scripts/benchmark_timing_conversions.py control --chart --baseline e77434c22
python scripts/benchmark_timing_conversions.py final --chart --runs 5
python scripts/benchmark_timing_conversions.py debug --chart --debug
$env:RUSTFLAGS = '-C target-feature=+crt-static'
cargo clippy --offline --manifest-path target/chart-metadata-paths/workspace/Cargo.toml -p deadsync-chart --all-targets --no-deps
```

`--baseline` changes only the archived build copy. Set
`DEADSYNC_CHART_ARCHIVE` to place that copy and binaries on another drive.
Without `--chart`, the same runner continues to reproduce the earlier timing
conversion batch.
