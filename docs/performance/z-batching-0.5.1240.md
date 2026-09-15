# Logical-Z batch boundaries - 0.5.1240

Baseline: `4e8446163` (`fix(songs): refresh stale caches on Reload Songs/Courses`).
Measured on 2026-09-15. The sprite and textured-mesh logical-Z compatibility
conditions in `finish_frame` are removed. Sorting still establishes painter order;
texture, blend, camera, draw kind, geometry identity, depth state, and contiguous
sprite instance requirements remain in place.

## Why the untextured-mesh guard stays

The original three-condition proposal is not safe for every accepted mesh input.
An untextured mesh can contain one or two trailing vertices. A draw call ignores
those incomplete triangles. Combining two runs can instead assemble them into
a new triangle, or change the next run's triangle grouping.

For example, one vertex at logical Z=0 and two vertices at logical Z=1 previously
produce no triangles. Removing the mesh Z condition joins them into one draw and
produces a triangle. The new regression test failed with that removal, and a
software pixel test confirms that this can create visible pixels.

The untextured-mesh Z condition therefore remains, with a comment explaining its
purpose. Sprite and textured-mesh instances preserve primitive boundaries when
combined, so their Z conditions can be removed. A future mesh change would need
to address incomplete triangle boundaries explicitly rather than simply remove
the condition. This patch preserves the previous behavior for those inputs.

## Result

The compatible sprite and textured-mesh fixtures produce 512 -> 1 or 16 -> 1
finalizer draw runs. All untextured-mesh and texture/camera/contiguity controls
keep their old run counts. The mixed fixture produces 1,214 -> 1,081 runs.
These counts precede the optional sprite-gather pass.

CPU results are workload-dependent. In the five-process confirmation series,
the ordinary 16-layer sprite fixture uses 9.0% fewer median thread cycles and
processes 496.7 -> 578.0 million input items/s. The 512-layer sprite fixture gains
more, with significant order sensitivity described below. Textured-mesh CPU
changes are small. The unchanged layered-mesh control is slower: +7.3% median
cycles in ordinary finalization and +4.4% with sprite-run tracking. These results
do not establish an across-the-board CPU improvement.

**Allocation churn is unchanged at zero:** all 416 observations report zero
allocations, reallocations, frees, allocated bytes, and freed bytes. Input/output
capacity is prepared before measurement. This does not describe cold frame
construction or the allocations elsewhere in composition/rendering.

**Scope:** timings measure `finish_frame`, including vertex transformation and
geometry lookup where applicable. They exclude input construction, sorting,
sprite gathering, fixture destruction, driver submission, and GPU work. Throughput
means input draw items finalized per second, not game FPS. Fewer final draw runs
can save backend work, but that saving was not timed here.

## Measurements

Each operation finalizes 2,048 input items. Values below are medians of five
process medians from the confirmation series (affinity mask 16). Cycle change is `(new / old - 1)`; negative values are better.
The two modes instantiate the same production template with sprite-run tracking
turned off (ordinary already-ordered input) or on (used after fallback sorting).
The benchmark supplies already-sorted input in both modes.

### Ordinary finalization

| Fixture | Draw runs old -> new | us/op old -> new | Thread cycles/op old -> new | Cycle change | Million items/s old -> new |
|---|---:|---:|---:|---:|---:|
| `sprites_layers` | 512 -> 1 | 11.36 -> 3.55 | 26,440 -> 9,529 | -64.0% | 180.4 -> 577.5 |
| `sprites_16layers` | 16 -> 1 | 4.12 -> 3.54 | 10,543 -> 9,590 | -9.0% | 496.7 -> 578.0 |
| `sprites_same_z` | 1 -> 1 | 3.82 -> 3.52 | 10,205 -> 9,324 | -8.6% | 535.7 -> 582.3 |
| `sprites_textures` | 2048 -> 2048 | 13.16 -> 12.48 | 30,646 -> 28,927 | -5.6% | 155.6 -> 164.1 |
| `sprites_fragmented` | 2048 -> 2048 | 16.27 -> 16.10 | 37,911 -> 36,800 | -2.9% | 125.9 -> 127.2 |
| `meshes_layers` | 512 -> 512 | 52.00 -> 55.43 | 114,139 -> 122,513 | +7.3% | 39.4 -> 36.9 |
| `meshes_same_z` | 1 -> 1 | 47.58 -> 48.03 | 106,125 -> 106,783 | +0.6% | 43.0 -> 42.6 |
| `meshes_cameras` | 2048 -> 2048 | 57.55 -> 58.85 | 128,178 -> 130,712 | +2.0% | 35.6 -> 34.8 |
| `tmesh_layers` | 512 -> 1 | 69.10 -> 67.27 | 152,209 -> 149,366 | -1.9% | 29.6 -> 30.4 |
| `tmesh_16layers` | 16 -> 1 | 68.54 -> 67.58 | 152,106 -> 149,681 | -1.6% | 29.9 -> 30.3 |
| `tmesh_depth` | 512 -> 1 | 68.85 -> 68.64 | 152,718 -> 151,365 | -0.9% | 29.7 -> 29.8 |
| `tmesh_cameras` | 2048 -> 2048 | 75.34 -> 74.59 | 167,124 -> 165,106 | -1.2% | 27.2 -> 27.5 |
| `mixed` | 1214 -> 1081 | 44.26 -> 43.23 | 98,910 -> 96,419 | -2.5% | 46.3 -> 47.4 |

