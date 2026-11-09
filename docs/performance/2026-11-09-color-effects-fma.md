Sprite and text DiffuseRamp, DiffuseShift and Pulse effects now select an FMA-enabled helper on supported x86 CPUs. LLVM can interpolate RGBA channels and paired scale values together instead of calling the CRT's scalar `fmaf` for each component. Spin, blink and inactive effects bypass CPU feature detection. Other architectures and CPUs without FMA use the existing arithmetic.

The implementation shares the original, always-inlined arithmetic between the runtime and portable paths. It preserves effect timing, elapsed-time clock handling, interpolation order, fused rounding, clamping and rotation. There is no inline assembly, explicit SIMD intrinsic, approximation, global CPU-feature requirement, new dependency, state field or allocation. Unsafe allowances are confined to runtime-guarded calls and the private feature-specific helpers, following the existing presentation conventions.

Archived Cargo release binaries measure **1.881x / 1.981x / 1.398x** faster sprite DiffuseRamp / DiffuseShift / Pulse calls, and **1.846x / 2.070x / 1.408x** for text. These measure complete effect-function calls through non-inlined wrappers, including phase calculation, state copying and runtime dispatch. Actor traversal, complete frame composition, GPU time and whole-application speedups are unmeasured.

| Call | Effect | Before cycles | After cycles | Median speedup | Six-pair range |
| --- | --- | ---: | ---: | ---: | ---: |
| Sprite | None | 21.95 | 21.21 | 1.013x | 0.851-1.731x |
| Sprite | DiffuseBlink | 57.15 | 57.13 | 0.992x | 0.749-1.012x |
| Sprite | DiffuseRamp | 98.50 | 52.75 | 1.881x | 1.825-3.065x |
| Sprite | DiffuseShift | 177.86 | 89.74 | 1.981x | 1.661-3.315x |
| Sprite | Pulse | 127.17 | 90.55 | 1.398x | 1.198-1.450x |
| Sprite | Spin | 84.65 | 85.98 | 0.986x | 0.864-1.062x |
| Text | None | 20.98 | 21.51 | 0.996x | 0.833-1.158x |
| Text | DiffuseBlink | 57.17 | 57.59 | 0.994x | 0.886-1.023x |
| Text | DiffuseRamp | 96.95 | 52.14 | 1.846x | 1.665-1.892x |
| Text | DiffuseShift | 182.40 | 88.10 | 2.070x | 2.040-2.079x |
| Text | Pulse | 126.90 | 90.20 | 1.408x | 1.357-1.508x |
| Text | Spin | 20.93 | 21.51 | 0.996x | 0.872-1.124x |

The [raw CSV](2026-11-09-color-effects-fma-raw.csv) retains all **2,880 observations**. The [summary](2026-11-09-color-effects-fma-summary.csv) includes wall time and an explicitly portable control for every case (variant 0 is production, variant 1 is portable). Speedups are medians of six paired process-median ratios; absolute columns are independent medians, so their quotient can differ. Round zero is excluded from summaries and retained in the raw data.

Spin's primary median is about 1.4% more CPU cost, while no-effect and blink medians are close to parity; their ranges all cross one. Portable-control medians span 0.989-1.184x, including a broad 0.913-1.742x sprite/no-effect control. These unchanged-body controls expose scheduling, frequency and code-layout variation. The substantial gains in the three selected effects persist across every process pair; small control differences are not evidence of a consistent improvement or regression. The ranges are extrema, not confidence intervals or guarantees on other machines.

Measurements used Windows x86-64, an Intel Core i7-1250U, rustc 1.97.1 / LLVM 22.1.6, opt-level 3, fat LTO, static CRT and generic CPU settings. The benchmark thread is pinned to logical CPU 2 and records `QueryThreadCycleTime` and `Instant`. Each process measures ten rounds of 4,096 iterations over 64 effect/state fixtures per case: 262,144 calls per observation. Fresh before/after processes alternate order across six pairs and rotate effect order by 0 through 5. Fixture setup is outside timing; input copying, output observation and the complete effect call are inside. No builds or correctness checks launched for this change overlap timing; no compiler process was observed immediately after measurements.

The archived baseline contains the same benchmark and extracted portable bodies before adding runtime dispatch. Both portable bodies exactly match the frozen arithmetic from `cb214e495`; they were only renamed and wrapped for the baseline build. The benchmark loop and fixtures remain identical between the two binaries. Binaries, build/test logs, source snapshots and the comparative runner are retained in `target/assembly-audit/`.

Validation:

- Baseline and updated library release suites each pass **201 tests**, with 13 opt-in benchmarks ignored. The updated masked-render integration suite passes **178 tests**, with 12 ignored.
- The new sprite and text regressions each compare **53,760 effect states** against frozen baseline functions, on runtime-selected and explicitly portable paths: **1,935,360 float-field comparisons** in total. They cover all twelve modes, both clocks, timing boundaries, zero/negative/nonfinite timing, signed zero, subnormals, overflow, infinities, signaling/quiet NaNs and deterministic random IEEE inputs. Non-NaN results require exact bits; NaN results require matching classification and count payload differences. Release observes one NaN-payload difference in each runtime-selected suite and none on the portable paths. NaN payload selection is unspecified; exact NaN bits are not a portability promise.
- The ARM64 Linux release compile check passes, exercising the architecture-independent fallback.
- Final fat-LTO masked-render assembly contains packed FMA instructions in both specialized helpers (three sites in the sprite helper and two in text), with no scalar `call fmaf` in either. The extracted helper assembly is retained in `target/assembly-audit/color-effects-optimized.asm`.
- Both new regressions also pass with the presentation package at opt-level 0, debug assertions and overflow checks enabled, using cached release dependencies. Cargo's artifact metadata confirms these settings. Each suite again compares 53,760 states and observes no NaN-payload differences in this configuration.
- Clippy for all crate targets completes with existing diagnostics and no warning in the changed effect code or new regression/reference files. The changed effect block and both new Rust files pass rustfmt; whitespace checks pass.

Reproduce the regression and paired production/portable benchmark from the repository root:

```powershell
cargo test --release --locked -j 2 -p deadlib-present --lib --test masked_render -- --test-threads=1
cargo test --release --locked -j 2 -p deadlib-present --lib compose::effect_performance -- --nocapture --test-threads=1
$env:EFFECT_BENCH_ROTATION = '0'
cargo test --release --locked -j 2 -p deadlib-present --lib compose::effect_performance::benchmark_effects -- --exact --ignored --nocapture --test-threads=1
Remove-Item Env:EFFECT_BENCH_ROTATION
cargo check --release --locked -j 2 -p deadlib-present --target aarch64-unknown-linux-gnu
cargo clippy --release --locked -j 2 -p deadlib-present --all-targets --no-deps
```

SHA-256 of the measured executables:

- Before: `dc186e03dc2cd941d1b19067c0ba0505662d59f896bbd497fb98b54d798ea3d9`.
- After: `4e53a44e02e1a8ab339dc4edea2c3cf76a84c1c9b662e463fb46ed0d9645d5c2`.

Full application/runtime testing was not performed. The application assembly build encountered a missing Ninja dependency in `shaderc-sys`; the presentation crate, effect benchmarks and composition tests build independently of it. Physical non-FMA and ARM64 runtime testing were not performed.
