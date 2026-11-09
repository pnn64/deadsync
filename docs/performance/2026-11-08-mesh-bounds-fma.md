Affine textured-mesh bounds scans now dispatch to an FMA-enabled helper on supported x86 CPUs when the mesh has at least four vertices. The helper invokes the existing transform expressions and bounds reduction, allowing LLVM to replace per-vertex math-library calls with packed hardware FMA. Perspective transforms, smaller meshes and unsupported CPUs retain the portable path. There is no inline assembly, explicit intrinsic, approximation, new allocation, vertex-format change or global CPU-feature requirement.

This path belongs to CPU presentation composition and is used for masked textured meshes, including clipped text meshes. It is outside the software renderer and Lua. The three bounds callers perform culling and containment checks before geometry clipping; unmasked drawing does not automatically benefit.

The presentation crate changes from `forbid(unsafe_code)` to `deny(unsafe_code)`, retaining a default compile error for unsafe code while permitting local exceptions. Only the bounds dispatcher and its private FMA helper opt in in production. The unsafe call follows `is_x86_feature_detected!("fma")`, which checks CPU/OS support. A separate test-only allowance covers the existing Windows affinity/cycle-count benchmark helper. No raw-pointer or unchecked-memory operation is added to production.

The integrated Cargo release binaries measure **3.175x** faster affine bounds scanning at 96 vertices and **4.361x** at 1536 vertices, reducing CPU cost by about **68.5%** and **77.1%**. Independent median wall times fall from **0.739 to 0.230 microseconds** and **11.318 to 2.602 microseconds**, respectively. These measure the complete bounds helper, not full clipping, total composition, GPU rendering or application frame time. The [integrated raw data](2026-11-08-mesh-bounds-fma-raw.csv) retains all 1680 observations; the [summary](2026-11-08-mesh-bounds-fma-summary.csv) retains every case and range.

| Workload | Before cycles/scan | After cycles/scan | Median speedup | Six-pair range |
| --- | ---: | ---: | ---: | ---: |
| affine-0 | 34.8 | 34.8 | 0.999x | 0.954-1.064x |
| affine-1 | 45.2 | 45.5 | 0.996x | 0.955-1.040x |
| affine-3 | 81.3 | 80.0 | 1.020x | 0.945-1.054x |
| affine-4 | 96.5 | 67.4 | 1.422x | 1.285-1.527x |
| affine-6 | 175.0 | 88.5 | 1.968x | 1.877-2.100x |
| affine-96 | 1,618.7 | 503.3 | 3.175x | 2.967-3.894x |
| affine-1536 | 24,807.2 | 5,691.1 | 4.361x | 3.691-4.799x |
| affine-8192 | 128,417.4 | 39,328.4 | 3.250x | 3.137-3.536x |
| identity-6 | 179.4 | 86.1 | 2.043x | 1.999-2.172x |
| perspective-6 | 116.9 | 114.0 | 1.032x | 0.981-1.083x |
| identity-96 | 1,610.5 | 516.8 | 3.123x | 2.923-3.256x |
| perspective-96 | 1,705.9 | 1,702.8 | 1.002x | 0.946-1.044x |
| identity-1536 | 24,492.1 | 5,635.4 | 4.262x | 3.864-4.494x |
| perspective-1536 | 27,548.3 | 26,853.3 | 1.012x | 0.959-1.098x |

Tiny affine and perspective controls remain near parity. Affine-one measures 0.996x, and all control ranges cross parity. Reported ranges are observed process extrema, not confidence intervals or guarantees on other machines.

The standalone prototype measured 3.148x at 96 affine vertices and 4.561x at 1536, with corresponding ranges of 3.127-3.208x and 4.299-4.832x. Its [raw data](2026-11-08-mesh-bounds-fma-prototype-raw.csv) retains 2520 observations across original, runtime-selected and forced-portable variants. The [runtime summary](2026-11-08-mesh-bounds-fma-prototype-summary.csv) and [portable summary](2026-11-08-mesh-bounds-fma-prototype-portable-summary.csv) remain separate from the integrated results. Forced-portable medians range from 0.975x to 1.034x across prototype cases. This verifies fallback behavior on the FMA-capable test machine; it does not measure an older processor and compiling detection to false omits its runtime cost.

The audited assembly has four `fmaf` calls per affine vertex. The specialized helper uses packed `vfmadd231ps` across eight vertices, plus vector min/max and NaN-aware comparisons/blends; short inputs and tails use scalar hardware FMA. It operates on the existing 60-byte `TexturedMeshVertex` layout. All six production kernel bodies match the measured prototype after removing whitespace and comments; added lint attributes do not change those bodies. Full assembly and source manifests remain in the local audit directory.

Private bounds are **not bit-identical for every IEEE input**. Vectorized min/max reductions can select a different signed zero or NaN payload. Finite nonzero values match bit-for-bit; zero values compare equally and NaNs preserve their classification. The bounds are used only for clipping comparisons and caching the same comparisons' inputs, and are not copied into output vertices. The implementation documents this qualification next to the specialized helper. Actual geometry transforms and clipping arithmetic remain unchanged.