### Finalization with sprite-run tracking

| Fixture | Draw runs old -> new | us/op old -> new | Thread cycles/op old -> new | Cycle change | Million items/s old -> new |
|---|---:|---:|---:|---:|---:|
| `sprites_layers` | 512 -> 1 | 6.41 -> 3.75 | 15,671 -> 9,642 | -38.5% | 319.3 -> 546.0 |
| `sprites_16layers` | 16 -> 1 | 4.04 -> 3.87 | 10,474 -> 9,966 | -4.9% | 506.7 -> 529.7 |
| `sprites_same_z` | 1 -> 1 | 3.79 -> 3.71 | 9,896 -> 9,721 | -1.8% | 540.4 -> 552.5 |
| `sprites_textures` | 2048 -> 2048 | 17.20 -> 17.22 | 39,598 -> 39,316 | -0.7% | 119.1 -> 119.0 |
| `sprites_fragmented` | 2048 -> 2048 | 18.50 -> 18.59 | 42,235 -> 42,637 | +1.0% | 110.7 -> 110.2 |
| `meshes_layers` | 512 -> 512 | 49.49 -> 51.81 | 110,636 -> 115,538 | +4.4% | 41.4 -> 39.5 |
| `meshes_same_z` | 1 -> 1 | 50.14 -> 49.67 | 111,780 -> 110,385 | -1.2% | 40.8 -> 41.2 |
| `meshes_cameras` | 2048 -> 2048 | 58.05 -> 57.28 | 129,245 -> 127,234 | -1.6% | 35.3 -> 35.8 |
| `tmesh_layers` | 512 -> 1 | 69.73 -> 68.33 | 155,359 -> 151,723 | -2.3% | 29.4 -> 30.0 |
| `tmesh_16layers` | 16 -> 1 | 68.97 -> 68.41 | 152,824 -> 152,120 | -0.5% | 29.7 -> 29.9 |
| `tmesh_depth` | 512 -> 1 | 69.96 -> 67.57 | 155,560 -> 150,213 | -3.4% | 29.3 -> 30.3 |
| `tmesh_cameras` | 2048 -> 2048 | 74.82 -> 76.81 | 165,318 -> 169,482 | +2.5% | 27.4 -> 26.7 |
| `mixed` | 1214 -> 1081 | 46.01 -> 44.44 | 102,630 -> 99,350 | -3.2% | 44.5 -> 46.1 |

### Variation and interpretation

An initial three-process series used logical processor 1 (affinity mask 2).
Some controls varied substantially: its ordinary layered-mesh comparison showed
+20.2% median cycles despite identical mesh logic and draw counts. A separate
five-process series used logical processor 4 (mask 16) to check that result.
All eight processes use the same executable. Both series remain in the raw CSV,
identified by affinity mask; the table uses the five-process series throughout.
The confirmation series still shows a layered-mesh regression (+7.3% ordinary,
+4.4% tracked), so it is not dismissed as measurement noise.

Ordinary 512-layer sprite process medians range from 5.32-11.80 us
old to 3.35-3.62 us new. Paired cycle reductions range from
27.0% to 66.5%, with 1.49x-3.39x throughput.

Tracked 512-layer sprite process medians range from 6.15-6.90 us
old to 3.62-3.86 us new. Paired cycle reductions range from
35.5% to 42.4%, with 1.66x-1.84x throughput.

The first ordinary sprite workload is particularly order-sensitive: old-first
and new-first runs should not be interpreted as one stable speedup. The tracked
fixture is more consistent. Small differences in other cases also overlap sample
variation. No whole-application FPS or driver-submission improvement was measured.
All observations, ranges, and regressions are retained in
[the raw CSV](z-batching-0.5.1240.csv).

### Fixture details

- `*_layers`: four input items per logical layer, 512 layers total.
- `*_16layers`: 128 input items per layer, 16 layers total.
- `*_same_z`: a single logical layer; the old code already emits one run.
- `*_textures` and `*_cameras`: alternating state prevents coalescing.
- `sprites_fragmented`: each pair of instance indices is reversed, preventing
  contiguous runs even though texture and blend match.
- `tmesh_depth`: all instances use depth testing and shared geometry.
- `mixed`: 16-item groups alternate sprites, meshes, and textured meshes, with
  periodic blend and depth-state changes.
- Mesh input consists of shared triangles. Textured meshes share geometry and
  vary transform, tint, and texture-mask values. Source geometry remains alive,
  matching actor ownership during finalization.

### Method

- Windows x86-64, Intel Xeon E5-2696 v4 at 2.20 GHz, 44 logical processors.
- Rust 1.98.1 (`48a229cea`), LLVM 22.1.8, repository release settings with full LTO.
- Frozen old `finish_frame` and current production `finish_frame` compile into
  the same optimized test executable. The frozen body was checked against
  `4e8446163`, allowing only whitespace and test visibility changes.
