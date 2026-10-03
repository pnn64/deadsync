# Timing and foreground preparation performance 0.5.1701

Parent: `4499f127a` (0.5.1700). Date: 2026-10-03.

Three optimizations remove redundant storage or work:

1. Construct BPM timing points directly in one `Arc<[BeatTimePoint]>` using
   the existing exact-size initializer. The original `Arc<Vec<_>>` needed
   a separate vector allocation and capacity bookkeeping. Offset changes
   retain copy-on-write behavior. `find_event` now accepts the point slice.
2. Borrow time signatures already in total beat order and beginning at a
   nonpositive note row; borrow a constant 4/4 signature for empty input.
   Background expansion no longer copies, allocates and sorts these inputs.
   Unordered tables and tables needing a default insertion keep the original
   owning normalization and stable sort.
3. Check the foreground media extension before querying `Path::is_file`.
   Unsupported entries avoid metadata queries. Supported files keep their
   original extension rank, filename comparison and target-following checks.

No dependencies, unsafe blocks or public API changes were added. Row-table
storage remains unchanged. A shared row-slice experiment was rejected after
its larger cases produced mixed CPU results.

## Measurement method

Windows x86-64, Intel Xeon E5-2696 v4 (22 cores/44 logical processors),
Rust/Cargo 1.98.1, release opt-level 3 with full LTO. Six serial rounds
alternate original-first/current-first after compilation and tests finish.
Each measurement reports seven-sample medians/ranges and thread CPU cycles
from `QueryThreadCycleTime`. Other host activity is not isolated.

The [raw CSV](timing-foreground-0.5.1701.csv) contains 204 measurements:
17 paired workloads times six rounds times two implementations. Allocation
counts are measured separately from timing using the existing thread-local
System allocator wrapper. Requested/freed bytes measure allocator traffic,
not peak RSS. Tables show separate six-round time medians and medians of
paired cycle reductions/throughput ratios. Negative reductions mean more
cycles; overlapping ranges and varying round signs indicate noise.

The BPM baseline freezes the parent's point-building loop and original
`Arc::new(Vec)` storage. The signature and foreground baseline function
bodies match the parent modulo visibility/naming/formatting. Those comparisons
run in the same final release test binaries and include result destruction.
The unchanged point arithmetic, extension rank and filename comparator are
shared by both variants.

Complete timing comparisons also build the untouched parent rules/core Rust
sources in an isolated workspace, adding only the identical `timing_full.rs`
benchmark module. The parent enables the same `log` std/alloc features as the
current build; compiler, release profile and allocator harness match. The
separate binaries alternate execution order each round. Their 125 original
and 127 current rules regression tests both pass.

## BPM timing storage

One operation constructs and destroys a shared point table, including the
original floating-point/FMA and saturating time arithmetic. Inputs are
prepared outside measurement. Throughput counts tables per second.

| Workload | Original -> current ns/op | CPU-cycle reduction (round range) | Throughput ratio | Allocations/op | Requested bytes/op |
|---|---:|---:|---:|---:|---:|
| `bpms/1` | 142.0 -> 100.2 | 31.1% (6.4..38.3) | 1.46x | 2 -> 1 | 56 -> 32 |
| `bpms/128` | 957.9 -> 825.7 | 13.6% (6.8..33.6) | 1.16x | 2 -> 1 | 2088 -> 2064 |
| `bpms/1024` | 6233.8 -> 5875.1 | 5.0% (1.1..13.6) | 1.05x | 2 -> 1 | 16424 -> 16400 |

Median CPU cycles fall 5.0-31.1%, with positive savings in every round for
all three table sizes. Every case removes one allocation/free pair and
24 requested/freed bytes.
The shared table's vector header and capacity disappear. On this target,
`TimingData` grows from 168 to 176 bytes because its slice pointer carries the
length; one timing instance plus its point allocation therefore uses 16 fewer
requested bytes. Additional clones have an eight-byte larger inline layout.
This is a layout calculation, not an RSS measurement.

## Background time signatures

One operation normalizes and drops a signature table. Common normalized
tables and empty/default input perform zero heap operations. Reversed and
positive-start tables exercise the owning compatibility path, including
default insertion and its original reallocation. Throughput counts tables
per second.

