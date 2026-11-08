Textured-mesh triangle clipping now selects an FMA-enabled helper on supported x86 CPUs. The helper inlines the existing edge interpolation and polygon clipping, allowing LLVM to combine eight scalar fused operations into a packed FMA instruction. The portable path keeps its compact edge calls; empty and incomplete triangles return before feature detection. The clipping algorithm, triangle order, interpolation formulas and vertex-buffer reuse remain unchanged. There is no inline assembly, explicit intrinsic, approximation, global CPU-feature requirement or new allocation.

This is CPU presentation composition in `deadlib-present`, outside the software renderer and Lua. It follows the bounds-scan optimization in `5de01cdf4`. Bounds culling can bypass this kernel for fully contained or rejected meshes, so gains from those isolated controls are not additive gains on top of the previous optimization. Partial clipping is the primary workload of interest.

The integrated Cargo release binaries measure **1.884x** faster partial clipping at 96 vertices and **1.869x** at 1536 vertices, reducing CPU cost by about **46.9%** and **46.5%**, respectively. These are complete clipping-helper measurements, including the returned object and vertex-buffer reuse, not full composition, GPU or application frame time. The [integrated raw data](2026-11-09-mesh-clip-fma-raw.csv) retains all 2160 observations; the [summary](2026-11-09-mesh-clip-fma-summary.csv) includes every workload and process range.

| Workload | Before cycles/call | After cycles/call | Median speedup | Six-pair range |
| --- | ---: | ---: | ---: | ---: |
| contained-0 | 96.8 | 61.0 | 1.557x | 1.494-1.625x |
| partial-0 | 97.8 | 62.1 | 1.548x | 1.511-1.615x |
| rejected-0 | 96.7 | 62.4 | 1.531x | 1.434-1.564x |
| contained-3 | 286.8 | 214.8 | 1.338x | 1.292-1.396x |
| partial-3 | 475.1 | 288.6 | 1.641x | 1.596-1.675x |
| rejected-3 | 143.3 | 114.1 | 1.262x | 1.207-1.317x |
| contained-6 | 400.8 | 282.2 | 1.427x | 1.355-1.482x |
| partial-6 | 1,172.6 | 690.0 | 1.706x | 1.548-1.745x |
| rejected-6 | 190.1 | 133.0 | 1.415x | 1.349-1.527x |
| contained-96 | 3,915.7 | 2,230.4 | 1.756x | 1.721-1.796x |
| partial-96 | 19,837.9 | 10,588.7 | 1.884x | 1.847-1.934x |
| rejected-96 | 1,518.1 | 672.1 | 2.322x | 2.100-2.384x |
| contained-1536 | 60,639.4 | 34,984.9 | 1.727x | 1.622-1.754x |
| partial-1536 | 332,902.1 | 179,056.3 | 1.869x | 1.814-1.901x |
| rejected-1536 | 22,801.0 | 10,292.9 | 2.211x | 2.058-2.352x |
| perspective-partial-6 | 1,104.7 | 682.6 | 1.636x | 1.591-1.645x |
| perspective-partial-96 | 19,401.5 | 10,982.5 | 1.775x | 1.695-1.796x |
| perspective-partial-1536 | 330,578.8 | 188,449.2 | 1.753x | 1.718-1.844x |

All integrated workload medians and observed pair ratios improve. The early empty-input return also avoids the old helper's setup cost. This is evidence for the measured workloads on this machine; observed extrema are not confidence intervals or guarantees on other hardware.

The preceding extracted prototype measured **2.288x** at 96 partially clipped vertices and **2.292x** at 1536. Its wrapper returned emitted vertices while eliding constant output-instance metadata and fixing material parameters. The integrated harness passes material settings through `black_box` and retains the full returned object through `black_box`; its lower measured gain is the production result reported above. The [prototype raw data](2026-11-09-mesh-clip-fma-prototype-raw.csv), [runtime summary](2026-11-09-mesh-clip-fma-prototype-summary.csv) and [portable summary](2026-11-09-mesh-clip-fma-prototype-portable-summary.csv) remain separate. Prototype forced-portable medians range from 0.956x to 1.027x, with all process ranges crossing parity. Compiling detection false on an FMA-capable machine does not measure an older processor or its detection overhead. The prototype's rejected single-triangle case was slower (0.867x, approximately 6.5 ns extra); the integrated equivalent improves 1.262x. Both results are retained.

An earlier prototype inlined the portable polygon path as well and regressed contained portable workloads by about 13%. The committed structure expands the edge loop only for the FMA path. Its portable wrapper remains out of line. The rejected prototype data remains in the local audit directory.

The audited prototype assembly contains eight `callq fmaf` instructions for one edge intersection: two position components, two UV components and four color components. The specialized body uses one packed `vfmadd213ps` on a YMM register for these eight interpolations, plus hardware FMA for the position and UV transforms. The specialized clipping body has zero `fmaf` calls. Eleven production kernel bodies match the measured prototype after ignoring formatting/comments and removal of a redundant length test after the early return. Local assembly snippets and the full assembly are retained in the audit directory.

The crate still denies unsafe code by default. Only the dispatcher and private feature-specific helper add local exceptions. The call is guarded by `is_x86_feature_detected!("fma")`, which checks CPU and OS support. The specialized body uses ordinary slice, vector and arithmetic operations; no unchecked memory access is added. A separate test-only allowance covers the existing Windows affinity/cycle-counter helper.

