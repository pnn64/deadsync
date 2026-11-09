Vertex-colored software textured meshes now use a runtime-checked FMA entry point for eligible byte textures. The previous final-LTO prepared-mesh byte caller contains 283 static CRT `fmaf` call sites across its filtering and material branches. Coverage, UV interpolation, filtering, vertex color interpolation, depth interpolation, blending and packing are all candidates for native fused operations inside the pixel loop.

Stripe rejection happens before dispatch. The optimized and portable paths share the original numerical pixel loop from `b8f046c77`, including alpha rejection before depth testing, interpolation order and callback order. Projection, clipping, triangle setup, draw counts, byte/float16 writers and the depth-test/quantization body are unchanged. The ordinary textured-sprite path from the previous pass is retained as a performance control.

The new entry requires x86/x86-64 FMA support, byte texture storage, and vertex UV, depth and color magnitudes at most `f32::MAX / 4`. This bounds intermediates during covered-pixel interpolation and excludes nonfinite shading. Half-float textures and exceptional inputs retain the portable loop. Rust's CPU/OS feature detector guards the private unsafe call; other architectures and CPUs without FMA retain portable arithmetic. There is no global CPU feature, approximate arithmetic or new allocation.

Prepared Alpha draws at 32x32 and 128x128 measure **3.7-3.9x faster**, and complete projected linear-filtered Alpha mesh draws measure **3.95-4.15x faster**. Quantized-depth alpha-mask draws improve about **3.3-3.5x**, and Alpha draws rejected by depth testing about **3.5-3.7x** at sizes 8 through 128. Tiny 2x2 direct Alpha draws improve about 2.1x, with projection overhead included. Other blend-mode results and the broader process ranges are retained in the CSVs.

| Byte-attachment workload | Before cycles | After cycles | Median speedup | Six-pair range |
| --- | ---: | ---: | ---: | ---: |
| Prepared nearest, 2x2, Alpha | 948.64 | 332.26 | 2.829x | 2.664-4.317x |
| Prepared linear, 2x2, Alpha | 1,451.45 | 468.02 | 3.094x | 2.857-4.632x |
| Prepared nearest, 32x32, Alpha | 181,648.89 | 48,216.06 | 3.768x | 2.328-5.667x |
| Prepared linear, 32x32, Alpha | 280,035.78 | 74,244.90 | 3.772x | 2.400-5.505x |
| Prepared nearest, 128x128, Alpha | 2,931,179.66 | 788,011.06 | 3.718x | 3.544-3.771x |
| Prepared linear, 128x128, Alpha | 4,631,619.72 | 1,192,147.00 | 3.883x | 3.606-5.857x |
| Mask with UNORM16 depth, 128x128, Alpha | 3,717,996.81 | 1,111,562.12 | 3.342x | 2.006-5.092x |
| Rejected by depth, 128x128, Alpha | 3,353,606.22 | 924,220.88 | 3.600x | 2.253-3.709x |
| Direct projected linear, 32x32, Alpha | 300,031.56 | 75,458.49 | 3.954x | 2.332-4.019x |
| Direct projected linear, 128x128, Alpha | 4,984,547.44 | 1,202,362.78 | 4.146x | 2.736-4.241x |
| Sprite control, 32x32, Add | 64,436.19 | 44,184.31 | 1.459x | 1.001-1.657x |
| Empty prepared stripe | 52.18 | 43.40 | 1.033x | 0.891-1.636x |

The timing fixtures draw two textured triangles forming a colored rectangle using an 8x8 byte texture, repeat wrapping and mixed alpha including fully transparent texels. Prepared cases reuse projected vertices and triangle setup, matching retained rendering. Direct cases include projection, clipping, triangle setup and returned vertex counts. All timing includes buffer clearing, optional depth clearing, dispatch, rendering and output observation. Depth-reject fixtures clear depth to zero so ordinary depth testing rejects every shaded fragment. These are software drawing calls, not complete frames, window presentation, GPU rendering or application FPS.

The [raw CSV](2026-11-09-software-tmesh-fma-raw.csv) retains all **17,520 observations** across 73 workloads. The [summary CSV](2026-11-09-software-tmesh-fma-summary.csv) includes all tested sizes and blend modes, wall time and reference controls. Variant 0 is production; variant 1 uses six frozen original textured-mesh entry points and rasterization functions from `b8f046c77`, with unchanged production sampling, writers, projection helpers and depth tests. Both variants use the current ordinary textured-triangle path for the sprite control. The entire benchmark module is identical in the before/after source snapshots.

Unchanged-reference control medians range **0.923-1.462x**, and sprite-control medians range **0.996-1.459x**, indicating substantial process/phase variation. The 32x32 Add sprite control is a particularly visible outlier; its code path is unchanged. Within each updated process, frozen-reference/current ratios are **3.865-4.197x** for prepared nearest 32x32 Alpha draws, **3.777-3.981x** for prepared linear 32x32 Alpha draws, and **4.161-4.307x** for direct linear 128x128 Alpha draws. These corroborate the large target-path improvement despite the broad before/after ranges. Ranges here are observed samples, not confidence intervals; no universal speedup for every machine or workload is established.

The measured library-test executable's `.text` section grows from **2,498,336 to 2,656,688 bytes**, an increase of **158,352 bytes / 6.3%**. File size grows from 2,918,400 to 3,078,656 bytes. These binaries contain regression fixtures and several callback/reference variants; this is not an application binary-size measurement.