| Workload | Original -> current ns/op | CPU-cycle reduction (round range) | Throughput ratio | Allocations/op | Requested bytes/op |
|---|---:|---:|---:|---:|---:|
| `signatures/0/empty` | 82.3 -> 10.8 | 87.0% (84.8..88.2) | 7.77x | 1 -> 0 | 48 -> 0 |
| `signatures/1/sorted` | 73.3 -> 12.2 | 83.0% (81.0..84.9) | 5.97x | 1 -> 0 | 12 -> 0 |
| `signatures/128/sorted` | 303.9 -> 157.9 | 47.3% (38.9..49.4) | 1.90x | 1 -> 0 | 1536 -> 0 |
| `signatures/128/reversed` | 406.5 -> 407.2 | -0.7% (-4.0..1.9) | 0.99x | 1 -> 1 | 1536 -> 1536 |
| `signatures/128/positive` | 486.4 -> 476.4 | 2.9% (-4.6..3.9) | 1.03x | 1 -> 1 | 4608 -> 4608 |

The borrowed cases reduce median cycles 47.3-87.0% and remove all allocation
traffic. Owning controls retain identical allocation/reallocation traffic;
their cycle ranges cross zero and show no consistent timing change.

## Foreground media discovery

One operation enumerates a real temporary directory and returns its preferred
media path. Construction/destruction of filesystem fixtures is outside the
measurement. Enumeration, path allocation, metadata checks, ranking and
returned-path destruction are included. The all-supported case is a control;
mixed cases contain eight supported images/movies and many `.ssc` files.
Throughput counts directory queries per second.

| Workload | Original -> current ns/op | CPU-cycle reduction (round range) | Throughput ratio | Allocations/op | Requested bytes/op |
|---|---:|---:|---:|---:|---:|
| `foreground/files=32/supported=32` | 1692025.0 -> 1658150.0 | 1.8% (0.8..3.6) | 1.02x | 166 -> 166 | 41310..41410 -> 41310..41410 |
| `foreground/files=128/supported=8` | 6342056.2 -> 750465.7 | 88.2% (87.6..88.6) | 8.44x | 646 -> 526 | 161118..161506 -> 134718..134866 |
| `foreground/files=1024/supported=8` | 51251100.0 -> 2608778.1 | 94.9% (94.9..95.0) | 19.66x | 5126 -> 4110 | 1279326..1282402 -> 1055806..1056850 |

Mixed directories reduce median cycles by 88.2-94.9% and improve throughput
8.44x-19.66x. The all-supported control keeps identical allocation traffic
and has a 1.8% median cycle reduction. Mixed directories omit 120 or 1016
metadata calls. On this Windows host that
also removes the same number of allocation/free pairs. Temporary path lengths
vary between subprocesses, explaining the requested-byte ranges. Supported
directory entries still undergo a target-following file check, so directory
links, dangling file links and ordinary files preserve their prior handling.

## Complete timing controls

These invoke the real `TimingData::from_segments`, including modifier-table
preparation, timing conversion and result destruction. Sources/row arrays are
prepared outside measurement. Complex cases include stops, delays, warps,
speeds, scrolls and fakes. The original row-table copies remain included.
Throughput counts complete timing objects per second.

| Workload | Original -> current ns/op | CPU-cycle reduction (round range) | Throughput ratio | Allocations/op | Requested bytes/op |
|---|---:|---:|---:|---:|---:|
| `construct/bpms=1/rows=128/complex=false` | 1148.0 -> 1046.1 | 10.5% (-7.8..23.9) | 1.12x | 10 -> 9 | 704 -> 680 |
| `construct/bpms=1/rows=65536/complex=false` | 21855.7 -> 20046.2 | 3.2% (-5.2..18.0) | 1.03x | 10 -> 9 | 262336 -> 262312 |
| `construct/bpms=1024/rows=8192/complex=false` | 139922.8 -> 134227.5 | 4.0% (0.6..7.0) | 1.04x | 10 -> 9 | 49328 -> 49304 |
| `construct/bpms=64/rows=8192/complex=true` | 49143.7 -> 47833.6 | 3.9% (-2.3..9.3) | 1.04x | 12 -> 11 | 39888 -> 39864 |

Complete construction removes one allocation/free and 24 requested/freed
bytes in every case. CPU results should be read with the displayed round
ranges, rather than treating isolated table gains as whole-load gains.
The 1024-BPM case reduces median cycles 4.0% with positive savings in every
round. Smaller/simple and complex constructor results are noisier, although
their paired medians also improve.

The query controls use prepared timing with 128 BPM/modifier segments and
8192 rows. Each operation runs 1024 lookups; ns/op and cycles/op refer to the
whole batch, and throughput counts individual lookups. Both implementations
remain allocation-free.

