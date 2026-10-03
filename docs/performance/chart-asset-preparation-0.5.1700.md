# Chart and asset preparation performance 0.5.1700

Parent: `7084e09f3` (0.5.1699). Date: 2026-10-03.

Three changes remove intermediate storage and unnecessary work during loading:

1. Convert borrowed cached notes directly into the gameplay note vector.
   Previously, the reference loader cloned all cached notes into a temporary
   `CachedChartPayload`, then allocated the final, larger note representation.
   The owning payload loader remains available for decoded cache data.
2. Reject non-image extensions before `Path::is_file` in artwork discovery.
   This skips target-following metadata queries for chart, Lua and other
   irrelevant files while preserving image-file and link handling.
3. Reject non-movie extensions before `Path::is_file` in background-movie and
   random-movie discovery. Movies retain their original filtering, filename
   order and existing sorters. Mixed song/asset directories avoid metadata
   queries for every unrelated entry.

No dependencies, unsafe code or public API changes were added.

## Measurement method

Windows x86-64, Intel Xeon E5-2696 v4 (22 cores/44 logical processors),
Rust/Cargo 1.98.1, release opt-level 3 with full LTO. Six serial rounds alternate
original-first/current-first after builds and validation finish. Each round
reports seven-sample medians and ranges. `QueryThreadCycleTime` records the
calling thread's CPU cycles. Other host activity is not isolated.

The three original function bodies are frozen from the parent and verified
modulo formatting. Both variants use the same unchanged timing conversion,
filename sorters and extension predicates. They run in the same release test
binary; directory comparisons measure complete listing operations.

The [raw CSV](chart-asset-preparation-0.5.1700.csv) contains 108 measurements
across nine paired workloads: elapsed time, thread cycles, useful-item
throughput, allocation/reallocation/free calls, requested bytes and freed
bytes. A thread-local System wrapper counts heap traffic in a separate
operation after timing. Requested bytes measure allocator traffic, not peak
RSS. Tables show separate six-round ns/op medians and medians of paired cycle
reductions/throughput ratios; their ratios can differ. Negative reductions
indicate more cycles.

These cases isolate loading operations and do not measure whole-library scan
time, gameplay FPS or frame latency.

## Gameplay note loading

One operation constructs and destroys a complete gameplay chart. Source
charts, raw note bytes and row/timing data are prepared outside measurement.
The complex case includes stops, delays, warps, speeds, scrolls, fakes and an
attack string. Both variants use the unchanged owning timing conversion and
`TimingData` constructor. Throughput counts charts per second.

| Workload | Original -> current ns/op | CPU-cycle reduction (round range) | Throughput ratio | Allocations/op | Requested bytes/op |
|---|---:|---:|---:|---:|---:|
| `notes/1024/complex=false` | 49817.2 -> 22417.2 | 55.3% (10.5..91.2) | 5.63x | 18 -> 17 | 70932 -> 54548 |
| `notes/65536/complex=false` | 1793361.0 -> 1343556.2 | 25.3% (22.0..28.6) | 1.34x | 18 -> 17 | 4522260 -> 3473684 |
| `notes/65536/complex=true` | 1619787.5 -> 1254203.1 | 21.1% (19.5..32.1) | 1.27x | 27 -> 26 | 4522491 -> 3473915 |

One allocation/free pair disappears in every note-loading case: 16,384 bytes
for 1024 notes and 1,048,576 bytes for 65,536 notes. Large simple/complex cases
reduce median cycles by 25.3%/21.1%, with positive savings in every round and
1.34x/1.27x throughput. The smaller case has a wider cycle range; its exact
allocation saving is independent of the allocator-sensitive timing variation.

## Asset filename filtering

One operation lists and sorts image paths in a real temporary song directory.
The all-image case is a control; mixed cases contain many `.ssc` files and
eight images. Fixture creation/destruction are outside timing. Enumeration,
path construction, metadata queries and returned-vector destruction are
included. Throughput counts directory queries per second.

| Workload | Original -> current ns/op | CPU-cycle reduction (round range) | Throughput ratio | Allocations/op | Requested bytes/op |
|---|---:|---:|---:|---:|---:|
| `assets/files=32/images=32` | 1671700.0 -> 1682203.1 | -0.3% (-3.6..2.6) | 1.00x | 169 -> 169 | 43754..43854 -> 43754..43854 |
| `assets/files=128/images=8` | 6336181.3 -> 775737.5 | 87.7% (87.6..89.0) | 8.13x | 649 -> 529 | 159818..160206 -> 134618..134766 |
| `assets/files=1024/images=8` | 51445268.8 -> 2837165.6 | 94.6% (94.0..95.2) | 18.60x | 5129 -> 4113 | 1264586..1267662 -> 1051226..1052270 |

