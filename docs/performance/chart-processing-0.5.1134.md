# Chart processing performance - 0.5.1134

This pass applies `M-HOTPATH`, `M-MEM-REUSE`, and `M-THROUGHPUT` from the supplied
`rust-performance.md` to chart attacks, simultaneous-note filtering, and timing
histograms. These are chart preparation and score-processing improvements, not
measurements of gameplay FPS.

## Three changes

1. **Keep owned note buffers around chart attacks.** A complete single-player
   range is transformed in its existing vector. Normal adjacent two-player
   ranges retain player one's allocation and split off only player two, then
   append the result. The old wrapper copied each attacked range into temporary
   vectors and assembled another chart-sized vector. Gapped, overlapping, and
   unusual ranges retain the compatibility path. Inserting notes can still grow
   storage; generated random attacks retain their existing allocations.
2. **Compact simultaneous-note filtering one row at a time.** Candidate storage
   fits inline for ordinary rows. Each row's removal decisions and retained hold
   endpoints are finalized before survivors are copied into the retained prefix.
   This removes a chart-sized removal bitmap, a candidate-vector allocation, and
   the final full-chart retention scan. Column sorting is needed only when a
   subset of candidates is rejected. Duplicate-heavy rows exceeding `MAX_COLS`
   use reusable spill storage and keep the old selection policy. Output order,
   fake-note policy, foreign lanes, and inclusive hold endpoints are preserved.
3. **Reuse six of seven histogram samples between output points.** The smoother
   stores seven counts inline and fetches/converts only the entering sample for
   the next point. This reduces lookups from 7N to N+6 without changing the seven
   fused multiply-add operations or their order. Clamped edges and floating-point
   output remain bit-exact. The output vector still allocates once; no additional
   temporary heap storage is introduced.

All production changes use safe Rust and add no dependencies or global caches.

## Method

Windows x86-64, Intel Xeon E5-2696 v4, rustc 1.98.0. Tests use the repository's
release profile (optimization level 3, full LTO). Frozen old routines come from
`535e921da` / 0.5.1133; their bodies were checked against that commit, ignoring
formatting and the renamed test helpers. Old and new run in the same executable
with the same allocator instrumentation and inputs. Shared downstream routines
stay common to both. Mirror attack benchmarks isolate the outer buffer change;
simultaneous filtering is compared independently against its old implementation.

Each workload warms up three times, then times seven batches. Each batch runs 32
chart operations, 512 smoothing operations, or 256 complete histogram builds.
Three invocations alternate order old/new, new/old, old/new. Results below are
medians of the three invocation medians. No builds or other tests run during
timing. Counters are disabled while timing and enabled for one additional
operation. Windows calling-thread cycles use `QueryThreadCycleTime`.

Allocation bytes are requested traffic, including full reallocation requests,
not peak live memory or process RSS. Reported allocation and free counts/bytes
match in these fixtures; all reallocation counts are zero. Throughput counts
input notes for chart/full-histogram work, or output points for smoothing.

Chart fixtures contain four tap notes per row, 128/2,048/8,192 rows per player,
and one or two players. Each measured operation restores input into a warmed
vector before calling the production routine; this identical copy is included
in both versions. Limits of two remove half the notes; limits of four exercise
the no-removal path. Attack fixtures apply a full-chart mirror. Their end-to-end
measurement includes attack parsing, turn application, row ordering, and range
rebuilding, as well as buffer management.

Smoothing fixtures cover 85 and 361 dense points and 1,801 sparse points.
Complete histogram builders process 128 or 8,192 judged rows with offsets spread
across 121 bins. Output destruction is included in measurements.

| Workload | Old us/op | New us/op | Old cycles/op | New cycles/op | Old -> new million items/s |
|---|---:|---:|---:|---:|---:|
| Filter 512 notes, limit 2 | 13.900 | 13.509 | 30,558.5 | 29,694.2 | 36.835 -> 37.900 |
| Filter 512 notes, limit 4 | 11.756 | 11.666 | 25,846.1 | 25,468.8 | 43.551 -> 43.890 |
| Mirror 512/player, 1P | 107.616 | 16.531 | 233,616.6 | 36,327.2 | 4.758 -> 30.972 |
| Mirror 512/player, 2P | 205.800 | 57.769 | 451,182.2 | 126,706.4 | 4.976 -> 17.726 |
| Filter 8,192 notes, limit 2 | 259.225 | 237.997 | 568,360.9 | 521,429.1 | 31.602 -> 34.421 |
| Filter 8,192 notes, limit 4 | 210.984 | 181.613 | 458,377.8 | 396,979.5 | 38.828 -> 45.107 |
| Mirror 8,192/player, 1P | 1,168.100 | 305.994 | 2,556,955.6 | 669,749.4 | 7.013 -> 26.772 |
| Mirror 8,192/player, 2P | 1,722.741 | 823.553 | 3,770,920.8 | 1,802,986.7 | 9.510 -> 19.894 |
| Filter 32,768 notes, limit 2 | 1,064.653 | 973.978 | 2,327,248.7 | 2,126,701.3 | 30.778 -> 33.643 |
| Filter 32,768 notes, limit 4 | 842.309 | 758.181 | 1,843,923.5 | 1,659,063.3 | 38.903 -> 43.219 |
| Mirror 32,768/player, 1P | 4,419.847 | 1,220.909 | 9,662,342.0 | 2,671,081.7 | 7.414 -> 26.839 |
| Mirror 32,768/player, 2P | 8,722.744 | 4,780.716 | 19,079,557.2 | 10,455,985.3 | 7.513 -> 13.708 |
| Smooth 85 dense points | 1.794 | 1.555 | 3,941.6 | 3,408.7 | 47.366 -> 54.673 |
| Smooth 361 dense points | 6.490 | 5.636 | 14,240.5 | 12,362.3 | 55.627 -> 64.047 |
| Smooth 1,801 sparse points | 149.928 | 46.063 | 328,482.6 | 100,986.3 | 12.012 -> 39.099 |
| Build histogram, 128 notes | 5.307 | 4.483 | 11,612.9 | 9,828.6 | 24.121 -> 28.554 |
| Build histogram, 8,192 notes | 88.146 | 86.951 | 193,163.4 | 190,018.4 | 92.937 -> 94.214 |

