Mesh-area scans used to select among multiple clipping masks now resolve vertex storage once, visit complete triangles through a slice, and select a hardware-FMA helper on supported x86 CPUs. This removes the repeated shared/reusable/transient storage branches and two CRT `fmaf` calls per triangle seen in the previous optimized assembly. Sprite areas and meshes with fewer than three vertices return before feature detection; other CPUs and architectures use the shared portable triangle loop.

The projection formula, triangle order, fused cross product, sequential area accumulation and incomplete-tail behavior remain unchanged. Mask-selection control flow is byte-identical to `6b94c5be9`; strict area comparisons, ties, payloads and pool management retain their behavior. There is no parallel floating-point reduction, approximation, affine-projection shortcut, inline assembly, explicit intrinsic, new dependency, state field or allocation. Unsafe allowances are local to runtime-checked dispatch and the private feature-specific helper, following the existing presentation conventions.

Archived Cargo release library-test executables measure **1.67-2.08x** faster nonempty area calls. Complete two-mask selection improves **1.52x / 1.65x** for contained meshes with 96 / 768 vertices and **1.10x / 1.12x** for partially clipped meshes of those sizes. These measure actual private area and complete mask-selection calls through non-inlined wrappers; complete frame, GPU and application performance are unmeasured.

| Area workload | Before cycles | After cycles | Median speedup | Six-pair range |
| --- | ---: | ---: | ---: | ---: |
| Shared, affine, 3 vertices | 30.96 | 18.56 | 1.668x | 1.664-1.678x |
| Shared, affine, 6 vertices | 53.04 | 27.02 | 1.961x | 1.947-1.973x |
| Shared, affine, 18 vertices | 138.50 | 70.30 | 1.970x | 1.962-1.975x |
| Shared, affine, 96 vertices | 714.08 | 360.32 | 1.982x | 1.978-2.029x |
| Shared, affine, 768 vertices | 5,526.35 | 2,755.73 | 2.005x | 1.993-2.013x |
| Reusable, affine, 96 vertices | 750.66 | 360.68 | 2.082x | 2.077-2.086x |
| Transient, affine, 96 vertices | 749.31 | 360.05 | 2.080x | 2.075-2.127x |

Perspective cases have similar medians: 1.671x at three vertices, 1.981x at 96, and 2.005x at 768. All perspective results and unchanged-body controls are retained in the [summary CSV](2026-11-09-mesh-area-fma-summary.csv).

| Complete two-mask selection | Before cycles | After cycles | Median speedup | Six-pair range |
| --- | ---: | ---: | ---: | ---: |
| Rejected, 6 vertices | 84.25 | 78.52 | 1.070x | 0.997-1.673x |
| Contained, 6 vertices | 142.49 | 116.34 | 1.227x | 1.169-1.607x |
| Partial, 96 vertices | 2,591.95 | 2,335.42 | 1.104x | 1.048-1.128x |
| Contained, 96 vertices | 1,027.51 | 675.78 | 1.523x | 1.513-1.537x |
| Partial, 768 vertices | 20,724.44 | 18,507.30 | 1.119x | 1.042-1.139x |
| Contained, 768 vertices | 6,981.50 | 4,214.96 | 1.654x | 1.652-1.670x |

There is a small measured cost on short inputs: shared zero/two-vertex medians are **0.961-0.964x**, roughly **0.3 additional CPU cycles per call**, even though feature detection is bypassed. The sprite control measures 1.099x with unchanged arithmetic. Frozen-reference control medians span **0.928-1.068x**. These controls expose layout, scheduling and frequency variation; they do not establish that all unaffected paths are equally fast. The rejected six-vertex fixture is named `partial-6` in the raw data because it uses the partial-mask set, but its geometry lies outside both masks. Its range crosses parity and includes an outlier; it is a rejection control. Larger area scans and the larger complete-mask workloads improve in every process pair. Ranges are extrema, not confidence intervals or guarantees on other hardware.

The [raw CSV](2026-11-09-mesh-area-fma-raw.csv) retains all **5,520 observations**, including warmup round zero. Variant 0 is production; variant 1 calls frozen original area or mask-selection control flow. Summaries exclude round zero, compute each process median, then report the median and range of six paired before/after ratios. Absolute cycle/time columns are independent medians, so their quotient can differ from the paired speedup.

Measurements used Windows x86-64, an Intel Core i7-1250U, rustc 1.97.1 / LLVM 22.1.6, opt-level 3, fat LTO, static CRT and generic CPU settings. Each process pins its benchmark thread to logical CPU 2 and records `QueryThreadCycleTime` and `Instant`. Six fresh before/after process pairs alternate binary order; area-case order rotates by 0 through 5. Complete-mask workloads retain their fixed order, while production/reference order alternates within each round.

