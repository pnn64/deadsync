# Geometry resolution without a cache preflight - 0.5.1241

Baseline: `8b0c3004f`. Measured on 2026-09-15.

## Change

Remove the all-cached preflight in `resolve_textured_mesh_geometries`, the private
`all_cached` flag, and its initialization/update bookkeeping: 22 runtime lines.
The existing per-slot loop already skips unchanged retained sources, copies
transient vertices, and retries cache admission when needed. Buffer preparation,
source ordering, cache keys, and backend cache policies stay the same.

If the preflight succeeded, every slot satisfies the main loop's existing
`continue` condition. If it failed, the old code ran that same main loop anyway.
This removes duplicate validation of a cached prefix when a later slot changes.
Fully stable lists still need one validation traversal; their former early-return
path was slightly cheaper in some benchmarks.

## Results

For a changed final slot, the five-process medians show:

- 16 geometries: 31.6% fewer thread cycles, 1.49x throughput.
- 64 geometries: 45.9% fewer thread cycles, 1.85x throughput.
- 256 geometries: 50.0% fewer thread cycles, 2.00x throughput.

Stable lists of 16/64/256 geometries take approximately 6/6/10 ns more per call
(+13.2%/+4.6%/+1.7% median thread cycles). Changing the first slot and mixed
cached/transient frames also regress in some cases. This is a smaller resolver
with a clear benefit for changes near the end, not a universal CPU speedup.

All 210 allocation-counted observations report **zero allocations, reallocations,
frees, allocated bytes, and freed bytes**. These are warmed resolver operations;
input creation, initial capacity, backend cache allocation, and fixture destruction
are outside the measurements.

### All workloads

Each operation resolves the indicated number of geometries. Cycle change is
`(new / old - 1)`; negative values are better. Results are medians of five process
medians. Throughput is millions of geometries/s, except the empty case, which uses
millions of resolver calls/s.

| Case | Geometries | ns/op old -> new | Thread cycles/op old -> new | Cycle change | Million units/s old -> new |
|---|---:|---:|---:|---:|---:|
| Stable | 0 | 6.5 -> 6.8 | 14.6 -> 15.3 | +4.8% | 154.0 -> 147.9 |
| Stable | 1 | 8.3 -> 8.0 | 18.5 -> 18.0 | -2.7% | 121.2 -> 124.5 |
| ChangeFirst | 1 | 10.7 -> 8.4 | 23.8 -> 18.7 | -21.4% | 93.5 -> 119.4 |
| ChangeLast | 1 | 11.7 -> 8.9 | 26.0 -> 19.8 | -23.8% | 85.7 -> 112.8 |
| Mixed | 1 | 17.2 -> 14.7 | 38.2 -> 32.6 | -14.7% | 58.0 -> 67.9 |
| Misses | 1 | 17.6 -> 16.5 | 39.0 -> 36.5 | -6.4% | 56.8 -> 60.6 |
| Stable | 16 | 43.7 -> 49.5 | 96.2 -> 108.9 | +13.2% | 366.5 -> 323.5 |
| ChangeFirst | 16 | 42.3 -> 44.5 | 93.1 -> 97.9 | +5.2% | 378.4 -> 359.9 |
| ChangeLast | 16 | 74.2 -> 50.0 | 160.9 -> 110.0 | -31.6% | 215.6 -> 320.2 |
| Mixed | 16 | 65.2 -> 67.7 | 143.5 -> 149.3 | +4.0% | 245.4 -> 236.3 |
| Misses | 16 | 163.5 -> 160.9 | 358.2 -> 352.8 | -1.5% | 97.8 -> 99.4 |
| Stable | 64 | 146.4 -> 152.8 | 320.8 -> 335.7 | +4.6% | 437.1 -> 418.8 |
| ChangeFirst | 64 | 172.3 -> 185.2 | 378.6 -> 406.8 | +7.4% | 371.4 -> 345.6 |
| ChangeLast | 64 | 304.0 -> 164.4 | 667.7 -> 361.5 | -45.9% | 210.5 -> 389.3 |
| Mixed | 64 | 289.9 -> 297.9 | 637.1 -> 653.5 | +2.6% | 220.7 -> 214.8 |
| Misses | 64 | 847.9 -> 820.9 | 1860.0 -> 1800.0 | -3.2% | 75.5 -> 78.0 |
| Stable | 256 | 589.3 -> 599.7 | 1292.7 -> 1315.2 | +1.7% | 434.4 -> 426.9 |
| ChangeFirst | 256 | 607.4 -> 620.5 | 1328.4 -> 1328.5 | +0.0% | 421.5 -> 412.6 |
| ChangeLast | 256 | 1213.2 -> 606.4 | 2661.4 -> 1330.7 | -50.0% | 211.0 -> 422.1 |
| Mixed | 256 | 1127.0 -> 1181.4 | 2471.1 -> 2587.7 | +4.7% | 227.1 -> 216.7 |
| Misses | 256 | 3499.9 -> 3431.6 | 7650.6 -> 7513.7 | -1.8% | 73.1 -> 74.6 |

