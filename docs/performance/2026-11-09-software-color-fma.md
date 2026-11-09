The software renderer now selects a runtime-checked FMA entry point for colored, untextured triangles with finite vertex colors. Stripe rejection happens first, so triangles outside a worker's rows return before feature detection. Both paths share the original coverage, barycentric interpolation, alpha rejection and callback loop; only the bounds preparation and function boundary change. Projection, draw counts and byte/float16 pixel-writer bodies remain unchanged.

The previous generic loop lowers its two coverage FMAs and eight color-interpolation FMAs to scalar CRT calls. The new entry point permits native scalar and packed FMA. There is no approximation, changed interpolation order, manual SIMD intrinsic, inline assembly, new allocation or global CPU-feature requirement. The private unsafe call is guarded by Rust's CPU/OS feature detector, following the existing software YUV uploader's convention. Other architectures and CPUs without FMA retain the portable loop.

The finite-color guard is required for exact float16 attachment bytes. An unrestricted candidate changed a stored NaN sign in the integrated regression (`Multiply`, alpha preservation disabled), despite passing the smaller standalone prototype. Nonfinite vertex colors therefore retain portable arithmetic. The final regressions preserve every byte and float16 bit, including those NaN encodings; the rejected candidate's failure log remains in `target/assembly-pass3/unrestricted-NaN-sign-tests.log`.

Archived Cargo release library-test binaries measure roughly **1.8-2.2x** faster typical colored-triangle draws and **1.88-2.08x** faster complete mesh projection/rasterization/blending into byte attachments. The complete-mesh fixtures contain two triangles forming a colored rectangle. Buffer clearing, output observation and runtime dispatch are included. These are software drawing calls, not window presentation, complete frames, GPU rendering or application FPS.

| Byte-attachment workload | Before cycles | After cycles | Median speedup | Six-pair range |
| --- | ---: | ---: | ---: | ---: |
| Triangle, 2x2, Alpha | 339.17 | 204.29 | 1.643x | 1.105-2.436x |
| Triangle, 8x8, Alpha | 4,766.87 | 2,561.78 | 1.860x | 1.221-2.906x |
| Triangle, 32x32, Alpha | 77,373.48 | 41,860.92 | 1.852x | 1.263-2.694x |
| Triangle, 128x128, Alpha | 1,241,367.16 | 674,273.50 | 1.847x | 1.223-2.702x |
| Triangle, 32x32, Multiply | 65,383.02 | 33,141.64 | 1.937x | 1.315-2.959x |
| Triangle, 128x128, Add | 1,237,526.59 | 644,532.09 | 1.928x | 1.301-2.861x |
| Mesh, 8x8, Alpha | 10,187.02 | 5,413.70 | 1.881x | 1.269-2.848x |
| Mesh, 32x32, Alpha | 133,321.26 | 69,764.97 | 1.901x | 1.283-2.813x |
| Mesh, 128x128, Alpha | 2,155,225.22 | 1,114,920.28 | 1.928x | 1.164-2.869x |
| Mesh, 128x128, Multiply | 1,755,723.81 | 876,406.25 | 1.966x | 1.395-3.144x |
| Empty stripe | 19.48 | 20.07 | 0.971x | 0.589-1.414x |

The [raw CSV](2026-11-09-software-color-fma-raw.csv) retains all **7,200 observations** across 30 workloads. The [summary](2026-11-09-software-color-fma-summary.csv) includes all modes, wall time and unchanged-reference controls. Variant 0 is production; variant 1 uses frozen original projection/rasterization control flow with the unchanged production pixel writers. The NoEffect triangle cases improve 3.83-4.31x, but preserve the existing behavior of leaving pixel values unchanged; they are not ordinary visible drawing throughput.

Process-to-process ranges are wide. Frozen-reference control medians span **0.949-1.299x**, exposing layout, frequency and scheduling variation. For a second comparison within each updated process, 32x32 Alpha triangles measure **1.826-1.868x** faster than the frozen reference across the six processes; complete 128x128 Alpha meshes measure **1.754-2.024x**. These support the optimization without making the broad before/after ranges into confidence intervals. Empty stripes show about **0.6 additional CPU cycles per call** at the median despite bypassing feature detection. No universal improvement for every workload or machine is established.