Each process measures ten rounds per case. Area iterations are `max(300000 / max(vertex_count, 3), 512)`; complete-mask iterations are `max(100000 / vertex_count, 128)`. Fixture construction is outside timing. Area timing includes the complete object-area call and output observation. Mask timing additionally includes cloning a shared source, bounds, clipping, candidate selection, output observation and recycling transient vertices into a warmed pool. Both variants use identical fixture and timing code. No build or correctness check from this change overlaps timing, and compiler inventories at both measurement boundaries are empty.

The initial standalone prototype measured roughly 1.87-2.06x for runtime-selected FMA on nonempty scans. Its slice-only variant measured about 1.09-1.13x faster than enum indexing, separating the storage traversal change from hardware FMA. These prototype wall-time measurements are exploratory; the Cargo measurements above are the production results. Probe source, assembly, all 288 prototype observations, archived executables, comparative runner, manifests and validation logs remain under `target/assembly-pass2/`.

Validation:

- Baseline and updated library release suites each pass **203 tests**, with 14 opt-in benchmarks ignored. The updated masked-render integration executable passes **180 tests**, with 13 ignored.
- **5,473 area cases** compare runtime-selected and explicitly portable execution with frozen original code. Cases cover all three vertex storage kinds, every length through 40 and larger boundary lengths, incomplete triangle tails, affine/perspective/singular transforms, near-zero/negative homogeneous W, signed zero, subnormals, overflow, infinities, signaling/quiet NaNs and deterministic random IEEE inputs. All non-NaN results require exact bits. NaNs require matching classification and count payload differences: release observes 55 payload differences on the runtime-selected path and zero on the portable path. NaN payload selection is unspecified, and area comparisons retain their behavior.
- **972 complete mask-selection cases** match frozen control flow, including payload data, instance parameters, metadata, storage kind, pool capacities, mask order, rejection, contained/partial geometry and ties. The reference keeps original area computation and shares unchanged production bounds, projection and clipping helpers.
- Both new regressions pass with the presentation package at **opt-level 0**, debug assertions and overflow checks enabled, using cached release dependencies. They again compare 5,473 area cases and 972 complete selections, with zero observed NaN-payload differences in this configuration.
- The ARM64 Linux release compile check passes. Clippy completes for all crate targets with existing diagnostics and no warning in changed area code or the new regression/reference files. The changed area block and both new Rust files pass rustfmt; whitespace checks pass.
- Final fat-LTO composition-test assembly uses **`vfnmadd231ss` and `vfmadd231ss`** in the specialized triangle loop, with no CRT `fmaf` call or bounds-panic call in that helper. The accumulator remains sequential. The complete original and optimized area assembly is retained locally.

Another candidate, desaturation's multiply-by-one FMAs, already compiles to ordinary additions; its proposed rewrite produced the same optimized instruction sequence and was excluded from this change. Shared RGBA interpolation still has four scalar FMA calls in a generic standalone build; this pass concentrates on the measured mesh scan and its complete mask-selection caller.

Reproduce the checks and current production/frozen-reference comparison:

```powershell
cargo test --release --locked -j 2 -p deadlib-present --lib --test masked_render -- --test-threads=1
cargo test --release --locked -j 2 -p deadlib-present --lib compose::area_performance -- --nocapture --test-threads=1
$env:AREA_BENCH_ROTATION = '0'
cargo test --release --locked -j 2 -p deadlib-present --lib compose::area_performance::benchmark_mesh_areas -- --exact --ignored --nocapture --test-threads=1
Remove-Item Env:AREA_BENCH_ROTATION
cargo check --release --locked -j 2 -p deadlib-present --target aarch64-unknown-linux-gnu
cargo clippy --release --locked -j 2 -p deadlib-present --all-targets --no-deps
```

The archived before/after binaries use the same benchmark loop and fixtures; this is verified against the pre-change source snapshot. Frozen originals are from `6b94c5be9`.

SHA-256:

- Before executable: `beae67ff724afab4a8df3ac03352f0b205d7c465870290fd62b392bdfc0ae769`.
- After executable: `cd9b6784437f86f5abcc8aaae9b878eb3b7ca4eb6b685e863aee0b6a07cb1d9e`.
- Benchmark loop and fixtures: `f1f0f5647ca5823bef4b86f4efad304b43d12dcfc481f1d4141a9c95dd538361`.

Full application/GPU testing and physical non-FMA or ARM64 runtime testing were not performed. The full application assembly build remains unavailable because `shaderc-sys` requires a missing Ninja executable; the presentation crate and its tests build independently.
