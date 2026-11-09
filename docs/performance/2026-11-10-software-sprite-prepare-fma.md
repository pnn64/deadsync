Sprite geometry preparation now uses a CPU-checked FMA entry point on x86/x86-64. The previous final-LTO `prepare_objects` assembly retains 16 static scalar CRT `fmaf` call sites, including the sprite transform. Inlining the original vertex-preparation body into a private `#[target_feature(enable = "fma")]` function lets LLVM emit native fused operations.

The portable and specialized paths share the original arithmetic from `7753a0a0f`: local offset, rotation, matrix projection, clip-W rejection, perspective division, viewport conversion and UV transformation. No reciprocal approximation, global CPU requirement or allocation is added. Zero-sized viewports return before feature detection; other architectures and CPUs without FMA use the portable implementation. The unsafe call follows runtime CPU/OS detection.

Complete frame-object preparation for **96 sprites improves about 1.50x for plain sprites and 1.66-1.67x for rotated/perspective sprites**. The isolated vertex loop improves **2.60-2.86x** at that count. The complete caller also clears/reserves retained buffers, resolves the texture/camera, filters instances, calculates sprite rows and inverse denominators, and stores prepared objects. These measurements exclude drawing and presentation and do not establish an application FPS gain.

| Workload, sprites per operation | Before cycles | After cycles | Median speedup | Six-pair range |
| --- | ---: | ---: | ---: | ---: |
| Full preparation, plain, 1 | 148.39 | 124.98 | 1.190x | 0.785-1.759x |
| Full preparation, rotated, 1 | 161.21 | 128.72 | 1.255x | 1.248-1.940x |
| Full preparation, plain, 96 | 12,722.93 | 8,492.95 | 1.501x | 1.493-2.242x |
| Full preparation, rotated, 96 | 13,820.86 | 8,308.73 | 1.663x | 1.075-2.575x |
| Full preparation, perspective, 96 | 13,854.94 | 8,312.17 | 1.668x | 0.989-2.464x |
| Full preparation, plain, 3,072 | 512,227.47 | 273,025.25 | 1.880x | 0.887-2.480x |
| Full preparation, rotated, 3,072 | 442,890.52 | 264,838.58 | 1.672x | 0.990-2.441x |
| Full preparation, perspective, 3,072 | 442,481.72 | 265,867.77 | 1.665x | 1.041-2.539x |
| Full preparation, rejected clip-W, 96 | 4,366.84 | 1,339.28 | 2.849x | 1.782-4.328x |
| Full preparation, overflow, 96 | 16,115.69 | 8,236.24 | 1.607x | 1.034-2.426x |
| Full preparation, zero width, 1 | 20.68 | 23.08 | 0.896x | 0.845-1.406x |
| Vertex loop, plain, 96 | 10,951.94 | 4,208.29 | 2.603x | 2.568-3.763x |
| Vertex loop, rotated, 96 | 12,227.49 | 4,275.72 | 2.863x | 1.974-4.156x |

The [raw CSV](2026-11-10-software-sprite-prepare-fma-raw.csv) retains **8,640 observations** across 36 workloads: six fixture types, counts 1/96/3,072, and vertex-only/full-caller paths. The [summary CSV](2026-11-10-software-sprite-prepare-fma-summary.csv) includes both variants, wall time and all controls. Variant 0 is production. Variant 1 freezes the original vertex function and complete `prepare_objects` caller from `7753a0a0f`; it shares unchanged row/setup helpers with production. Both archived binaries contain the identical complete benchmark module.

Unchanged-reference before/after control medians range **0.997-1.299x**, and some paired process ranges are broad. Within updated processes, frozen-reference/current ratios at 96 sprites span **1.508-1.514x** for plain full preparation, **1.611-1.674x** for rotated preparation and **1.559-1.674x** for perspective preparation. Their vertex-only ratios span **2.578-2.612x**, **2.859-3.177x** and **2.856-2.949x**, respectively. These support the common-case gain without treating observed ranges as confidence intervals. The 3,072-plain full-caller paired estimate is inflated relative to its **1.501-1.518x** same-process reference range.

Empty work is not a universal improvement. The one-sprite zero-width caller adds about 2.4 median cycles. Larger zero-width controls measure 1.226x and 0.976x before/after, with wide ranges. The frozen caller can specialize zero-width work much more aggressively than production even in the baseline binary; its same-process ratio is about 0.096x at 96 sprites and 0.007x at 3,072 in the updated binary. This compilation-context discrepancy is retained in the data and is not evidence of a new proportional slowdown. Very short zero-width caller measurements also include clock/loop overhead.

Two exploratory designs were rejected. Recomputing nonfinite output portably made the overflow fixture slower. Bounding every input before dispatch avoided the retry but erased the complete-caller gain. The final dispatch has no value-domain scan or retry. These prototypes, their sources and measurements remain under `target/assembly-pass8/postcheck-candidate/` and `target/assembly-pass8/bounded-candidate/`.