Measurements used Windows x86-64, an Intel Core i7-1250U, rustc 1.97.1 / LLVM 22.1.6, opt-level 3, fat LTO, static CRT and generic CPU settings. Six fresh before/after process pairs alternate binary order. The benchmark pins its thread to logical CPU 2, rotates triangle cases by 0 through 5, and alternates production/reference order within each round. Mesh cases retain their fixed order. Each case records ten rounds, excluding round zero from summaries while retaining it in the raw data. Each drawing observation performs `max(100000 / (size * size), 16)` calls; the empty-stripe control performs 100000 calls. Setup/allocation is outside timing; buffer clearing, the complete draw call and output observation are inside. Both binaries use identical benchmark loops and fixtures. No build or correctness check from this change overlaps timing; compiler inventories at both boundaries are empty.

Speedups are medians of six paired process-median cycle ratios. Absolute cycle/time columns are independent medians, so their quotient can differ. Complete before/after binaries, prototype source/data, compiler inventories, build/test logs and the comparative runner remain under `target/assembly-pass3/`.

Validation:

- Baseline and updated software library release suites each pass **30 tests**, with five opt-in/exhaustive tests ignored. The headless batching/rendering integration suite passes **33 tests**, with five ignored.
- **2,435,420 exact attachment-pixel comparisons** cover byte and float16 outputs, all five blend modes, preserved/forced alpha, reversed winding, clipping, empty and split stripes, signed zero, subnormals, finite extremes, infinities, signaling/quiet NaNs and deterministic arbitrary float/half patterns. Runtime and explicitly portable byte paths are checked against frozen original rasterization.
- **295,120 callback float fields** preserve exact non-NaN bits, with zero observed NaN-payload differences. Callback count, index, blend mode and order also match.
- **2,560 complete mesh-draw/stripe cases** preserve returned vertex counts and every output pixel, including singular/nonfinite projection inputs and partial worker rows.
- All three new regressions also pass with the software package at **opt-level 0**, debug assertions and overflow checks enabled, using cached optimized dependencies. Attachment bits and callback fields again match exactly.
- The full software crate passes an **ARM64 Linux release compile check**. Clippy for all crate targets completes with existing diagnostics; its attribute-spacing warning in the Windows `assert_depth_captures` test also exists in the baseline. The changed raster blocks and new Rust files pass rustfmt and whitespace checks.
- Final fat-LTO batching-test assembly contains native FMA in all three specialized callback variants: eight instruction sites for the byte callback, fourteen for float16, and four for the callback trace. None has a direct `fmaf` call. Some byte-blending work still calls the shared generic byte-writer helper, which retains scalar math calls; this change does not eliminate every scalar math call in the renderer. Original/prototype and optimized helper assembly is retained locally.

Reproduce the focused checks and current production/reference benchmark:

```powershell
cargo test --release --locked -j 2 -p deadlib-render-backend-software --lib --test batching_pixels -- --test-threads=1
cargo test --release --locked -j 2 -p deadlib-render-backend-software --lib color_raster_performance -- --nocapture --test-threads=1
$env:COLOR_RASTER_ROTATION = '0'
cargo test --release --locked -j 2 -p deadlib-render-backend-software --lib color_raster_performance::benchmark_colored_triangles -- --exact --ignored --nocapture --test-threads=1
Remove-Item Env:COLOR_RASTER_ROTATION
cargo check --release --locked -j 2 -p deadlib-render-backend-software --target aarch64-unknown-linux-gnu
cargo clippy --release --locked -j 2 -p deadlib-render-backend-software --all-targets --no-deps
```

Frozen projection/rasterization functions are from `ca3942150`. Byte/half writer and blending function bodies are byte-identical to that baseline. SHA-256:

- Before executable: `756d5d5db0bf675b105fcad26fd5149438e83c3ec2dbd3e8042f0932a4eaa602`.
- After executable: `06f102f7e95b5db72446a88c978529ea8d3110b3e9df4a10ac7e0b2fc6f2f6bf`.
- Benchmark loop and fixtures: `6e7569b6797f6552feb657273eb24972da0d31d85bd02b6023bb373b57cac8ba`.

Window/GPU runtime, application FPS, physical non-FMA CPUs and ARM64 runtime were not tested. Float16 performance was not benchmarked; its exact stored output is covered by the regressions.
