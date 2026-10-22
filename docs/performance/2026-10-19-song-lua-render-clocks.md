# Song Lua compiled rendering and material clocks

This pass starts from `5ffd4a261`, following the [vector batch pass](2026-10-19-song-lua-vector-batches.md). It removes repeated camera searches, trigonometry and frame-delay summation from measured hot loops, following M-HOTPATH, M-THROUGHPUT, M-INITIAL-CAPACITY and M-BOX-DST in [rust-performance.md](../../rust-performance.md).

1. Resolve the first camera for each visited player once per render, using two stack slots. Rebuild the cache each render so camera movement and rotation remain current. Empty binding lists still require no camera.
2. Prepare the original four rotation coefficients once per mesh binding and once per visited player camera, then reuse them across points. The forward/inverse arithmetic and projection order are unchanged; collision rotation uses the same helper.
3. Cache sequential `f32` frame-age prefixes in an exactly sized boxed slice for diffuse material clocks. UV reads use the prefix without rescanning prior frames. Single-frame clocks and additive state-only clocks allocate no prefix storage. The latter retain the original UV calculation if sampled directly.

## Measured results

The [raw results and validation receipt](2026-10-19-song-lua-render-clocks.json) retain 54 samples per variant/workload, across six alternating-order rounds. Values are medians before → after. Renderer numbers combine the first two changes and do not attribute separate speedups to each.

| Case | CPU cycles/unit | Allocations/unit | Requested bytes/unit | Units/second |
| --- | ---: | ---: | ---: | ---: |
| 16 balls, one player | 1,288.46 → 460.95 | 0 → 0 | 0 → 0 | 1,692,047 → 4,667,445 |
| 256 balls, two players | 173,153.67 → 26,278.54 | 0 → 0 | 0 → 0 | 12,621 → 83,295 |
| 128 eight-point meshes, two players | 226,666.34 → 144,743.11 | 0 → 0 | 0 → 0 | 9,671 → 15,132 |
| One 4,096-point mesh, two players | 714,098.66 → 497,146.92 | 0 → 0 | 0 → 0 | 3,061 → 4,394 |
| 64 unrotated eight-point meshes, two players | 57,128.30 → 51,933.18 | 0 → 0 | 0 → 0 | 38,322 → 42,016 |

Renderer units are scene frames. All five cases retain zero allocations, zero reallocations and zero requested-byte traffic per timed frame. Throughput gains range from 9.6–560.0%; CPU-cycle reductions range from 9.1–84.8%.

| Case | CPU cycles/unit | Allocations/unit | Requested bytes/unit | Units/second |
| --- | ---: | ---: | ---: | ---: |
| UV read, one frame | 9.80 → 9.26 | 0 → 0 | 0 → 0 | 222,222,222 → 235,155,791 |
| UV read, 64 frames | 80.68 → 9.78 | 0 → 0 | 0 → 0 | 27,003,308 → 222,469,410 |
| UV read, 256 frames | 435.75 → 9.56 | 0 → 0 | 0 → 0 | 5,027,778 → 228,050,171 |
| Update + UV, 64 frames | 45.20 → 14.84 | 0 → 0 | 0 → 0 | 48,443,745 → 147,112,909 |
| Initialize/drop, one frame | 28.88 → 29.80 | 0 → 0 | 0 → 0 | 75,315,383 → 73,421,439 |
| Initialize/drop, 64 frames | 84.94 → 429.58 | 0 → 1 | 0 → 256 | 25,629,525 → 5,104,320 |
| Initialize/drop additive, 64 frames | 82.01 → 82.95 | 0 → 0 | 0 → 0 | 26,677,338 → 26,378,264 |

Clock units are UV reads, update-plus-UV ticks, or complete constructor/drop operations as labeled. Every case has zero reallocations. Hot UV/update cases retain zero allocations. UV-only workloads read the last frame; the replay workload advances by 0.01 seconds per tick and samples UVs.

The one-frame UV result varies across rounds: per-round median throughput changes range from -0.2% to +24.8%. Multi-frame UV reads and update-plus-UV ticks improve in every round, as do all five renderer cases.

The 64-frame diffuse clock adds one 256-byte allocation at initialization, with constructor/drop time rising from 39.0 to 195.9 ns. Single-frame and additive constructor/drop timings are also included in the table; they still carry the larger private clock and cache drop check. This is a setup/steady-state tradeoff: the cache is built once and reused during replay, then freed with the clock. One-frame and additive clock initialization retain zero allocation traffic. The cache also adds a boxed-slice field to the private clock; these requested-byte numbers exclude stack/object-size changes and are not RSS measurements.

## Behavior and validation

Four new release checks pass against frozen code from `5ffd4a261`. Rendering checks cover one/two players, changing camera positions/rotations, duplicate-camera selection, empty scenes, clipping near/behind the camera, balls, varying mesh sizes, animated UV offsets and retained mesh Arcs. Rotation checks compare original and prepared arithmetic for forward/inverse transforms, signed zero, large/small finite angles, infinities and NaNs. Material checks cover 600 update/state steps per configuration, one/many frames, zero/negative/nonfinite delays, clamped state indices, nonfinite deltas and additive construction churn. Finite results compare exact bits where directly sampled; NaNs compare classification, not payload.

The default-feature release library suite has 689 passes, 13 failures and three ignored performance benchmarks. All 13 failures match the preceding commit's normalized diagnostics; its retained suite-log hash matches the committed receipt. No new failures appear. Normalization removes source locations, workspace prefixes, temporary process/thread identifiers and the backtrace hint. The JSON retains each failure name and diagnostic hash, plus source/executable/build-receipt hashes. Frozen renderer/rotation/clock code was checked against `git show 5ffd4a261` after declared receiver/visibility adapters. Native fixtures, acceptance criteria and tolerances are unchanged.

## Reproduction and scope

```powershell
cargo test -p deadsync-song-lua --lib --release --config profile.release.lto=false --no-run --message-format=json
python scripts/benchmark_song_lua_draw_capture.py --binary <test-executable> --baseline-commit 5ffd4a261 --test-filter compiled_scene::runtime::perf::benchmark_render --test-filter model_texture::perf::benchmark_material_clock --out docs/performance/2026-10-19-song-lua-render-clocks.json
```

Run the executable with `--test-threads=1` for the full library suite. Each benchmark uses one warmup plus nine retained samples per variant/workload per round, pinned to logical CPU 2 on Windows, Xeon E5-2696 v4 (44 logical CPUs), rustc 1.98.1, release optimization with LTO disabled. Both variants run in the same executable with the same counted System allocator. Fixture construction is excluded; initialization workloads explicitly include constructor/drop costs. Renderer samples reuse unique mesh Arcs; retained-Arc copy-on-write behavior is checked separately.

Windows QueryThreadCycleTime measures scheduled current-thread CPU cycles including kernel execution, not a user-only hardware PMU counter. Allocations cover current-thread Rust GlobalAlloc, including mlua, and exclude direct native malloc. Timing/cycles include allocation-accounting overhead. No builds or other benchmark runs overlap the measurements. These synthetic renderer and clock cases do not establish whole-song or corpus-wide speedups.