**NaN payload/sign bits in emitted vertices can differ.** This is the qualification approved with the prototype, and it is documented next to the specialized helper. Every non-NaN output value, including signed zero and infinity, must match by bits in the regression; NaNs must retain their classification. Output lengths, ordering and metadata also match. This is not a claim of bit-identical output for every IEEE input. The new release regression records 939 NaN-bit differences across its runtime and portable comparisons; the unoptimized build records none. The 3360 completed-frame comparisons below use finite fixtures and match every output bit.

Validation:

- **195 baseline and 195 updated library release tests pass**, with nine opt-in benchmarks ignored during ordinary runs. All twelve separate archived-binary benchmark invocations pass.
- **173 masked-render integration tests pass**, including allocation/churn, recycling, clipping, mask ties and clipped-text tests. These include shared library-module tests, so the two suite counts are not counts of wholly independent cases.
- The new kernel differential covers **70192 geometry/transform/mask cases with two material settings each**: 140384 inputs, each compared on runtime-selected and explicitly portable production paths against frozen clipping code. It verifies output vertex bits subject only to the NaN qualification, instance metadata/flags, untouched pool entries and reuse of the preallocated vertex buffer. It covers incomplete/tail lengths, perspective/affine/singular/random transforms, contained/partial/rejected/zero/infinite masks, random float bit patterns, signed zero, NaNs, infinities, subnormals, varied UV parameters, tint and texture-mask flags.
- The new frame differential verifies **3360 completed render frames bit-for-bit**, including retained storage identity. It exercises production clipping and frame finalization with shared/reusable/transient geometry, preserved prefixes, nested clipping, empty/invalid/overlapping/duplicate masks and ordinary/reflected/singular/perspective/offscreen transforms. Twelve reference function bodies, including the caller that enters triangle clipping, are frozen from `5de01cdf4`; source verification confirms they match that baseline. Unchanged bounds, transforms and frame finalization remain shared production routines.
- Both new regressions also pass at **opt-level 0 with debug assertions and overflow checks**. This directly compiles the production presentation crate and committed tests with cached release dependencies and fat LTO. It is not a full Cargo debug build.
- `cargo check --release` and Clippy for all crate targets complete. Existing warnings remain, including the existing many-argument signature of the dispatch entry; the newly introduced helpers and test/reference modules have no diagnostics. New files and changed production hunks are formatted. Crate-wide `cargo fmt --check` still reports pre-existing differences in `actors.rs`, other portions of `compose.rs` and `font_cache_original.rs`. Scoped and staged whitespace checks pass.
- Full application runtime/GPU and cross-architecture runtime testing were not performed.

Measurements used an Intel Xeon E5-2696 v4, Windows x86_64 MSVC, rustc 1.99.1 / LLVM 23.1.1, release opt-level 3, fat LTO, static CRT and generic CPU settings. Integrated Cargo test binaries use panic unwinding. The extracted prototype uses one codegen unit and panic abort. No own builds or correctness checks overlapped timing; no `rustc` processes were present immediately before or after the integrated run.

The committed ignored benchmark calls the production helper through a non-inlined wrapper. Vertices, transform, clipping rectangle and material settings are black-boxed, as is the complete returned object. Fixtures and reusable output vectors are allocated before timing. Each invocation clears and returns its emitted vector to the pool. The benchmark thread is pinned to logical CPU 2; `QueryThreadCycleTime` measures CPU cycles and `Instant` measures wall time. Each process measures ten rounds of `max(100000 / max(vertices, 1), 40)` iterations per case; round zero is discarded. Six fresh before/after process pairs alternate binary order and rotate cases by 0, 5, 10, 15, 20 and 25. Speedups are medians of six paired process-median ratios; absolute columns are independent medians of process medians.

The timing function, production wrapper and input fixtures are identical in both archived harnesses. The baseline's test-only portable wrapper calls the frozen original; the updated wrapper calls the production portable specialization. The completed frame reference also freezes the previously shared dispatch caller. Neither change alters the timing loop. SHA-256:

- Before executable: `a75e891677e383dc9945d358e410902d9277810325c39d2a502c87f2e5a0f893`.
- After executable: `fa96d54e555128aa412781671672196691ac301386f3da34a967c353ccbc732a`.
- Benchmark function: `9d20840dc2a7076119924d511c623b268569d5ac819669647c0e0bb65d8a40d2`.

Reproduction from the repository root:

```powershell
cargo test --release --locked -j 2 -p deadlib-present --lib --test masked_render -- --test-threads=1
$env:MESH_CLIP_BENCH_ROTATION = '0'
cargo test --release --locked -j 2 -p deadlib-present --lib compose::clip_performance::benchmark_mesh_clipping -- --exact --ignored --nocapture --test-threads=1
Remove-Item Env:MESH_CLIP_BENCH_ROTATION
cargo check --release --locked -j 2 -p deadlib-present
cargo clippy --release --locked -j 2 -p deadlib-present --all-targets --no-deps
```

For comparative measurements, archive the library test executable from `cargo test --release --locked -j 2 -p deadlib-present --lib --no-run --message-format=json` before and after the production change, retaining the same timing function and fixtures. Before integration, point the test-only portable wrapper at the frozen implementation. Run the exact ignored benchmark directly from the archived executables with all six rotations and alternating process order. Binaries, manifests, before/after source snapshots, logs, source verification, assembly and runners remain in `target/mesh-clip-fma-audit-2026-11-08/`, where the preceding investigation began.
