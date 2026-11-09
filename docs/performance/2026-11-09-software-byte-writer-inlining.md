The shared software byte-pixel writer now marks its returned closure `#[inline(always)]`. This lets the existing runtime-checked FMA colored-triangle entry point absorb byte blending and quantization. The previous optimized assembly still called an outlined writer containing scalar CRT `fmaf` calls, even after coverage and color interpolation had moved to hardware FMA.

The production change is one closure attribute and its explanatory comment. Arithmetic expressions, coverage, blending modes, clamping, byte packing, bounds checks and the float16 writer are unchanged. The existing CPU/OS feature check and finite-color guard still control entry to the FMA rasterizer. Generic callers continue using portable arithmetic. There is no new CPU requirement or unsafe production code.

Final fat-LTO headless integration-test assembly confirms the mechanism: the byte rasterizer goes from **8 to 18 native FMA instruction sites** and loses its call to the shared byte writer. Its remaining calls are bounds-check panic paths. The old outlined writer has **16 static `fmaf` call sites** across its branches. This does not remove scalar math calls from generic textured rasterizers or the whole renderer. Assembly excerpts and full executable assembly are retained under `target/assembly-pass4/`.

Complete colored-mesh draws into byte attachments measure about **2.1x faster for Alpha/Add** at 32x32 and 128x128, and **1.7x for Multiply**. Each mesh has two triangles forming a colored rectangle; measurements include projection, rasterization, blending, runtime dispatch, buffer clearing and output observation. These are drawing-call measurements, not application FPS or complete frame/presentation measurements.

There is a tradeoff in shared callers. Nearest-filtered Alpha/Add texture triangles improve approximately **5-9%**, and linear-filtered alpha-mask triangles improve approximately **3-6%** at sizes 8 through 128. Ordinary linear-filtered texture draws regress approximately **3-4%**. A weaker `#[inline]` candidate leaves the writer outlined and loses the colored-mesh gain, so the explicit hint is retained. Its prototype binary and log are archived locally. This change is useful for colored meshes; it is not a universal improvement for every mix of draws.

| Byte-attachment workload | Before cycles | After cycles | Median speedup | Six-pair range |
| --- | ---: | ---: | ---: | ---: |
| Mesh, 2x2, Alpha | 647.43 | 333.33 | 1.601x | 1.580-2.307x |
| Mesh, 8x8, Alpha | 5,419.55 | 2,460.70 | 2.201x | 2.136-3.708x |
| Mesh, 32x32, Alpha | 69,770.78 | 33,087.31 | 2.112x | 2.015-3.537x |
| Mesh, 128x128, Alpha | 1,115,473.19 | 527,783.38 | 2.114x | 1.381-2.123x |
| Mesh, 32x32, Add | 67,265.98 | 31,884.38 | 2.105x | 2.077-3.488x |
| Mesh, 128x128, Multiply | 876,077.00 | 504,076.09 | 1.738x | 1.114-2.605x |
| Nearest texture, 32x32, Alpha | 98,305.40 | 90,982.09 | 1.074x | 1.048-1.578x |
| Nearest texture, 128x128, Alpha | 1,579,974.28 | 1,487,744.53 | 1.087x | 0.706-1.642x |
| Linear texture, 32x32, Alpha | 147,018.15 | 152,799.01 | 0.961x | 0.897-1.414x |
| Linear texture, 128x128, Add | 2,365,162.28 | 2,464,261.38 | 0.961x | 0.896-1.400x |
| Linear alpha mask, 128x128, Alpha | 1,782,206.97 | 1,706,885.81 | 1.036x | 0.681-1.540x |
| Empty mesh stripe | 21.76 | 21.98 | 0.984x | 0.936-1.572x |

The [raw CSV](2026-11-09-software-byte-writer-inlining-raw.csv) retains **11,760 observations** across 49 workloads. The [summary CSV](2026-11-09-software-byte-writer-inlining-summary.csv) includes all tested modes, sizes, wall time and unchanged-reference controls. Variant 0 is production; variant 1 uses the frozen byte-writer closure from `04e4b89ec`. Both variants call the same current mesh/textured rasterizers, including the previously added colored-triangle FMA dispatch. The frozen writer body and the production blending/float16-writer bodies are verified against that commit.