| Workload | Original -> current ns/op | CPU-cycle reduction (round range) | Throughput ratio | Allocations/op | Requested bytes/op |
|---|---:|---:|---:|---:|---:|
| `queries/rows` | 1574.2 -> 1516.7 | 4.2% (-4.6..13.4) | 1.04x | 0 -> 0 | 0 -> 0 |
| `queries/time` | 9156007.8 -> 8988930.1 | 1.9% (1.4..2.9) | 1.02x | 0 -> 0 | 0 -> 0 |

Time queries reduce cycles in every round; the row query control has a wider
range crossing zero. No behavioral failures or consistent CPU regressions
appear in these controls.

These measurements do not establish gameplay FPS or whole-library scan gains.

## Behavioral validation

- Debug library tests: 35 chart, 127 rules, 861 gameplay and 233 simfile tests
  pass (1256 total).
- Release library tests: all 395 chart/rules/simfile tests pass. The separately
  built original rules suite passes all 125 tests.
- New point tests compare exact float bits, timestamps and maximum BPM for
  duplicate beats, infinities/NaNs, invalid BPMs and saturating offsets, and
  assert reduced allocation churn. Clone/offset tests verify sharing,
  detachment and unchanged original timing.
- Existing timing tests compare tables, beat/time conversions, modifier
  queries, rewinds, nonfinite inputs and offset changes with their original
  algorithms. Only their private BPM field initializers were adapted to the
  new storage representation; the new benchmark keeps original storage.
- Signature tests compare stable duplicate order, signed zero, nonfinite
  values, empty defaults and row-rounded insertion boundaries. Normalized
  input remains unchanged and is borrowed with zero churn. Complete generated
  background rows/paths/effects/transitions match the original signatures.
- Foreground tests compare ranks, case/Unicode names, resource-fork names,
  missing/empty directories, supported-extension directories, file links,
  dangling links and directory junctions against the frozen original.
  File symlink assertions run when available; junctions are checked on Windows
  without symlink privilege.
- Architecture checks: 141 pass and the same 12 pre-existing failures remain.
  Their parent reproduction is documented in
  [the preceding baseline report](actor-song-timing-0.5.1699.md#behavioral-validation).
- Clippy for chart/rules/simfile all targets completes with existing warnings
  (including the already multiply included filesystem fixture module).
  Targeted rustfmt and `git diff --check` pass.
- Cargo.toml and Cargo.lock change exactly 0.5.1700 -> 0.5.1701. The lockfile
  changes only the three packages inheriting that workspace version.

## Reproduce

```powershell
cargo test -p deadsync-chart -p deadsync-rules -p deadsync-gameplay -p deadsync-simfile --lib
cargo test --release -p deadsync-chart -p deadsync-rules -p deadsync-simfile --lib
cargo clippy -p deadsync-chart -p deadsync-rules -p deadsync-simfile --all-targets
cargo test --release -p deadsync-rules --lib timing::timing_storage_perf::bpm_storage_benchmark -- --exact --ignored --nocapture --test-threads=1
cargo test --release -p deadsync-chart --lib background::signature_perf::signature_normalization_benchmark -- --exact --ignored --nocapture --test-threads=1
cargo test --release -p deadsync-simfile --lib media::foreground_filtering_perf::foreground_filtering_benchmark -- --exact --ignored --nocapture --test-threads=1
$env:DEADSYNC_BENCH_VARIANT = 'new'
cargo test --release -p deadsync-rules --lib timing::timing_full_perf::full_timing_benchmark -- --exact --ignored --nocapture --test-threads=1
```

For the complete original constructor/query comparison, make a separate
checkout of `4499f127a`, copy `crates/deadsync-rules/tests/perf/timing_full.rs`
into it, and append its `#[cfg(test)]`/`#[path]` module registration to that
checkout's timing.rs. Build with the same release flags and dependency
features, set `DEADSYNC_BENCH_VARIANT=old`, and run the same full-timing test.
The isolated parent build here copies its core/rules sources unchanged and
explicitly enables `log`'s std feature to match the unified current build.

Run benchmarks serially after all builds/tests finish. Repeat six rounds,
setting `$env:DEADSYNC_BENCH_NEW_FIRST = '1'` for even rounds and removing it
for odd rounds. Also alternate the separate parent/current full-timing
binaries. Ignored benchmarks stay out of ordinary regression runs.