The prototype's stronger edge suite checked 207680 bounds cases and 2907520 clipping decisions on both paths. It observed 2067 zero-sign and 20204 NaN-bit differences across runtime/portable comparisons, with no numeric or clipping-decision mismatch. A separate 55496-case prototype suite matched all bounds bits. These prototype results are supplementary to the committed regressions and frame checks below.

Validation:

- **193 baseline and 193 updated library release tests pass**, with eight opt-in benchmarks ignored in ordinary runs. All twelve archived-binary benchmark invocations pass separately.
- **171 masked-render integration tests pass**, including shared library-module tests, allocation/churn checks, recycling, containment, rejection, mask ties and clipped text behavior. Counts include shared tests and are not counts of distinct independent cases.
- The new bounds regression checks **209728 cases and 2929024 clipping decisions**, comparing runtime-selected and explicitly portable production paths against the frozen scalar implementation. It covers empty/tiny/vector-tail lengths, mixed signed zeros and NaN payloads, subnormals, infinities, extreme/random float patterns, affine/perspective and singular transforms. It compares finite nonzero bounds by bits and exercises exact-boundary, invalid and nonfinite masks.
- The new frame differential verifies **3360 completed render frames bit-for-bit**, including retained storage identity, using the actual production mask clipping and frame finalization. It covers contained, partial, rejected and degenerate finite meshes; signed-zero inputs; empty and incomplete triangles; identity, affine, perspective, reflection, singular and offscreen transforms; shared/reusable/transient storage; empty, invalid, edge, duplicate/tied and overlapping masks; preserved prefixes and nested clipping. It checks the whole backend-visible frame, not only bounds or an image approximation.
- Nine reference function bodies are frozen from baseline `54f8191ca`: the old bounds implementation and the clipping control flow that consumes it. Geometry generation and frame finishing are shared unchanged production routines. A source comparison confirms the frozen bodies match baseline. The reference module's Clippy allowance retains the original vector-argument signatures.
- Both new regressions also pass at **opt-level 0 with debug assertions and overflow checks**. This direct rustc build compiles the production presentation crate and committed tests, links cached release dependencies with LTO, and runs the two focused regressions. It is not a full Cargo debug build. A first attempt without LTO could not link the cached bitcode libraries; the LTO-enabled build completes successfully.
- `cargo check --release` and Clippy for all crate targets complete. Clippy reports warnings in existing code; the new modules and production change have no diagnostics. New test/reference files pass formatting, and changed production hunks are formatted. Crate-wide `cargo fmt --check` still reports pre-existing differences in `actors.rs`, other portions of `compose.rs`, and `font_cache_original.rs`; those portions are unchanged. Staged whitespace checks pass.
- Full application runtime/GPU testing and cross-architecture runtime testing were not performed.

Measurements used an Intel Xeon E5-2696 v4, Windows x86_64 MSVC, rustc 1.99.1 / LLVM 23.1.1, release opt-level 3, fat LTO, static CRT and generic CPU settings. Integrated measurements use Cargo release test executables with panic unwinding. The prototype used one codegen unit and panic abort. No builds or correctness checks from this change overlapped the integrated timed runs.

The committed `benchmark_mesh_bounds` calls the actual production helper through a non-inlined wrapper, with black-boxed inputs and outputs. Fixtures are allocated outside timing and checked against the reference before timing. Each process measures ten rounds per case using `max(500_000 / max(vertices, 1), 200)` calls per round; only round zero is excluded. Six fresh process pairs alternate before/after binary order and rotate cases by 0, 5, 10, 15, 20 and 25. The benchmark thread is pinned to logical CPU 2. `QueryThreadCycleTime` measures CPU cycles and `Instant` measures elapsed time. Speedups are medians of six paired process-median ratios; absolute columns are independent medians of process medians.

The benchmark/regression module is identical between archived binaries. The baseline has the original production calculation plus the new test-only module and lint policy needed by its timing helper. SHA-256:

- Before executable: `70d42def9237feabed96dfab935a871738c839c705a4d2f417b61ccde2812445`.
- After executable: `349c5a43d707067a561e18eb710b4460f9c3282794fad79da096f047107f8957`.
- Benchmark/regression module: `8fed5db95e861c7717f3269d69867ede7f1955bbca30b231ceacc7ac9313d240`.

Reproduction from the repository root:

```powershell
cargo test --release --locked -j 2 -p deadlib-present --lib --test masked_render -- --test-threads=1
$env:MESH_BOUNDS_BENCH_ROTATION = '0'
cargo test --release --locked -j 2 -p deadlib-present --lib compose::bounds_performance::benchmark_mesh_bounds -- --exact --ignored --nocapture --test-threads=1
Remove-Item Env:MESH_BOUNDS_BENCH_ROTATION
cargo check --release --locked -j 2 -p deadlib-present
cargo clippy --release --locked -j 2 -p deadlib-present --all-targets --no-deps
```

For comparative timing, archive the library test executable from `cargo test --release --locked -j 2 -p deadlib-present --lib --no-run --message-format=json` before and after the production change, keeping the same benchmark module. Run the exact ignored benchmark directly from each executable with all six rotations, alternating process order. Keep builds and correctness checks outside timing. Local binaries, manifests, source snapshots, assembly, logs, source verification and runners remain in `target/mesh-bounds-fma-audit-2026-11-08/`.
