# Partner-chart projection, 2026-11-04

Three retained optimizations in `deadsync-simfile::partner_charts`, starting at
`f7f34d930` on isolated branch `perf/non-lua-kernels-20261104`. Lua and the main
worktree's existing changes are outside this batch.

## Changes and exactness

1. Recognize partner chart types with ASCII-insensitive comparisons, keeping
   the existing UTF-8 validation and Unicode trimming. Routine suffix checks
   compare borrowed bytes, and fixed `dance-double`/`pump-double` replacements
   use borrowed `Cow` values instead of copied vectors. The suffix still trims
   only ASCII whitespace, preserving the original distinction from type lookup.
2. Iterate nonempty measures directly. Count borrowed nonempty lines, then read
   the same iterator to place notes on the authored grid. This removes the
   measure vector and every per-measure line vector. Empty-measure numbering,
   trimming, floating-point row rounding, keysounds and hold handling are unchanged.
3. Allocate the final merged grid once, fill empty rows, then scatter heads and
   tails directly from the existing tracks. The ownership map already supplies
   the final row bound. Remove the separate cell-symbol tree, its insertions,
   and a tree lookup for every output cell. Writes retain the original per-track
   insertion order, including replacement of an earlier tail by a later head.

All production changes use safe Rust. No dependency, state field or public API
is added. Hold overlap resolution and ownership/hash generation retain their
existing algorithms. The production diff changes four function bodies; the
additional module declaration only enables tests.

## Final paired measurements

| Workload | Before cycles/fixture | After cycles/fixture | CPU throughput |
| --- | ---: | ---: | ---: |
| Valid type lookup, 256 calls | 70,367.53 | 22,934.57 | 3.358x |
| Mixed type lookup, 256 calls | 51,948.52 | 14,217.02 | 3.670x |
| Sparse participant, 16 measures | 102,704.03 | 98,003.08 | 1.048x |
| Dense participant, 16 measures | 507,154.75 | 497,207.73 | 0.990x |
| Sparse merge, 16 measures | 1,973,188.00 | 543,891.80 | 3.628x |
| Dense merge, 16 measures | 7,044,589.20 | 3,582,957.00 | 1.987x |
| Hold/mine merge, 16 measures | 3,825,168.00 | 1,563,542.40 | 2.459x |
| Couple projection, 16 charts | 16,667.33 | 5,663.07 | 2.981x |
| Routine projection, four charts | 1,950,433.13 | 1,493,126.80 | 1.306x |

Cycle columns are independent five-process medians; throughput is the median
of the five paired ratios, so it need not equal the ratio of the two columns.
Dense participant CPU changes are inconclusive (paired range 0.916?1.042x).
Retention of the vector removal is justified by its allocation savings. Sparse
participant ratios are 1.015?1.119x. Sparse/dense/hold merges range
3.469?3.725x, 1.945?2.037x and 2.373?2.583x; couple/routine projections range
2.742?3.377x and 1.273?1.331x.

| Workload | Allocations before → after | Reallocations before → after | Allocated bytes before → after |
| --- | ---: | ---: | ---: |
| Valid type lookup, 256 calls | 256 → 0 | 0 → 0 | 3,072 → 0 |
| Sparse participant | 50 → 33 | 66 → 0 | 42,112 → 9,920 |
| Dense participant | 354 → 337 | 66 → 0 | 131,072 → 98,880 |
| Sparse merge | 283 → 191 | 155 → 11 | 230,031 → 115,525 |
| Dense merge | 2,535 → 1,925 | 155 → 11 | 804,015 → 580,437 |
| Hold/mine merge | 888 → 635 | 155 → 11 | 381,375 → 232,749 |
| Couple projection | 66 → 2 | 2 → 2 | 12,042 → 11,274 |
| Routine projection | 311 → 235 | 196 → 128 | 291,126 → 256,754 |

Counts cover each fixture including output destruction, with setup outside
measurement. Byte churn counts every allocation/reallocation request; it is
not retained memory. Peak live bytes are also recorded in the TSVs. For example,
dense-merge peak drops from 490,688 to 364,254 bytes. Routine projection still
allocates for tracks, ownership and participant hash preparation.

## Behavior verification

All **293 actual simfile unit tests pass** in release (23 tests remain ignored
as marked). The four focused projection tests also pass in debug and release;
these same four tests are included in the full-crate count. Full-crate
validation covers release unit tests; focused checks cover both profiles.

