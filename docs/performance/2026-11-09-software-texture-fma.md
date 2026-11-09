Ordinary software textured triangles now select a runtime-checked FMA entry point for eligible byte textures. Optimized assembly showed scalar CRT `fmaf` calls inside coverage, UV interpolation, bilinear filtering, blending and byte packing. The new entry point lets LLVM use native fused operations throughout that work.

Bounds and stripe rejection happen before dispatch. Both paths share the original pixel loop, unchanged from `4c803ac4f`; sampling, wrapping, coverage, alpha rejection, blend expressions, packing and callback order are unchanged. The byte and float16 pixel-writer bodies are also unchanged. This does not change the separate vertex-colored textured-mesh rasterizer.

Eligibility requires x86/x86-64 FMA support, byte texture storage, finite tint values and UV magnitudes at most `f32::MAX / 4`. The UV bound prevents intermediate interpolation overflow. Half-float textures and potentially nonfinite filtering retain portable arithmetic to preserve float16 NaN encodings. CPUs without FMA and other architectures retain the shared portable loop. The private unsafe entry is guarded by Rust's CPU/OS feature detector; no global CPU feature or approximate arithmetic is enabled.

Final fat-LTO triangle integration-test assembly contains seven emitted production byte-kernel bodies, each with **12-27 native FMA instruction sites and zero direct `fmaf` calls**. The emitted float16-writer and callback-trace variants also have native FMA and zero direct `fmaf` calls. Portable fallback code still contains scalar math calls. Full executable assembly, per-kernel excerpts and an assembly inventory remain under `target/assembly-pass5/`.

Archived release binaries measure approximately **2.8-3.0x faster nearest-filtered draws** and **3.3-3.5x faster ordinary linear-filtered draws** at sizes 8 through 128. Linear alpha masks improve approximately **2.6-2.9x**; opaque nearest draws improve approximately **3.1-3.2x**. Opaque masks do less sampling work to begin with and improve approximately **1.3-1.8x**, depending on size and blend mode. Even 2x2 textured draws improve approximately 1.6-2.7x in these fixtures.

These measurements include triangle/inverse-denominator setup, bounds, dispatch, coverage, interpolation, texture sampling, blending, packing, buffer clearing and output observation. The texture is an 8x8 byte RGBA fixture, with repeat wrapping and the caller's opaque/mask flags varied. They measure drawing calls into byte attachments, not complete sprites through object preparation, window presentation, complete frames, GPU rendering or application FPS.

| Byte-attachment workload | Before cycles | After cycles | Median speedup | Six-pair range |
| --- | ---: | ---: | ---: | ---: |
| Nearest, 2x2, Alpha | 411.88 | 205.17 | 2.007x | 1.999-2.073x |
| Nearest, 8x8, Alpha | 5,557.44 | 1,927.24 | 2.886x | 2.847-2.961x |
| Nearest, 32x32, Alpha | 91,059.70 | 32,669.20 | 2.787x | 2.754-4.063x |
| Nearest, 128x128, Alpha | 1,461,255.44 | 508,269.44 | 2.875x | 2.832-2.977x |
| Linear, 2x2, Alpha | 680.32 | 251.87 | 2.702x | 2.606-3.940x |
| Linear, 32x32, Alpha | 152,968.76 | 44,527.73 | 3.435x | 3.354-5.263x |
| Linear, 128x128, Alpha | 2,470,995.31 | 718,012.50 | 3.442x | 3.358-3.457x |
| Linear, 128x128, Multiply | 2,275,590.72 | 694,899.12 | 3.274x | 3.185-4.927x |
| Linear mask, 32x32, Alpha | 105,956.61 | 37,410.09 | 2.833x | 2.784-4.340x |
| Opaque nearest, 32x32, Alpha | 91,002.46 | 28,832.44 | 3.159x | 3.092-3.237x |
| Opaque mask, 32x32, Alpha | 44,072.75 | 25,841.60 | 1.706x | 1.702-1.708x |
| Opaque mask, 128x128, Multiply | 541,727.84 | 415,586.78 | 1.304x | 1.300-1.309x |
| Untextured mesh control, 32x32, Alpha | 32,986.41 | 33,001.93 | 0.999x | 0.999-1.454x |
| Empty textured stripe | 38.20 | 38.01 | 1.004x | 0.976-1.527x |

The [raw CSV](2026-11-09-software-texture-fma-raw.csv) retains all **17,760 observations** across 74 workloads. The [summary CSV](2026-11-09-software-texture-fma-summary.csv) includes all sizes, tested blend modes, wall time and reference controls. Variant 0 is production; variant 1 uses frozen original texture entry points and rasterization from `4c803ac4f`, with unchanged production sampling and pixel writers. Both variants use current untextured mesh rasterization for the controls. Complete benchmark fixtures and loops are identical in the before/after source snapshots.