Correctness comparisons require exact rejection decisions and every non-NaN coordinate bit, including infinities and signed zero. Exceptional NaN encodings are explicitly allowed to differ only when both values are NaNs. The baseline library binary reports zero such differences against the frozen copy; the updated binary reports **45,965 differing NaN coordinate encodings**. This change does not guarantee identical NaN payloads/signs or bit-identical geometry for exceptional inputs. Ordinary rendered fixtures require every metadata and attachment bit, without a NaN allowance.

The library-test executable's `.text` virtual size changes from **3,000,784 to 2,999,792 bytes**, a decrease of 992 bytes. File size changes from **3,438,080 to 3,436,032 bytes**. Both contain frozen references and test-only instantiations; application binary size is unmeasured.

Measurements use Windows x86-64, an Intel Core i7-1250U, rustc 1.97.1 / LLVM 22.1.6, generic CPU settings, release opt-level 3, fat LTO and static CRT. Six fresh before/after process pairs alternate executable order. The benchmark pins its thread to logical CPU 2, rotates the 18 count/fixture combinations by 0 through 5, and alternates production/reference order each round. Vertex-only work precedes full-caller work for each combination. Ten rounds are recorded; round zero is retained in raw data and excluded from summaries. Each observation executes `max(100000 / count, 16)` operations, each operation processing the entire sprite packet. Allocation/fixture construction stays outside timing; retained-buffer clearing/reservation stays inside the full caller. Compiler inventories at both measurement boundaries are empty, and no builds or test runs from this change overlap timing.

Speedups are medians of six paired process-median cycle ratios. Absolute cycle/time columns are independent medians, so their quotient can differ. Source snapshots, before/after executables, identical fixtures, measurement runner/manifest, rejected prototypes, assembly and validation logs remain under `target/assembly-pass8/`.

Validation:

- Baseline and updated software library release suites each pass **38 tests**, with ten opt-in/exhaustive tests ignored. Existing renderer comparisons continue to pass.
- **100,000 sprite cases** compare **1,185,392 coordinate fields** and **25,913 rejections**, covering finite values, arbitrary float bits, subnormals, signed zero, huge values, infinities, NaNs, singular/perspective matrices and zero-width viewports. Non-NaN fields and rejection decisions match exactly; the NaN encoding differences are counted above.
- **480 complete frame/stripe cases** compare prepared metadata, vertex counts, draw counts, depth bits and **127,080 exact attachment pixels**. They cover plain/rotated/perspective sprites, two viewport sizes, nearest/linear sampling, clamp/repeat wrapping, all five blend modes, byte attachments, both half-attachment alpha policies and split/empty stripes.
- Updated triangle and batching integration suites pass **42** and **41 tests**, with eleven and ten ignored respectively. The triangle binary also reports 45,965 differing NaN coordinate encodings against the frozen copy; all non-NaN geometry and ordinary rendered comparisons pass.
- Both new regressions pass with the software package at opt-level 0, debug assertions and overflow checks enabled, using optimized dependencies. This build reports zero NaN encoding differences, illustrating compilation-context sensitivity rather than a guarantee of stable encodings in optimized builds.
- ARM64 Linux release checks pass for all software crate targets. Clippy completes with no new warning messages. The new Rust files, changed production block and whitespace checks pass formatting verification.
- The shared arithmetic body and frozen references match the baseline source after whitespace normalization. The production caller, benchmark module and measured source snapshot are unchanged.

Final fat-LTO triangle-test assembly contains **14 native FMA instruction sites and zero direct `fmaf` calls** in `prepare_sprite_vertices_fma`. It retains eight scalar perspective divides with no reciprocal approximation. Generic callers still contain 16 static `fmaf` call sites for portable fallback and other setup; the optimization does not remove those branches from the executable. Assembly and excerpts are archived under `target/assembly-pass8/`.

Reproduce the focused regression and current/reference benchmark:

```powershell
cargo test --release --locked -j 2 -p deadlib-render-backend-software --lib --test triangle_pipeline --test batching_pixels -- --test-threads=1
cargo test --release --locked -j 2 -p deadlib-render-backend-software --lib sprite_prepare_performance -- --nocapture --test-threads=1
$env:SPRITE_PREPARE_ROTATION = '0'
cargo test --release --locked -j 2 -p deadlib-render-backend-software --lib sprite_prepare_performance::benchmark_sprite_preparation -- --exact --ignored --nocapture --test-threads=1
Remove-Item Env:SPRITE_PREPARE_ROTATION
cargo check --release --locked -j 2 -p deadlib-render-backend-software --target aarch64-unknown-linux-gnu --all-targets
cargo clippy --release --locked -j 2 -p deadlib-render-backend-software --all-targets --no-deps
```

SHA-256:

- Before executable: `affa3c43fa6043b0f6f5ecb8867cbf26fc71caa986abd4fa77a1fe483304ba60`.
- After executable: `7695593a3fde07b6b7bd64cfc46a000c11819da720ee0ee1b491e8698456255e`.
- Benchmark loop: `6b2aa0df99dc859865070956d5b2b63b3253eeb962854d11590db796a808e4a3`.

Window/GPU runtime, application FPS, physical non-FMA CPUs and ARM64 runtime were not tested. The correctness comparisons and exceptional-NaN limits are described above.