The initial complete run passed 292 tests and failed the existing junction
fixture because its temporary directory was on the non-NTFS D: volume.
The unchanged compiled unit-test executable passes all 293 tests when rerun
with the normal C: temporary directory. Its source overlay hashes match the
checkout, and its binary/log hashes are recorded. A redundant rebuild caused
by rewriting identical overlays was stopped; the runner now avoids those writes.

The focused production/test module passes Clippy with `-D warnings`. The
full-crate compiler reports existing dead-code warnings in unchanged frozen
song-search helpers. Changed-source formatting, Python compilation and whitespace
checks pass.

Four new differential tests compare the frozen production implementation with
the live functions. They cover 20,000 random byte strings plus directed valid,
invalid, ASCII-case and Unicode-whitespace types; all participant fields and
measure counts; and exact grid bytes and ownership for 294 fixtures. Cases
include empty/whitespace measures, comments containing `&`, more than two
participants, malformed keysounds, mines, open/overlapping/adjacent holds,
different grids and 257-line measures with rounded row collisions.

SM/SSC projection checks compare bytes, owners, hash-note inputs and borrowed
versus owned results, then parse projected bytes with the real RSSP parser.
Supported, unsupported and Unicode-trimmed chart types are included. Frozen
production code matches the pre-batch prefix byte-for-byte after normalization
of newlines and the test-only visibility change; wrappers only expose private
participant results for testing.

Every kernel stage emits a deterministic trace covering all merge fixtures
and additional SM/SSC projections. The traces are identical in every stage,
including debug: **1,709,939 bytes**, SHA-256
`678bd61a339559ac1dfbfd56d889dd706a74723a4a1de69f26c10091e62df032`.

## Evidence and limits

Adjacent TSVs/manifests retain these stages:

- `baseline`: original production prefix with the differential/benchmark tests.
- `recognition`: borrowed type recognition and fixed replacements.
- `measures`: rejected measure-vector-only change. Participant CPU regressed
  to 0.975x/0.959x despite saving one allocation and two reallocations.
- `measure-lines`: removes both measure and line vectors. Participant ratios
  are 1.039x/1.023x, with 17 fewer allocations and zero vector reallocations.
- `scatter`: adds direct output generation without the cell-symbol tree.
- `control`: original production prefix with the final tests, five runs.
- `final`: all retained changes, five runs.
- `debug`: unoptimized kernel correctness suite and deterministic trace.
- `full-release`: actual simfile crate and existing tests.

The final unchanged-source control medians range 0.984?1.044x. Routine
projection has a 4.4% bias; its measured gain remains larger. Individual
control pairs range 0.944?1.050x.
Small CPU differences are inconclusive. Absolute cycles vary with machine
load/frequency; compare paired ratios. Measurements cover projection kernels,
not filesystem scans, full-song analysis or application startup.

## Method and reproduction

Windows x86-64, Rust 1.99.1 / LLVM 23.1.1, CPU identifier
`Intel64 Family 6 Model 79 Stepping 1, GenuineIntel`. The kernel runner compiles
the complete, verbatim production prefix before the crate-wide test module in
a small offline workspace using the real locked RSSP dependency. Test-only
visibility wrappers are outside timed production code. Optimization level 3,
LTO and static MSVC runtime are enabled; debug information and incremental
compilation are disabled. The full mode builds the actual crate and its local
dependency tree from a Git archive plus the three current source overlays.

Threads are pinned to CPU 2. Twelve rounds alternate old/current order; the
first is warmup and the median of the remaining eleven is reported. Five fresh
processes supply the final/control paired medians. Allocation counters are
thread-local wrappers around `System` and run separately from timing.

Sources, generated kernel, compiler flags, dependency lockfile, executable,
trace and correctness-log hashes are recorded. Exact sources/binaries/logs are
archived at `D:\deadsync-partner-charts-20261104` on the measurement machine.

```powershell
$env:DEADSYNC_PARTNER_ARCHIVE = 'D:\deadsync-partner-charts-20261104'
python scripts/benchmark_partner_charts.py baseline --baseline f7f34d930
python scripts/benchmark_partner_charts.py control --baseline f7f34d930 --runs 5
python scripts/benchmark_partner_charts.py final --runs 5
python scripts/benchmark_partner_charts.py debug --debug
python scripts/benchmark_partner_charts.py full-release --full
```

The offline commands require cached dependencies. The first full build needed
the locked `log` version fetched into Cargo's cache. Full-suite compilation uses
two jobs after restarting an overly parallel build. Full-suite tests retain the
host's temporary-directory settings for NTFS junction support. Initial build,
environment-failure and corrected test logs are retained in the local archive.
No production source or dependency version was changed for these retries.
