# Spline work performance 0.5.1684

Parent: `98d79dc83` (0.5.1683). Date: 2026-10-02.

This pass applies `rust-performance.md`'s M-HOTPATH, M-MEM-REUSE,
M-INITIAL-CAPACITY and M-THROUGHPUT guidance to three remaining spline costs:

1. Retain coefficient and diagonal buffers in `SongLuaSplineSolver` across
   changing curves. Warmed borrowed solves allocate nothing. Capture copies
   those coefficients directly into the required immutable Arc, eliminating
   the old temporary output vector and diagonal allocation for each solve.
   The existing owned `solve_song_lua_spline` API remains available and uses
   the same arithmetic kernel.
2. Share the most recently read geometry when all authored coordinate bits
   match. This includes independently read Position/Zoom handlers and adjacent
   lanes. Each Lua field and coordinate is still read and validated, including
   metadata getters, replacements and in-place edits. Sharing covers only
   coefficients; each handler retains its own metadata. Signed zeros retain
   their original authored bits. Preceding-frame matches leave the recent
   entry untouched, avoiding repeated reference-count updates for steady lanes.
   No table identity or global lookup is used.
3. Defer coefficient NaN scans until the first comparison of an identical
   buffer. Changing geometry and metadata-only changes need no eager scan.
   A lane caches unknown/true/false independently for Position and Zoom,
   preserves that state across metadata edits, and resets it when the Arc
   changes. Identical NaN buffers remain non-equal, including coefficients
   that overflow from finite Lua points.

No dependency or unsafe production code was added. Limits remain 65536
points per spline and 128 MiB of logically counted coefficients per layer.
Shared Position/Zoom frames still count both buffers toward that limit, so
sharing does not alter which authored layers exceed the budget.

## Measurement method

Windows 11 Pro 10.0.26100, Intel Xeon E5-2696 v4 (22 cores/44 logical
processors), Rust/Cargo 1.98.1, repository release profile (opt-level 3, full
LTO). Builds and checks finish before timing. Six serial rounds alternate
old-first/new-first. Each measurement contains the median and min/max of
seven timing batches. The [raw CSV](spline-work-0.5.1684.csv) contains 324
measurements across 27 paired workloads. Tables use the median of paired
round percentage changes and show the complete CPU-change range. Negative
reductions mean slower. Windows QueryThreadCycleTime measures calling-thread
CPU cycles. These microbenchmarks do not establish gameplay FPS, GPU or
end-to-end song-load improvements.

Old/new ns/op columns are separate six-round medians. Percentage changes
use paired rounds, so dividing those two summary medians may give a different
ratio when timings vary between rounds.

Allocator accounting uses the repository's thread-local System wrapper in
separate operations after timing. Lua's mlua allocator also routes through
that wrapper. Counts, reallocations, frees, requested bytes and freed bytes
are in the CSV; all measured operations have balanced allocation/free counts
and bytes, with identical counts in all six rounds. Requested bytes measure
allocator traffic, not peak live memory, RSS or allocator metadata.

The solver, complete reader, and complete capture baselines are frozen from
`98d79dc83`. The frozen reader also calls the frozen solver. Lua conversion
and NoteField lookup helpers were unchanged in this pass.

## Reusable solver

One operation solves a prepared curve. Old constructs/drops an owned vector;
new borrows workspace coefficients after warmup. Fixture construction and
workspace growth/destruction are outside measurement. Throughput counts
points. Zero allocation applies to the borrowed API within retained capacity;
an owned result still requires storage.

| Workload | Old -> new ns/op | CPU-cycle reduction (round range) | Throughput increase | Allocations/op | Requested bytes/op |
|---|---:|---:|---:|---:|---:|
| `workspace_2_3axes` | 69.9 -> 15.3 | 78.6% (50.2..81.0) | 384.0% | 1 -> 0 | 96 -> 0 |
| `workspace_32_0axes` | 200.7 -> 128.9 | 33.3% (11.7..53.9) | 52.0% | 1 -> 0 | 1536 -> 0 |
| `workspace_32_1axes` | 602.8 -> 498.1 | 17.5% (11.5..28.4) | 21.3% | 2 -> 0 | 1664 -> 0 |
| `workspace_32_3axes` | 739.9 -> 644.1 | 12.1% (8.4..23.8) | 13.7% | 2 -> 0 | 1664 -> 0 |
| `workspace_256_3axes` | 5585.6 -> 5441.5 | 3.4% (-6.4..6.7) | 3.6% | 2 -> 0 | 13312 -> 0 |
| `workspace_4096_3axes` | 107273.4 -> 101421.9 | 4.3% (-2.4..16.7) | 5.2% | 2 -> 0 | 212992 -> 0 |
| `workspace_65536_3axes` | 2898106.2 -> 2104892.2 | 27.0% (25.3..29.1) | 37.0% | 2 -> 0 | 3407872 -> 0 |

