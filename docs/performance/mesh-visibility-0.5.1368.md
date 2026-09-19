# Skip unused textured-actor preparation

Baseline: `adfd8870b`. Reviewed presentation, shared geometry resolution, software
projection/rasterization, and the native GL/Vulkan/Metal and wgpu draw/upload
loops. This change is confined to `deadlib-present`.

Textured actors with neither a positive diffuse alpha nor a glow alpha above
0.0001 now skip placement, matrix construction, texture resolution, and unused
draw metadata. They still consume their original position in painter order.
The original comparisons preserve the handling of NaNs and signed zero. No
cache, allocation, public API, or geometry arithmetic was added.

The frozen function in `tests/mesh_visibility/baseline.rs` and its replacement
are compared across alpha/glow boundaries, NaNs, shared/reusable geometry, and
saturating order counters. A following visible actor checks texture resolution
and ordering after the skip. Debug and release comparisons pass. The final
presentation library and integration test run passes; the rendering CPU suites
also passed during this review. `cargo clippy --locked -p deadlib-present
-p deadlib-render-backend-software --lib -- -D clippy::perf` passes with existing
non-performance warnings.

## Measurements

Windows x86-64, Intel Core i7-1250U, Rust 1.97.1, the repository's release profile
(optimization level 3, full LTO), no added target-CPU flags. Benchmarks inherit
affinity mask 4 (logical CPU 2), run serially, and have no concurrent build/test
jobs from this review. Five independent invocations alternate old/new order.
Each uses the existing `tests/support/perf.rs` helper: three warmups, seven timed
batches, then a separate allocation-counted operation. One operation composes
256 actors using warmed output vectors and the existing texture lookup cache;
timings include input barriers and clearing/dropping the emitted payloads.

| Fixture | Old median ns / 256 | New median ns / 256 | Median paired time reduction | Range across five pairs |
| --- | ---: | ---: | ---: | ---: |
| Hidden | 4,266.2 | 2,186.1 | 48.8% | 48.8% to 50.3% |
| Diffuse only | 9,767.3 | 9,903.7 | -1.5% | -1.8% to -1.4% |
| Glow only | 9,826.4 | 9,949.6 | -1.3% | -3.0% to -0.7% |

Times are medians of the five invocation medians. Percentages are medians of
the five paired reductions, so they need not equal the ratio of the displayed
times. All variants have zero warmed allocation/reallocation/free calls.
Calling-thread cycle reductions were 48.8%, -1.5%, and -1.3%, respectively.
Per-invocation medians and sample ranges are in
[`mesh-visibility-0.5.1368.csv`](mesh-visibility-0.5.1368.csv).

This saves about 8 ns per hidden actor in this fixture, with a measured cost of
about 0.5 ns per visible actor. The additional visibility guard has a real small
cost on these visible controls. Whether a scene benefits depends on its hidden
actor count; this is not an overall renderer or frame-rate improvement claim.
An earlier guard arrangement had a less favorable visible-path result and was
simplified before these final measurements.

Reproduce:

```powershell
cargo test --locked -p deadlib-present --lib --test mesh_visibility
cargo test --locked --release -p deadlib-present --test mesh_visibility --no-run
# Run the emitted executable after all builds finish. Children inherit affinity.
(Get-Process -Id $PID).ProcessorAffinity = 4
& target/release/deps/mesh_visibility-<hash>.exe mesh_visibility_bench --ignored --nocapture --test-threads=1
# Repeat five times, setting this only for invocations 2 and 4:
$env:DEADSYNC_PERF_REVERSE = '1'
```