## Method

- Windows x86-64, Intel Xeon E5-2696 v4 at 2.20 GHz (22 cores, 44 logical processors).
- Rust 1.98.1 (`48a229cea`), LLVM 22.1.8, repository release profile with full LTO.
- Frozen original resolver/storage and production resolver compile into the same
  optimized test executable. The frozen implementation was checked against
  `8b0c3004f`, ignoring whitespace.
- Five fresh processes; even runs reverse old/new order. The benchmark thread
  pins itself to logical processor 4 (affinity mask 16) before fixture setup or
  measurement. No builds or other tests ran during the recorded measurements;
  the machine was not otherwise reserved exclusively.
- Each variant warms its output buffers and resolves once before measurement.
  The existing helper then performs three warmups and seven timed samples of
  4,096 operations. Allocation counters run separately for one operation.
- `QueryThreadCycleTime` measures calling-thread cycles; wall time determines
  throughput. These are OS thread-cycle measurements, not instructions retired.
  Timing/cycle medians are computed separately. Very small differences and tiny
  empty/single-entry workloads are sensitive to measurement variation.
- `Stable`: unchanged cached entries. `ChangeFirst` / `ChangeLast`: alternate the
  corresponding slot between two nonzero keys on every operation. `Mixed`: every
  fourth geometry is transient. `Misses`: admission is rejected for every entry
  and retried on subsequent operations. Each geometry has six vertices.
- The cache callback is a deterministic, allocation-free stand-in for retained
  backend buffers or rejected admission. This isolates resolver validation and
  transient copying; it does not measure backend hash lookups, GPU allocations,
  command submission, rendering, or game FPS.
- Executable SHA-256:
  `20d8e09244ba6bda1312e5d0870afbeb64b5358f405cfe9d178fced003821972`.
- Raw observations, including per-process sample ranges, are retained in
  [geometry-resolution-0.5.1241.csv](geometry-resolution-0.5.1241.csv).
  Local terminal logs are `target/geometry-resolution-perf/run-{1,2,3,4,5}.log`.

## Behavior checks

- All existing render-core tests pass, including stable cache reuse, mixed
  transient/cached geometry, rejected admission followed by promotion, changed
  keys/counts, reordered slots, empty/shrinking lists, and concatenated passes.
- A new test compares the frozen/current resolvers over 8,192 frame transitions.
  It checks exact vertex bytes (including generated non-finite values), resolved
  sources, cache callback arguments/order, and public output-buffer capacities.
  Future frame transitions also exercise reuse of the stored cache metadata.
- Presentation and native OpenGL, Vulkan, and wgpu backend tests pass:
  `cargo test -p deadlib-render-core -p deadlib-present -p deadlib-render-backend-gl -p deadlib-render-backend-vulkan -p deadlib-render-backend-wgpu --features deadlib-render-core/test-util --locked --quiet`.
  491 test executions passed; 14 optional benchmarks/device tests/doctests were
  ignored. Some presentation harnesses include existing unit tests again.
- `cargo check --locked --quiet` passes for the application.
- Scoped `rustfmt --check`, `git diff --check`, and baseline verification pass.
- This change does not modify draw commands, shaders, or backend submission.
  This run verifies CPU outputs and cache requests; no new pixel capture was made.

## Reproduce

```powershell
cargo test -p deadlib-render-core --features test-util --locked
cargo test -p deadlib-render-core --test geometry_resolution --release --locked --no-run
$env:DEADSYNC_PERF_AFFINITY = '16'
cargo test -p deadlib-render-core --test geometry_resolution --release --locked geometry_resolution_bench -- --ignored --nocapture --test-threads=1
Remove-Item Env:DEADSYNC_PERF_AFFINITY
```

For the five-process comparison, run the printed release executable directly five
times after compilation. Set `DEADSYNC_PERF_REVERSE=1` only for even runs and clear
it for odd runs. Keep `DEADSYNC_PERF_AFFINITY=16` for all runs on a host with that
logical processor available. Omit the affinity variable for an unpinned run.
Builds and regression tests should run separately from measurements.