## Validated recent geometry

One operation performs eight complete reads, then Lua collection. Old has no
previous lane to reuse; new can share its recent validated coordinates.
The repeat workloads isolate that use case. Changing workloads alternate
point values and require a new immutable output on each read. Both variants
retain their warmed point scratch outside measurement; new also retains
solver/recent scratch. Edits are included. Throughput counts points read.

| Workload | Old -> new ns/op | CPU-cycle reduction (round range) | Throughput increase | Allocations/op | Requested bytes/op |
|---|---:|---:|---:|---:|---:|
| `geometry_repeat_2` | 14327.8 -> 12690.7 | 12.9% (6.5..17.8) | 14.1% | 16 -> 0 | 1664 -> 0 |
| `geometry_repeat_32` | 69952.4 -> 61197.3 | 12.5% (10.9..14.0) | 14.3% | 24 -> 0 | 25728 -> 0 |
| `geometry_repeat_256` | 473239.1 -> 417810.2 | 10.0% (7.1..13.7) | 11.1% | 24 -> 0 | 204928 -> 0 |
| `geometry_repeat_4096` | 9414306.2 -> 6919256.2 | 27.1% (23.4..29.4) | 37.3% | 24 -> 0 | 3276928 -> 0 |
| `geometry_changing_2` | 16739.0 -> 16448.0 | 1.7% (-0.4..10.8) | 1.8% | 16 -> 8 | 1664 -> 896 |
| `geometry_changing_32` | 70761.7 -> 70754.3 | 1.1% (-0.3..5.1) | 1.2% | 24 -> 8 | 25728 -> 12416 |
| `geometry_changing_256` | 478326.5 -> 478707.0 | -1.8% (-4.6..4.9) | -1.7% | 24 -> 8 | 204928 -> 98432 |
| `geometry_changing_4096` | 9149812.5 -> 8011556.2 | 12.0% (11.6..13.4) | 13.7% | 24 -> 8 | 3276928 -> 1572992 |

## Lazy NaN validation

One operation processes 128 different-buffer cache transitions. Old scans
the fresh buffer; new marks it unknown and defers scanning until identity
comparison actually needs it. Buffers are prepared outside measurement.
Throughput counts transitions. The removed scan is measured directly; it is
a small component of complete Lua capture.

| Workload | Old -> new ns/op | CPU-cycle reduction (round range) | Throughput increase | Allocations/op | Requested bytes/op |
|---|---:|---:|---:|---:|---:|
| `reflexivity_fresh_32` | 15887.5 -> 121.1 | 99.2% (99.0..99.2) | 127.1x | 0 -> 0 | 0 -> 0 |
| `reflexivity_fresh_256` | 115081.3 -> 115.6 | 99.9% (99.9..99.9) | 1000.8x | 0 -> 0 | 0 -> 0 |
| `reflexivity_fresh_65536` | 29512464.9 -> 115.6 | 99.9996% (99.9995..99.9997) | 252453.2x | 0 -> 0 | 0 -> 0 |

## Complete capture

Each operation constructs, captures 120 samples, and destroys the complete
capture storage, then collects Lua garbage. Fixture construction is outside
measurement; edits, buffer growth, required output allocation, deduplication
and destruction are included. Throughput counts lane captures. The fixture
uses the production player actor and identical Position/Zoom geometry;
`distinct` controls use separate, different Zoom points. In changing distinct
workloads, Position changes while Zoom stays fixed.

| Workload | Old -> new ns/op | CPU-cycle reduction (round range) | Throughput increase | Allocations/op | Requested bytes/op |
|---|---:|---:|---:|---:|---:|
| `capture_1x2_steady` | 420862.5 -> 435550.0 | -0.3% (-19.1..5.9) | -0.2% | 9 -> 7 | 1678 -> 1470 |
| `capture_8x32_steady` | 14232775.0 -> 15341756.2 | -0.5% (-20.5..2.1) | -0.4% | 60 -> 22 | 55798 -> 18422 |
| `capture_8x256_steady` | 101977606.2 -> 102483656.2 | -0.5% (-2.9..1.4) | -0.5% | 60 -> 22 | 416886 -> 118774 |
| `capture_1x256_metadata` | 12629068.8 -> 12679343.8 | -0.3% (-12.3..1.9) | -0.3% | 11 -> 8 | 73398 -> 47782 |
| `capture_1x32_changing` | 2439781.2 -> 2098300.0 | 13.7% (10.5..16.7) | 15.9% | 725 -> 127 | 791318 -> 207382 |
| `capture_8x256_changing` | 134540756.2 -> 118562606.2 | 12.6% (-2.2..19.0) | 14.5% | 5772 -> 974 | 49332598 -> 11975030 |
| `capture_8x256_steady_distinct` | 106773143.8 -> 106514075.0 | 0.2% (-3.1..12.1) | 0.2% | 60 -> 30 | 416886 -> 217206 |
| `capture_8x256_changing_distinct` | 119512525.0 -> 118001106.2 | 1.8% (-4.1..3.2) | 1.8% | 2916 -> 982 | 24946166 -> 12073462 |
| `capture_8x0_disabled` | 1141900.0 -> 1176512.5 | -2.0% (-5.7..4.9) | -2.0% | 11 -> 11 | 3958 -> 3958 |