- Three initial processes pinned to logical processor 1 (affinity mask 2), then
  five confirmation processes pinned to logical processor 4 (mask 16). Even runs
  reverse old/new order. No builds or other tests ran during measurements.
  The machine was not otherwise reserved exclusively.
- Per variant: three warmups and seven samples of 256 operations, each with fresh
  input. Setup and destruction are outside timing and allocation accounting.
- Windows `QueryThreadCycleTime` records calling-thread cycles. This is an OS
  thread-cycle measurement, not a hardware instructions-retired counter. Timer
  overhead is included equally in both variants. Wall time determines throughput;
  cycle and time medians are computed separately.
- Allocator counters run separately for one operation per variant per process.
  They count allocation churn, not peak memory or retained capacity.
- Executable SHA-256:
  `9208173b2ca0d40ce0c033474668367d29ee133f715ea50c87e52efd62d594b9`.
- Raw logs remain in `target/z-batching-safe-perf/run-{1,2,3}.log` and
  `target/z-batching-safe-perf-cpu4/run-{1,2,3,4,5}.log`. The CSV preserves all
  416 rows from the final, safe implementation. Earlier exploratory measurements
  of the unsafe three-condition removal are not used in this report.

## Correctness

The semantic frame comparator expands sprites into instances, meshes into
triangles, and textured meshes into instances with their geometry and state.
It checks those sequences in painter order, including offscreen passes, while
allowing batch boundaries and data offsets to differ. The strict comparator
continues to compare exact segmentation.

Tests compare frozen/current finalizers in both tracking modes and cover texture,
blend, camera, draw-kind, geometry, depth, and noncontiguous-instance boundaries.
Negative tests alter order, vertex data, instance data, cameras, blends, depth,
and render-target metadata. A sorting/gathering test verifies both the case where
the old code gathers but the new code no longer needs to, and the case where
both still gather fragmented instances.

The existing textured-mesh shadow test now expects one run containing shadow then
original. Their instance tints and shared geometry cache key are unchanged.

Pixel fixtures compare split/combined batches on overlapping translucent draws,
additive blending, texture-mask glow, multiple cameras, and offscreen targets.
Tests verify visible content, and depth-enabled fixtures must differ from their
depth-disabled equivalent on backends that implement depth testing.

| Backend | Result |
|---|---|
| Native OpenGL | 28 exact before/after pixel comparisons passed; depth rejection checked |
| Native Vulkan | 28 exact comparisons passed; existing backend ignores `depth_test` |
| Vulkan via wgpu | 28 exact comparisons passed; depth rejection checked |
| DirectX via wgpu | 28 exact comparisons passed; depth rejection checked |
| Software | 48 direct/staged raster and offscreen-pass comparisons passed; no depth testing |
| OpenGL via wgpu | Screenshot readback unavailable on this machine; pixels unverified |
| Native Metal / Metal via wgpu | Unavailable on this Windows host; untested |

GPU: NVIDIA GeForce GTX 1650 SUPER, driver 32.0.15.9186. Pixel equality is checked
within each backend; different backends are not required to produce identical
pixels. The software check invokes the production raster paths directly because
its window backend has no screenshot implementation. It checks offscreen pass
content, not the complete target-texture sampling path.

Validation completed:

- `cargo test -p deadlib-present -p deadlib-render-core -p deadlib-render-backend-software --locked --quiet`:
  502 test executions passed, 12 ignored including manual benchmarks/doctests.
  Private-source test harnesses include some existing unit tests again.
- Four separate GPU test processes passed (one per available capture backend).
- Eight release benchmark processes passed their expanded-output comparisons.
- `cargo check --locked --quiet` passed for the application.
- Scoped `rustfmt --check`, `git diff --check`, and frozen-baseline verification passed.

## Reproduce

```powershell
cargo test -p deadlib-present -p deadlib-render-core -p deadlib-render-backend-software --locked
cargo test -p deadlib-present --test z_batching --release --locked --no-run
cargo test -p deadlib-present --test z_batching --release --locked z_batching_bench -- --ignored --nocapture --test-threads=1
```

For the confirmation series, run the printed release executable directly in five
fresh processes with affinity mask 16. Set `DEADSYNC_PERF_REVERSE=1` only in even
processes. Run builds and regression tests separately from measurements.

```powershell
cargo test -p deadlib-render --test batching --locked --no-run
foreach ($kind in @('opengl', 'vulkan', 'vulkan-wgpu', 'directx')) {
    $env:DEADSYNC_BATCH_BACKEND = $kind
    cargo test -p deadlib-render --test batching --locked -- --ignored --nocapture --test-threads=1
    if ($LASTEXITCODE -ne 0) { throw "Pixel comparison failed: $kind" }
}
Remove-Item Env:DEADSYNC_BATCH_BACKEND
```

The GPU test defaults to OpenGL if `DEADSYNC_BATCH_BACKEND` is unset. It is ignored
by ordinary test runs because it requires a graphics device and window system.