| Workload | Old -> new allocation/free calls per op | Old -> new requested/freed bytes per op |
|---|---:|---:|
| Filter 512 notes, limit 2 | 2 -> 0 | 672 -> 0 |
| Mirror 512/player, 1P | 2 -> 0 | 122,880 -> 0 |
| Mirror 512/player, 2P | 3 -> 1 | 245,760 -> 61,440 |
| Filter 8,192 notes, limit 2 | 2 -> 0 | 8,352 -> 0 |
| Mirror 8,192/player, 1P | 2 -> 0 | 1,966,080 -> 0 |
| Mirror 8,192/player, 2P | 3 -> 1 | 3,932,160 -> 983,040 |
| Filter 32,768 notes, limit 2 | 2 -> 0 | 32,928 -> 0 |
| Mirror 32,768/player, 1P | 2 -> 0 | 7,864,320 -> 0 |
| Mirror 32,768/player, 2P | 3 -> 1 | 15,728,640 -> 3,932,160 |
| Smooth 85 dense points | 1 -> 1 | 680 -> 680 |
| Smooth 361 dense points | 1 -> 1 | 2,888 -> 2,888 |
| Smooth 1,801 sparse points | 1 -> 1 | 14,408 -> 14,408 |
| Build histogram, 128 notes | 3 -> 3 | 2,044 -> 2,044 |
| Build histogram, 8,192 notes | 3 -> 3 | 2,044 -> 2,044 |

Both filter limits have the same allocation counts and bytes. On 32,768-note
charts, filtering removes 32,928 bytes of allocation traffic per call and lowers
cycles by about 9-10%. The small 512-note no-removal case is essentially flat
within batch variation, while still eliminating its two allocations.

For 32,768 notes/player, full-chart mirror processing reduces cycles by 72.4%
in single-player and 45.2% in versus. Single-player eliminates 7,864,320 requested
bytes per operation; versus reduces requested bytes by 75% (15,728,640 to
3,932,160). Throughput rises about 3.62x and 1.82x respectively.

Dense smoothing reduces median cycles by about 13%; sparse smoothing reduces
them by 69.3%, with the same output allocation. The full 128-note histogram
fixture improves by about 15% in this run. The 8,192-note full builder changes
by only about 1.6%, within overlapping batch ranges; its counting work remains
dominant. These measurements do not establish a broad score-screen speedup.

## Behavior and validation

- Eight new tests compare old/new note contents and range rebuilding, exercise
  active holds, fake and foreign-lane notes, oversized duplicate rows, removal
  limits, invalid/overlapping/gapped player ranges, and seeded random attacks.
  They also compare all smoothed floating-point bits across dense/sparse/empty
  histograms, narrow/clamped windows, and complete histogram metadata.
- Allocation gates require no churn for ordinary simultaneous filtering and
  single-player mirror attacks, at most one second-player buffer for versus
  mirror attacks, and only the output allocation for smoothing. Storage identity
  and retained capacity are checked where applicable.
- Baseline with behavior tests installed: gameplay 753 passed, rules 104 passed;
  each had one ignored manual benchmark. Final debug: gameplay 755 passed, rules
  105 passed, no failures. The three additional passing tests are allocation
  assertions added after baseline capture.
- Final release: gameplay 755 passed, rules 105 passed, no failures; two manual
  benchmarks ignored. All 860 tests also pass in debug.
- `cargo check --workspace --bins` passes.
- `cargo clippy -p deadsync-gameplay -p deadsync-rules --all-targets -- -D clippy::perf`
  passes; existing non-performance warnings remain. New/changed routines and
  benchmark files pass formatting checks; `git diff --check` passes.

## Reproduction

```powershell
cargo test -p deadsync-gameplay -p deadsync-rules --lib -- --test-threads=1
cargo test --release -p deadsync-gameplay --lib chart_transform_bench -- --ignored --test-threads=1 --nocapture
cargo test --release -p deadsync-rules --lib histogram_bench -- --ignored --test-threads=1 --nocapture
$env:DEADSYNC_PERF_REVERSE = "1"
# Repeat the two benchmark commands to reverse old/new order.
Remove-Item Env:DEADSYNC_PERF_REVERSE
```

The checked-in benchmarks include both implementations; no historical checkout
or local dataset is required. Build first and run benchmarks serially without
other builds. Behavior/allocation assertions are regression gates; timing is
informational and should be remeasured on deployment hardware.