## Findings and controls

The warmed reusable solver uses no allocations and reduced median cycles by
12.1% for 32-point, three-axis curves and 27.0% at 65536 points. The 256-point
and 4096-point solver gains were smaller (3.4% and 4.3%) with slower rounds.
Repeated full reads reduced cycles by 10.0..27.1% for cubic curves and removed
all allocation traffic. Changing reads retain one required Arc instead of
three allocations, with roughly half the requested bytes; 4096-point changing
reads improved 12.0%, while 256-point reads measured 1.8% more cycles and
32-point reads were close to neutral.

Complete changing capture improved 13.7% for one 32-point lane and 12.6% for
eight 256-point lanes. The latter reduced allocator calls by 83.1% and
requested bytes by 75.7%. The changing distinct-geometry control improved
1.8% with a -4.1..3.2% round range, so its CPU result is close to neutral;
allocation calls and requested bytes still declined substantially.

Steady and metadata captures measured 0.3..0.5% more cycles at the paired
median, with mixed round results. One small steady workload had slow outliers
up to 20.5%; the report retains these measurements. Distinct steady geometry
was neutral (+0.2% reduction). Disabled capture measured 2.0% more cycles
(-5.7..4.9% range), with unchanged allocation traffic. These controls do not
support a claim of faster steady or disabled capture. Lazy NaN transitions
remove the eager scan, but an identity comparison that needs the property
still pays its first scan; the kernel's large gain is not a whole-capture gain.

## Memory tradeoffs and behavior coverage

Warmed changing cubic reads need one immutable Arc allocation instead of three
allocations (temporary output, diagonal, immutable Arc); repeated identical
geometry needs none. Capture owns retained point, coefficient and diagonal
buffers until compilation ends. At the 65536-point limit, the newly retained
solver buffers total 3407872 bytes (3.25 MiB). One recent Arc may retain up to
3145744 bytes (3 MiB plus its reference counts); it normally also belongs to
a captured frame but may remain alive after a handler is disabled. Thus
scratch can retain up to roughly 6.25 MiB more than the parent's point scratch
in the worst case. It is released at capture completion; there is no global
cache, locking, eviction or gameplay-time allocation. Equal Position/Zoom
frames share one buffer instead of retaining two in the finished tracks.
The tri-state flags fit in the same lane padding as the parent's booleans.

Tests compare old/new coefficients at growth/shrink boundaries, all varying
axes and the size limit. All non-NaN bits must match, including signed zero,
subnormals and infinities; NaNs must retain classification. Lua tests cover
metadata side effects, invalid points, replacement/resize/mode transitions,
shared and distinct handlers, exact logical byte limits, lazy flag invalidation,
and non-reflexive shared coefficients generated by finite-point overflow.
Allocator assertions cover zero churn in warmed solves/repeated reads and
one required Arc allocation for changed geometry.

Validation passed: 2224 debug tests across gameplay, notefield and song Lua;
1599 release unit tests across gameplay and song Lua; both native C++ spline
parity integration tests; formatting checks; and Clippy for the affected crates.
After the final recent-reference adjustment, all 923 Lua debug tests and both
native parity checks passed again, followed by the final release test run.
Ignored tests are manual benchmark entry points (95 debug / 88 release).
Clippy reports existing repository warnings; none refer to the new workspace
or recent-geometry helpers and their performance modules.

## Reproduction

```powershell
cargo test -p deadsync-gameplay -p deadsync-song-lua -p deadsync-notefield --locked
cargo clippy -p deadsync-gameplay -p deadsync-song-lua -p deadsync-notefield --all-targets --locked
cargo test -p deadsync --test song_lua_itgmania_semantic_parity position_spline --locked
cargo test --release -p deadsync-gameplay -p deadsync-song-lua --lib --locked
cargo test --release -p deadsync-gameplay --lib spline_workspace_perf::benchmark_spline_workspace --locked -- --ignored --exact --nocapture --test-threads=1
cargo test --release -p deadsync-song-lua --lib lua_util::spline_workspace_reader_perf::benchmark_spline_geometry_reuse --locked -- --ignored --exact --nocapture --test-threads=1
cargo test --release -p deadsync-song-lua --lib perframe::spline_workspace_capture_perf::benchmark_spline_capture_workspace --locked -- --ignored --exact --nocapture --test-threads=1
```

Repeat benchmarks in six serial rounds, setting `DEADSYNC_PERF_REVERSE=1`
for even rounds and removing it for odd rounds. Build before timing and avoid
running compilation/tests concurrently with benchmarks.