Mixed directories reduce median cycles by 87.7-94.6% and increase throughput
8.13x-18.60x. On this Windows host, omitting metadata also removes 120 or 1016
allocation/free pairs, matching the number of rejected files. The all-image
control has identical allocation traffic and a -0.3% median cycle reduction;
its -3.6%..+2.6% round range shows no consistent timing change.

## Movie filename filtering

One operation lists and sorts background-movie paths in a real temporary song
folder. The all-movie case is a control; mixed cases contain many `.ssc` files
and eight movies. Fixture creation/destruction are outside timing. Enumeration,
path construction, metadata queries, the original cached-key filename sort and
returned-vector destruction are included. Throughput counts queries per second.

| Workload | Original -> current ns/op | CPU-cycle reduction (round range) | Throughput ratio | Allocations/op | Requested bytes/op |
|---|---:|---:|---:|---:|---:|
| `movies/files=32/movies=32` | 1679340.6 -> 1620831.3 | 0.7% (-1.9..7.2) | 1.01x | 200 -> 200 | 44010..44110 -> 44010..44110 |
| `movies/files=128/movies=8` | 6344887.5 -> 753496.9 | 88.2% (87.7..88.5) | 8.51x | 656 -> 536 | 159882..160270 -> 134682..134830 |
| `movies/files=1024/movies=8` | 52005475.0 -> 2589093.7 | 94.9% (94.8..95.2) | 19.64x | 5136 -> 4120 | 1264650..1267726 -> 1051290..1052334 |

Mixed directories reduce median cycles by 88.2-94.9% and increase throughput
8.51x-19.64x. They also remove 120 or 1016 allocation/free pairs on Windows.
The all-movie control retains identical allocation traffic, with a 0.7% median
cycle reduction and a -1.9%..+7.2% round range. Both directory optimizations
retain the original output vectors and sorting behavior.

All allocation/free counts and byte totals balance. Reallocation counts remain
unchanged in every pair. Unique temporary paths differ between processes, so
absolute directory byte totals and metadata-byte savings vary with path length.
The 1024-entry mixed cases remove 213,360-215,392 requested/freed bytes per
operation. These allocation counts are Windows measurements, not a claim that
metadata queries allocate on every platform.

## Behavioral validation

- All 231 simfile library tests pass in debug and release.
- Direct-loader comparisons cover empty/small charts, every note type,
  hold/roll tails, simple/complex timing, offsets and NaNs. Complete debug
  output includes every gameplay-chart field and timing internals. A churn
  assertion protects removal of the intermediate note buffer.
- Directory comparisons cover image/movie extension case, resource forks,
  unrelated files, directories with media extensions, missing directories,
  file links when privileges permit and Windows directory junctions.
- Root architecture boundaries: 141 pass; the same 12 failures established in
  the previous pass remain. The failure set is unchanged. See the
  [previous report](actor-song-timing-0.5.1699.md) for baseline reproduction.
- Clippy completes with existing warnings. Targeted rustfmt checks for all
  changed production/test files and `git diff --check` pass.

## Reproduce

```powershell
cargo test -p deadsync-simfile --lib
cargo test --release -p deadsync-simfile --lib
cargo clippy -p deadsync-simfile --lib
cargo test --release -p deadsync-simfile --lib cache::tests::gameplay_note_loading_perf::gameplay_note_loading_benchmark -- --exact --ignored --nocapture --test-threads=1
cargo test --release -p deadsync-simfile --lib artwork::asset_filtering_perf::asset_filtering_benchmark -- --exact --ignored --nocapture --test-threads=1
cargo test --release -p deadsync-simfile --lib changes::movie_filtering_perf::movie_filtering_benchmark -- --exact --ignored --nocapture --test-threads=1
```

Run benchmarks serially after builds finish. Repeat six rounds, setting
`$env:DEADSYNC_BENCH_NEW_FIRST = '1'` for even rounds and removing it for odd
rounds. Ignored benchmarks do not run during ordinary regression tests.
