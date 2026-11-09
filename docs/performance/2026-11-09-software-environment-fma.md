The software environment mapper now dispatches eligible projected triangles to a runtime-checked FMA pixel loop. Previous final-LTO assembly retains scalar CRT `fmaf` calls for coverage, texture filtering, blending and byte packing. Keeping sampling inline in the specialized loop lets LLVM use native fused operations throughout that work.

UV generation, sphere mapping, projection, clipping, triangle setup and stripe rejection remain outside the new entry point. Both paths share the original pixel loop from `db59c1b42`: coverage, depth interpolation, perspective weighting, UV/color interpolation, reflection composition, alpha rejection, depth testing and callback order retain their original arithmetic. The FMA path packs the three perspective-weight divisions into one vector divide; the portable path keeps the original division expressions. Each path computes the original denominator and uses real division, preserving rounding without reciprocal approximation. Sampling, pixel-writer and depth-test function bodies are unchanged.

The FMA path requires CPU/OS support, byte primary/additive textures, positive normal inverse-W values and bounded projected UV, depth and color values. Inverse-W and shading magnitudes are capped at `f32::MAX / 4`; inverse-W must be at least `f32::MIN_POSITIVE`. These guards keep covered-pixel perspective interpolation finite. Half textures, negative/subnormal inverse-W and exceptional shading retain portable math. The private unsafe call follows the renderer's existing runtime-feature convention; no global CPU requirement or new allocation is introduced.

An exploratory packed-division-only candidate showed roughly 0-0.8% changes in representative Alpha draws, too small to support a useful standalone gain. The final change confines that packing to the specialized path and also removes the per-pixel scalar math calls. The prototype source, binary and measurements remain under `target/assembly-pass7/packed-only/`.

Complete Alpha environment draws at sizes 8 through 128 measure about **2.5-2.6x faster with nearest sampling**, **2.7-3.0x with sphere mapping or masks**, and **3.1-3.5x with reflection/perspective cases**. At 2x2, fixed material/projection work limits the Alpha gains to approximately 1.35-1.75x. Other blend modes and broad process ranges are retained in the CSVs.

| Byte-attachment workload | Before cycles | After cycles | Median speedup | Six-pair range |
| --- | ---: | ---: | ---: | ---: |
| Nearest, 2x2, Alpha | 1,568.30 | 940.63 | 1.350x | 1.197-2.298x |
| Reflection, 2x2, Alpha | 3,345.64 | 1,937.40 | 1.728x | 1.024-2.961x |
| Nearest, 32x32, Alpha | 193,790.71 | 68,721.13 | 2.531x | 1.737-4.232x |
| Sphere/linear, 32x32, Alpha | 310,600.80 | 94,586.02 | 2.906x | 2.018-4.887x |
| Reflection/linear, 32x32, Alpha | 530,919.99 | 139,232.09 | 3.388x | 3.319-5.038x |
| Mask/linear, 32x32, Alpha | 267,790.16 | 95,244.72 | 2.897x | 2.716-2.914x |
| Perspective/reflection, 32x32, Alpha | 460,590.41 | 128,257.32 | 3.390x | 2.878-5.788x |
| Nearest, 128x128, Alpha | 3,034,184.12 | 992,866.72 | 2.580x | 2.555-4.366x |
| Reflection/linear, 128x128, Alpha | 8,844,347.47 | 2,240,132.75 | 3.475x | 2.459-5.134x |
| Perspective/reflection, 128x128, Alpha | 7,543,696.94 | 2,122,490.22 | 3.464x | 2.426-4.894x |
| Texture control, 32x32, Alpha | 88,229.91 | 82,614.95 | 0.996x | 0.965-1.692x |
| Empty stripe | 709.33 | 524.01 | 1.132x | 0.938-1.619x |

Each fixture draws two textured triangles forming a colored rectangle with an 8x8 byte texture and mixed alpha, including transparent texels. Cases cover ordinary UVs, primary sphere mapping, a secondary reflection texture, masks and a perspective camera with varying inverse-W. Timing includes material UV generation, projection/clipping/setup, dispatch, buffer/depth clearing, sampling, blending, depth work and output observation. Allocation and fixture creation stay outside timing. These are complete environment drawing calls, not whole frames, window presentation, GPU rendering or application FPS.

The [raw CSV](2026-11-09-software-environment-fma-raw.csv) retains **17,520 observations** across 73 workloads. The [summary CSV](2026-11-09-software-environment-fma-summary.csv) includes all sizes, tested blend modes, wall time and reference controls. Variant 0 is production; variant 1 uses the frozen original environment rasterizer from `db59c1b42`, with unchanged production UV mapping, projection, sampling, writers and depth tests. Both variants use the current ordinary textured-mesh rasterizer for the control. The complete benchmark module is identical in the archived before/after builds.

Unchanged-reference control medians range **0.865-1.240x**, and unchanged texture-control medians range **0.989-1.074x**, exposing substantial process variation. Within the updated processes, frozen-reference/current ratios are **2.524-2.564x** for 32x32 nearest Alpha draws and **3.292-3.537x** for 32x32 reflection Alpha draws. Perspective/reflection Alpha at 128x128 spans **2.503-3.624x**. These corroborate the improvement without treating observed ranges as confidence intervals. The updated empty-stripe ratio against the same-process frozen reference is **0.932-0.956x**, so empty work is not a universal win despite bypassing feature detection.