Unchanged-reference control medians span **0.983-1.019x**, and untextured-mesh medians span **0.997-1.000x**. Individual process-pair ranges are wider, exposing frequency and scheduling outliers. Within the updated processes, frozen-reference/current cycle ratios are **2.746-2.879x** for 32x32 nearest Alpha draws and **3.353-3.454x** for 32x32 linear Alpha draws. These corroborate the large gain without interpreting observed ranges as confidence intervals. Empty textured stripes remain approximately 38 CPU cycles at the median.

The measured library-test executable's `.text` section grows from **1,837,296 to 2,007,408 bytes**, an increase of **170,112 bytes / 9.3%**. Its file size grows from 2,247,168 to 2,418,688 bytes. These binaries include several callback variants, regression fixtures and frozen reference implementations; this is not an application binary-size measurement.

Measurements used Windows x86-64, an Intel Core i7-1250U, rustc 1.97.1 / LLVM 22.1.6, release opt-level 3, fat LTO, static CRT and generic CPU settings. Six fresh before/after process pairs alternate binary order. The benchmark pins its thread to logical CPU 2, rotates all cases by 0 through 5 and alternates production/reference order within each round. Each case records ten rounds, excluding round zero from summaries while retaining it in raw data. Drawing observations perform `max(100000 / (size * size), 16)` calls; empty-stripe controls perform 100000 calls. Allocation and fixture creation stay outside timing. No build or correctness check from this change overlaps timing; compiler inventories at both measurement boundaries are empty.

Speedups are medians of six paired process-median cycle ratios. Absolute cycle/time columns are independent medians, so their quotient can differ. Archived binaries, fixtures, runner, manifest, assembly and validation logs remain under `target/assembly-pass5/`.

Validation:

- Baseline and updated software library release suites each pass **34 tests**, with seven opt-in/exhaustive tests ignored. Updated triangle and batching integration suites pass **38** and **37 tests**, with eight and seven ignored respectively.
- **6,480 draw/stripe cases** preserve every byte pixel and returned draw count, covering all five blend modes, nearest/linear filters, clamp/repeat wrapping, opaque/masked paths, byte/float16 textures, exceptional tint values, partial rows and empty stripes.
- **974,652 exact float16 pixel comparisons** preserve all stored bits with alpha preserved or forced, including arbitrary half attachment/texture patterns, signed zero, subnormals, finite extremes, infinities and NaNs. **954,648 callback float fields** preserve every bit, including NaN payloads, alongside callback index, mode and order.
- Both new regressions also pass with the software package at opt-level 0, debug assertions and overflow checks enabled, using optimized dependencies. Existing color, byte-writer, projection, clipping, coverage, depth and mixed-rendering regressions pass in the release suites.
- ARM64 Linux release checks pass for all software crate targets. Clippy completes with no new warning messages. New Rust files, the changed raster block and whitespace checks pass formatting verification.

Reproduce the focused checks and current production/reference benchmark:

```powershell
cargo test --release --locked -j 2 -p deadlib-render-backend-software --lib --test triangle_pipeline --test batching_pixels -- --test-threads=1
cargo test --release --locked -j 2 -p deadlib-render-backend-software --lib texture_raster_performance -- --nocapture --test-threads=1
$env:TEXTURE_RASTER_ROTATION = '0'
cargo test --release --locked -j 2 -p deadlib-render-backend-software --lib texture_raster_performance::benchmark_texture_raster -- --exact --ignored --nocapture --test-threads=1
Remove-Item Env:TEXTURE_RASTER_ROTATION
cargo check --release --locked -j 2 -p deadlib-render-backend-software --target aarch64-unknown-linux-gnu --all-targets
cargo clippy --release --locked -j 2 -p deadlib-render-backend-software --all-targets --no-deps
```

SHA-256:

- Before executable: `190f59bad12a1bbb98c1b0d6a7d25ac748926da1e96f1ae5651e0be499f14b28`.
- After executable: `ad30fbf417379acb5efb80cbe40043c9ebf86dde052c08cc5aede7034b727d7c`.
- Benchmark loop: `8c3ed4b64c892a1cda0ccf2ee1bcd27908fd7589c60cf9ca42515e0b697b1e3b`.

Window/GPU runtime, application FPS, physical non-FMA CPUs and ARM64 runtime were not tested. Half-float texture fallback and float16 attachment throughput were not benchmarked; their exact output is covered by the regressions.