Process-to-process ranges are wide. Unchanged-reference control medians span **0.979-1.231x**, exposing frequency, layout and scheduling variation. Within the updated processes, frozen-reference/current cycle ratios for 32x32 Alpha meshes are **2.078-2.107x**. For 32x32 linear-filtered Alpha triangles, five of six ratios are **0.958-0.966x** and one is **1.013x**, supporting the reported small regression as well as the mesh gain. These ranges are observations, not confidence intervals.

Inlining increases the measured library-test executable's `.text` section from **1,599,072 to 1,679,584 bytes**: **80,512 bytes / 5.0%**. The executable grows from 2,002,944 to 2,084,864 bytes. Those binaries include regression fixtures and duplicate reference implementations; this is not an application binary-size measurement.

Measurements used Windows x86-64, an Intel Core i7-1250U, rustc 1.97.1 / LLVM 22.1.6, release opt-level 3, fat LTO, static CRT and generic CPU settings. Six fresh before/after process pairs alternate binary order. The benchmark pins its thread to logical CPU 2, rotates all cases by 0 through 5 and alternates production/reference order within each round. Ten rounds are recorded per case, excluding round zero from summaries while retaining it in raw data. Drawing observations perform `max(100000 / (size * size), 16)` calls; the empty-stripe control performs 100000 calls. Allocation and fixture creation stay outside timing. No build or correctness check from this change overlaps measurement; compiler inventories at both boundaries are empty.

Speedups are medians of six paired process-median cycle ratios. Absolute cycle/time columns are independent medians, so their quotient can differ. Archived binaries, the comparative runner, full benchmark fixtures, manifest, alternative-hint experiment, assembly and check logs remain under `target/assembly-pass4/`.

Validation:

- Baseline and updated software library release suites each pass **32 tests**, with six opt-in/exhaustive tests ignored. The updated headless batching/rendering integration suite passes **35 tests**, with six ignored.
- **179,140 exact writer-pixel comparisons** cover all five blend modes in generic and detected-FMA contexts, arbitrary float bits, normalized inputs, signed zero, subnormals, infinities, NaNs, finite extremes and both sides of every byte-quantization boundary.
- **6,480 complete mesh/textured draw/stripe cases** match the frozen writer exactly, including returned draw counts, nearest/linear sampling, clamp/repeat wrapping, opaque/masked paths, byte/float16 textures, exceptional tint values, partial rows and empty stripes.
- Existing regressions pass **2,435,420 byte/float16 attachment-pixel comparisons**, **295,120 callback float-field comparisons** with zero observed NaN-payload differences, and **2,560 mesh projection/stripe cases**.
- The two new regressions also pass with the software package at opt-level 0, debug assertions and overflow checks enabled, using optimized dependencies.
- All software crate targets pass an ARM64 Linux release compile check. Clippy completes for all targets with existing argument-count, style and duplicate-module diagnostics. The changed Rust blocks and new Rust files pass rustfmt and whitespace checks.

Reproduce the focused checks and current production/reference benchmark:

```powershell
cargo test --release --locked -j 2 -p deadlib-render-backend-software --lib --test batching_pixels -- --test-threads=1
cargo test --release --locked -j 2 -p deadlib-render-backend-software --lib byte_writer_performance -- --nocapture --test-threads=1
$env:BYTE_WRITER_ROTATION = '0'
cargo test --release --locked -j 2 -p deadlib-render-backend-software --lib byte_writer_performance::benchmark_byte_writer_callers -- --exact --ignored --nocapture --test-threads=1
Remove-Item Env:BYTE_WRITER_ROTATION
cargo check --release --locked -j 2 -p deadlib-render-backend-software --target aarch64-unknown-linux-gnu --all-targets
cargo clippy --release --locked -j 2 -p deadlib-render-backend-software --all-targets --no-deps
```

SHA-256:

- Before executable: `1c966ffda0ebf5480f464a75a40aaa466242eaaab371041281212ad529e18e9f`.
- After executable: `1426b52555f1d88cb2a049d92851911dc3c3b00c59ce9cdd7169b46f48ccebcd`.
- Benchmark loop: `fc51690c6bb4d16c53d13b87d9e69f1720d37693e87efef7e606fcce056146ba`.

Window/GPU runtime, complete application throughput, physical non-FMA CPUs and ARM64 runtime were not tested. Float16 attachments are covered for correctness; their performance was not measured.