Measurements used Windows x86-64, an Intel Core i7-1250U, rustc 1.97.1 / LLVM 22.1.6, release opt-level 3, fat LTO, static CRT and generic CPU settings. Six fresh before/after process pairs alternate binary order. The benchmark pins its thread to logical CPU 2, rotates all cases by 0 through 5 and alternates production/reference order within each round. Ten rounds are recorded per case, excluding round zero from summaries while retaining it in raw data. Drawing observations perform `max(100000 / (size * size), 16)` calls; the empty-stripe control performs 100000 calls. Allocation and fixture preparation stay outside timing. No compilation or rendering-test execution from this change overlaps timing; compiler inventories at both measurement boundaries are empty.

Speedups are medians of six paired process-median cycle ratios. Absolute cycle/time columns are independent medians, so their quotient can differ. Archived binaries, identical benchmark fixtures, runner, manifest, assembly and validation logs remain under `target/assembly-pass6/`.

The expanded raw-bit regression exposed NaN-sign variation even between two portable baseline copies, before the production optimization. The first mismatch was a half lane containing `0xfe00` versus `0x7e00`; all other lanes of that pixel matched. The initial failure is retained in `target/assembly-pass6/before-tests.log`. Exceptional arithmetic therefore cannot be described as universally preserving every NaN encoding against a separately compiled frozen copy.

The final regression always requires exact byte pixels, every non-NaN half lane, every non-NaN callback field, callback indices/modes/order, returned counts and every depth bit. It additionally requires **all bits**, including half attachment NaN signs, in its strict comparison group: byte textures with bounded prepared shading attributes, plus the direct fixture draws. Outside that domain it counts only half NaN-sign differences and callback NaN-encoding differences, requiring both differing values to be NaNs. The archived before/after library-test executables each record **272 half-lane sign differences and 31 callback NaN-encoding differences** against the frozen copy. The triangle integration executable reports zero such differences with the same fixtures, further showing sensitivity to compilation context. Equal counts are not a guarantee of identical NaN encodings for every exceptional input.

Validation:

- Baseline and updated software library release suites each pass **35 tests**, with eight opt-in/exhaustive tests ignored.
- **8,192 draw/stripe configurations** cover prepared and direct meshes, all five blend modes, all six depth modes, both depth formats, no depth attachment, nearest/linear filters, clamp/repeat wrapping, opaque/masked paths, byte/half textures, reversed winding, clipping, singular transforms, partial/empty stripes and exceptional float inputs.
- The regression compares **487,326 exact byte pixels**, **974,652 half pixels**, **714,400 callback float fields** and **1,670,760 exact depth fields**. Of the half pixels, **384,930** are in the strict comparison group and require every stored bit to match. Exceptional NaN comparisons follow the rules and counts above.
- Updated triangle and batching integration suites pass **39** and **38 tests**, with nine and eight ignored respectively. Existing projection, clipping, coverage, alpha rejection and depth-mode regressions pass alongside the new comparison.
- The new regression also passes with the software package at opt-level 0, debug assertions and overflow checks enabled, using optimized dependencies. It applies the same exact finite/depth checks and separately counts exceptional NaN encodings.
- All software crate targets pass an ARM64 Linux release compile check. Clippy completes with no new warning messages. New Rust files, the changed raster block and whitespace checks pass formatting verification.

Final fat-LTO triangle integration-test assembly emits **seven production byte-kernel bodies with 20-35 native FMA instruction sites each and zero direct `fmaf` calls**. The seven float16-writer bodies and fifteen trace-callback bodies likewise contain native FMA and zero direct `fmaf` calls. Portable fallback arithmetic still contains scalar math calls. Full assembly, per-kernel excerpts, the earlier prepared-caller excerpt and an inventory are retained under `target/assembly-pass6/`.

Reproduce the focused checks and current production/reference benchmark:

```powershell
cargo test --release --locked -j 2 -p deadlib-render-backend-software --lib --test triangle_pipeline --test batching_pixels -- --test-threads=1
cargo test --release --locked -j 2 -p deadlib-render-backend-software --lib tmesh_raster_performance -- --nocapture --test-threads=1
$env:TMESH_RASTER_ROTATION = '0'
cargo test --release --locked -j 2 -p deadlib-render-backend-software --lib tmesh_raster_performance::benchmark_tmesh_raster -- --exact --ignored --nocapture --test-threads=1
Remove-Item Env:TMESH_RASTER_ROTATION
cargo check --release --locked -j 2 -p deadlib-render-backend-software --target aarch64-unknown-linux-gnu --all-targets
cargo clippy --release --locked -j 2 -p deadlib-render-backend-software --all-targets --no-deps
```

SHA-256:

- Before executable: `81235a03c8d5fc54971cc42881ef5514b2fe4bbe98551c22b07e169cd513fc4e`.
- After executable: `fe6fc7bf321bb381ea820047220e90ebc44d68b8123900db621e5ad679ab5a87`.
- Benchmark loop: `da7f52389a69aa42788480fba4b4c3819f9f44a4e7a8cd41d0645f1e6fb679ae`.

Window/GPU runtime, application FPS, physical non-FMA CPUs and ARM64 runtime were not tested. Half-texture fallback and float16 attachment throughput were not benchmarked; correctness coverage and NaN limits are described above.