The library-test executable's `.text` section grows from **2,750,560 to 2,818,880 bytes**: **68,320 bytes / 2.5%**. File size grows from 3,180,032 to 3,248,640 bytes. These binaries include several callback variants and frozen reference implementations; this is not an application binary-size measurement.

Measurements used Windows x86-64, an Intel Core i7-1250U, rustc 1.97.1 / LLVM 22.1.6, release opt-level 3, fat LTO, static CRT and generic CPU settings. Six fresh before/after process pairs alternate binary order. The benchmark pins its thread to logical CPU 2, rotates all cases by 0 through 5 and alternates production/reference order within each round. Ten rounds are recorded per case, excluding round zero from summaries while retaining it in raw data. Drawing observations perform `max(100000 / (size * size), 16)` calls; the empty-stripe control performs 100000 calls. No compilation or rendering-test execution from this change overlaps timing; compiler inventories at both boundaries are empty.

A final lint cleanup removes two redundant reference coercions. The rebuilt executable matches the measured after executable byte-for-byte after masking only PE/debug timestamps and PDB age; all runtime code and data are identical. The exact comparison and both binary hashes are archived in `target/assembly-pass7/borrow-cleanup-comparison.json`.

Speedups are medians of six paired process-median cycle ratios. Absolute cycle/time columns are independent medians, so their quotient can differ. Source snapshots, archived binaries, identical fixtures, runner, manifest, prototype experiment, assembly and validation logs remain under `target/assembly-pass7/`.

Validation:

- Baseline and updated software library release suites each pass **36 tests**, with nine opt-in/exhaustive tests ignored. Existing environment regressions also compare earlier sphere-mapping/projection implementations across camera, sampler, culling and material combinations.
- **8,192 new draw/stripe cases** cover all eight texture-generation flags, masks, secondary textures and primary fallback, nearest/linear filters, clamp/repeat wrapping, all six depth modes, both depth formats, missing depth attachments, perspective cameras, reversed winding, culling, singular transforms, partial/empty stripes and exceptional float inputs.
- **487,326 byte pixels** and **1,670,760 depth fields** match exactly. The comparison also checks **974,652 half pixels** and **672,860 callback float fields**, requiring exact non-NaN bits and exact callback indices/modes/order. Regular byte-texture fixtures require every half/callback bit, including half attachment NaN signs.
- For exceptional inputs, the comparison separately counts half NaN-sign differences and callback NaN-encoding differences against the frozen copy, requiring both differing values to be NaNs. The archived before/after library-test binaries each report **18 differing half lanes and zero differing callback fields**. The baseline already exhibits these differences. The triangle-test executable reports 18 half lanes and 160 callback fields, illustrating compilation-context sensitivity; equal library-test counts are not a universal guarantee of stable NaN encodings or locations.
- Updated triangle and batching integration suites pass **40** and **39 tests**, with ten and nine ignored respectively. New frame comparisons use Alpha blending; existing pixel-writer regressions cover all five blend modes, and timing covers Alpha/Add/Multiply.
- The new regression also passes with the software package at opt-level 0, debug assertions and overflow checks enabled, using optimized dependencies. It applies the same exact finite/depth checks and separately counts exceptional NaN encodings.
- ARM64 Linux release checks pass for all software crate targets. Clippy completes with no new warning messages after removing redundant reference coercions. New Rust files, the changed raster block and whitespace checks pass formatting verification.

Final fat-LTO triangle-test assembly contains **33 native FMA sites in the byte kernel**, **25 in the float16-writer kernel** and **16 in the callback-trace kernel**, with zero direct `fmaf` calls in each. All three use one packed perspective-weight divide. Their remaining scalar divide is depth quantization, not another perspective weight. Projection/UV preparation and portable fallback code still contain scalar math. Full assembly, kernel excerpts and an inventory are archived under `target/assembly-pass7/`.

Reproduce the focused checks and current production/reference benchmark:

```powershell
cargo test --release --locked -j 2 -p deadlib-render-backend-software --lib --test triangle_pipeline --test batching_pixels -- --test-threads=1
cargo test --release --locked -j 2 -p deadlib-render-backend-software --lib environment_raster_performance -- --nocapture --test-threads=1
$env:ENVIRONMENT_RASTER_ROTATION = '0'
cargo test --release --locked -j 2 -p deadlib-render-backend-software --lib environment_raster_performance::benchmark_environment_raster -- --exact --ignored --nocapture --test-threads=1
Remove-Item Env:ENVIRONMENT_RASTER_ROTATION
cargo check --release --locked -j 2 -p deadlib-render-backend-software --target aarch64-unknown-linux-gnu --all-targets
cargo clippy --release --locked -j 2 -p deadlib-render-backend-software --all-targets --no-deps
```

SHA-256:

- Before executable: `703061a8756e529734079a9a1e64d81a9b7bcf306f173ba813627e0eb40ea98b`.
- After executable: `818f3d031d140842f6f90f8f4556990d14cb3b6a25fbdcb2c22e096770b846b3`.
- Benchmark loop: `492b6067e88d8ff44251f997c5abb72de8a75776c3384ae50d9ba6b7a414326f`.

The final lint-cleaned executable SHA-256 is `30aad38806ac2eb77b7b1b12bd835e892a0702a2356208bbf3b00b20d6a82729`.

Window/GPU runtime, application FPS, physical non-FMA CPUs and ARM64 runtime were not tested. Half-texture fallback and float16 attachment throughput were not benchmarked; correctness coverage and NaN limits are described above.
