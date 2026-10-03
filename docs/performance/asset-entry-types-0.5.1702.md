# Asset discovery performance 0.5.1702

Parent: `1958d87f1db982d1ef8a008e6a737cf935b30c17` (0.5.1701).
Date: 2026-10-03.

Three discovery optimizations remove redundant filesystem metadata work:

1. Artwork enumeration keeps each `DirEntry` until its file classification
   is complete. Accepted ordinary images use the entry's existing file type.
2. Song-background and random-movie enumeration do the same for accepted
   movies. Both movie paths retain their existing, different sorting rules.
3. Foreground resolution uses the entry's type for supported candidates,
   preserving extension rank and filename tie breaking.

A small crate-private helper shares the classification rule: ordinary
entries use `DirEntry::file_type`; links and file-type errors fall back to
the original target-following `Path::is_file`. Unsupported extensions still
short circuit before classification. No dependency, unsafe block or public
API change is added. The helper reuses classification data already returned
by directory enumeration.

## Measurement method

Windows x86-64, Intel Xeon E5-2696 v4 (22 cores/44 logical processors),
Rust/Cargo 1.98.1, release opt-level 3 with full LTO. The four original
function bodies are frozen from the parent commit, verified identical
modulo visibility and formatting, and run alongside current code in the
same final release test binary. Sorting/ranking helpers are unchanged and
shared by both variants.

One operation enumerates a real temporary directory and returns/destroys
the sorted path vector or selected foreground path. Enumeration, path
construction, classification, ordering and result destruction are included.
Fixture creation and cleanup are outside measurement. Supported files are
empty: these benchmarks measure discovery, not image/movie decoding.
Artwork rotates PNG/JPG/GIF/JPEG, movies MP4/AVI/WMV/MPG, and foreground
PNG/MP4/JPG/AVI. Unsupported files have `.ssc` extensions.

Six serial rounds alternate original-first/current-first after compilation
and regression tests finish. Each variant has three warmups and seven
samples of 16 queries; `QueryThreadCycleTime` measures calling-thread CPU
cycles. Other host activity is not isolated. Tables show separate six-round
time medians and medians of paired cycle reductions/throughput ratios.
Negative reductions mean more cycles. Range values show variation across
all six paired rounds, including controls.

The [raw CSV](asset-entry-types-0.5.1702.csv) contains 240 measurements:
20 workloads times six rounds times two implementations. The existing
thread-local System allocator wrapper measures a separate complete query,
including result destruction. Requested/freed bytes measure allocator
traffic, not peak RSS. Temporary path lengths vary between subprocesses,
explaining requested-byte ranges. One avoided metadata query removes one
allocation/free pair per supported ordinary file on this Windows host;
reallocation counts remain identical for every workload.

## Artwork enumeration

| Files / supported | Original -> current us/query | CPU-cycle reduction (round range) | Throughput ratio | Allocations/query | Requested bytes/query |
|---|---:|---:|---:|---:|---:|
| 32 / 32 | 1678.1 -> 177.9 | 89.4% (88.8..90.0) | 9.42x | 169 -> 137 | 43386..43486 -> 36906..36942 |
| 128 / 128 | 6414.4 -> 411.5 | 93.6% (92.7..93.9) | 15.58x | 649 -> 521 | 169914..170302 -> 143994..144126 |
| 1024 / 1024 | 52408.7 -> 2704.4 | 94.8% (94.0..95.0) | 19.29x | 5130 -> 4106 | 1375418..1378494 -> 1168058..1169086 |
| 1024 / 8 | 2609.9 -> 2219.7 | 15.1% (11.2..17.4) | 1.18x | 4113 -> 4105 | 1047058..1048102 -> 1045438..1046466 |
| 128 / 0 | 365.2 -> 368.6 | 0.5% (-4.4..6.2) | 1.00x | 518 -> 518 | 131770..131902 -> 131770..131902 |

## Movie enumeration

Song-background movie lists:

| Files / supported | Original -> current us/query | CPU-cycle reduction (round range) | Throughput ratio | Allocations/query | Requested bytes/query |
|---|---:|---:|---:|---:|---:|
| 32 / 32 | 1686.6 -> 171.2 | 89.8% (89.6..89.9) | 9.83x | 200 -> 168 | 43610..43710 -> 37146..37182 |
| 128 / 128 | 6485.7 -> 410.1 | 93.6% (93.1..93.8) | 15.53x | 776 -> 648 | 170810..171198 -> 144954..145086 |
| 1024 / 1024 | 51782.8 -> 2692.1 | 94.7% (94.3..94.9) | 18.72x | 6152 -> 5128 | 1358010..1361086 -> 1151162..1152190 |
| 1024 / 8 | 2604.0 -> 2267.1 | 12.8% (7.2..15.3) | 1.15x | 4120 -> 4112 | 1047114..1048158 -> 1045498..1046526 |
| 128 / 0 | 364.7 -> 361.5 | -3.2% (-5.1..4.5) | 0.97x | 518 -> 518 | 131770..131902 -> 131770..131902 |

Random-movie lists:

| Files / supported | Original -> current us/query | CPU-cycle reduction (round range) | Throughput ratio | Allocations/query | Requested bytes/query |
|---|---:|---:|---:|---:|---:|
| 32 / 32 | 1651.3 -> 167.0 | 89.8% (89.0..90.2) | 9.81x | 167 -> 135 | 42430 -> 35902 |
| 128 / 128 | 6435.3 -> 398.2 | 93.7% (93.5..94.1) | 15.79x | 647 -> 519 | 166078 -> 139966 |
| 1024 / 1024 | 51477.8 -> 2615.8 | 94.9% (94.8..95.0) | 19.66x | 5128 -> 4104 | 1352894 -> 1143998 |
| 1024 / 8 | 2571.7 -> 2301.8 | 11.9% (5.7..14.2) | 1.14x | 4111 -> 4103 | 1047838 -> 1046206 |
| 128 / 0 | 361.4 -> 363.1 | -2.4% (-5.6..6.1) | 0.98x | 518 -> 518 | 131902 -> 131902 |

## Foreground selection

| Files / supported | Original -> current us/query | CPU-cycle reduction (round range) | Throughput ratio | Allocations/query | Requested bytes/query |
|---|---:|---:|---:|---:|---:|
| 32 / 32 | 1648.4 -> 165.3 | 90.1% (89.7..90.6) | 10.09x | 166 -> 134 | 40510 -> 33982 |
| 128 / 128 | 6475.0 -> 376.2 | 94.3% (94.0..94.3) | 17.42x | 646 -> 518 | 158014 -> 131902 |
| 1024 / 1024 | 51637.5 -> 2269.8 | 95.6% (95.4..95.7) | 22.73x | 5126 -> 4102 | 1254718 -> 1045822 |
| 1024 / 8 | 2597.6 -> 2208.1 | 15.0% (13.3..21.1) | 1.18x | 4110 -> 4102 | 1047454 -> 1045822 |
| 128 / 0 | 357.4 -> 357.3 | 0.6% (-3.3..7.4) | 1.01x | 518 -> 518 | 131902 -> 131902 |

All-supported directories reduce median CPU cycles by 89.4-95.6% and
improve throughput 9.42x-22.73x. Every paired round improves for every
supported workload. Mixed directories with eight supported files improve
median cycles 11.9-15.1%, with 1.14x-1.18x throughput.

All four discovery paths remove one allocation/free pair per supported
ordinary file: 32, 128 or 1024 pairs in the all-supported cases and eight in
the mixed cases. Requested/freed bytes fall accordingly; reallocation
counts are unchanged.

Unsupported-only controls retain identical allocation/free/reallocation
traffic. Their paired median cycle changes range from a 3.2% increase to a
0.6% reduction; all four round ranges cross zero and timing ranges overlap.
These noisy controls do not establish a timing improvement or a consistent
regression. The tables retain the negative medians instead of treating them
as gains. No behavioral regression is found.

These are complete directory-discovery measurements. They do not establish
whole-library loading time or gameplay FPS gains. CPU/allocation results
apply to this Windows host; other platforms are not benchmarked here.

## Behavioral validation

- All 238 simfile library tests pass in debug and release; 17 manual
  benchmarks remain ignored in normal test runs.
- New tests compare complete returned paths and order against the parent's
  implementations for empty, all-supported, mixed and unsupported-only
  directories. They cover case/Unicode names, resource-fork handling,
  supported-extension directories, hard links, surviving hard links after
  source deletion, links/dangling targets, missing paths and non-directories.
  Existing selection/ordering tests also pass. Foreground ranks and selected
  filenames match the original.
- File symlink assertions run when the host permits symlink creation.
  Windows directory junctions exercise link/dangling-target behavior without
  requiring file-symlink privilege.
- Ordinary file classification matches `Path::is_file` with zero heap
  churn on this host. Complete supported-file queries assert reduced
  allocation churn; all unsupported-only controls retain identical counts.
- Architecture checks: 141 pass and the exact same 12 pre-existing failures
  remain. Their parent reproduction is documented in
  [the baseline report](actor-song-timing-0.5.1699.md#behavioral-validation).
- Simfile Clippy for all targets completes with existing warnings.
  Targeted rustfmt and `git diff --check` pass.
- Cargo.toml and Cargo.lock change exactly 0.5.1701 -> 0.5.1702. The lockfile
  changes only the three packages inheriting that workspace version.

## Reproduce

```powershell
cargo test -p deadsync-simfile --lib
cargo test --release -p deadsync-simfile --lib
cargo clippy -p deadsync-simfile --all-targets
cargo test --release -p deadsync-simfile --lib artwork::entry_types_perf::artwork_entry_types_benchmark -- --exact --ignored --nocapture --test-threads=1
cargo test --release -p deadsync-simfile --lib changes::entry_types_perf::movie_entry_types_benchmark -- --exact --ignored --nocapture --test-threads=1
cargo test --release -p deadsync-simfile --lib media::entry_types_perf::media_entry_types_benchmark -- --exact --ignored --nocapture --test-threads=1
```

Run the three benchmarks serially after all builds/tests finish. Repeat six
rounds, setting `$env:DEADSYNC_BENCH_NEW_FIRST = '1'` for even rounds and
removing it for odd rounds. Each command measures both the frozen original
and current implementation; the media command measures both random-movie
and foreground discovery.
