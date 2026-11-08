# Direct software sampling and culling — 2026-10-26

Three safe Rust optimizations in `deadlib-render-backend-software`, following the hot-path, indirection, and CPU-throughput guidance in `rust-performance.md`. Only three existing production functions change. No production function, type, dependency, public API, CPU feature requirement, or unsafe block is added. The Lua crates are excluded.

## Execution simplified

1. **Bilinear texel coordinates:** replace two general `floor()` calls with saturating integer conversions and a negative-edge correction. Wrapped coordinates put the scaled value in `[-0.5, size - 0.5]`, or NaN. Truncation already floors nonnegative values; negative finite values in this interval need exactly one subtraction. NaNs retain their original zero integer conversion.
2. **Offscreen raster bounds:** reject a rectangle when either floating-point upper bound is at or below `-1`. Its ceiling is negative, so it cannot reach a nonnegative raster coordinate. Both comparisons feed one branch through Boolean bitwise OR. This skips both axes' rounding, conversions, and clipping on that path. The remaining calculations are unchanged. Dimension subtraction still happens before rejection, preserving the existing invalid-dimension overflow behavior.
3. **Repeated border texels:** map index `size` directly to zero and index `-1` directly to `size - 1` for positive sizes. These are the bilinear filter's two border cases. They avoid signed integer division. The existing power-of-two path and general remainder fallback remain, including unusual sizes and signed-remainder overflow behavior.

The production diff adds 18 lines and removes five, including comments. It removes two general rounding calls from every bilinear sample and avoids intermediate arithmetic and library calls on rejected geometry and border texels. The extra guards express the numeric domains directly; there is no new abstraction layer or cache.

## Isolated before/after measurements

Each row compares consecutive stages: original → bilinear conversion → offscreen rejection → border wrapping. All builds use the repository's **default release profile, including LTO**. Numbers are median thread CPU cycles per batch from six fresh processes per version.

| Change / workload | Before cycles | After cycles | CPU throughput | Paired ratio range |
| --- | ---: | ---: | ---: | ---: |
| Bilinear sampling, clamp | 412,950.84 | 357,745.49 | 1.154x | 1.100–1.221x |
| Bilinear sampling, repeat | 470,930.67 | 434,547.46 | 1.084x | 0.973–1.164x |
| Mixed visible/offscreen bounds | 166,215.27 | 85,703.77 | 1.939x | 1.836–2.107x |
| Repeated border indices | 94,789.98 | 44,076.70 | 2.151x | 2.087–2.212x |
| Complete sampling, 3×3 repeated texture | 502,736.21 | 451,478.58 | 1.114x | 0.954–1.170x |

Raw data, including unchanged controls: [bilinear conversion](2026-10-26-direct-sampling-floor.tsv), [offscreen rejection](2026-10-26-direct-sampling-bounds.tsv), [border wrapping](2026-10-26-direct-sampling-edges.tsv). Ratio ranges are minimum/maximum ratios of corresponding process runs, not confidence intervals.

Sampling and bounds batches contain 4,096 calls. Border-index batches contain 4,096 pairs, or 8,192 calls. Every allocation probe for the changed workloads reports **zero allocations, reallocations, frees, and requested bytes before and after**. Fixture creation and allocation probes are outside CPU timing.

## All three changes together

The [final comparison](2026-10-26-direct-sampling-final.tsv) restores all three original production functions in the before binary and enables all three changes in the after binary.

| Workload | Before cycles/batch | After cycles/batch | CPU throughput | Before → after ns/batch |
| --- | ---: | ---: | ---: | ---: |
| Sampling, clamp | 400,282.40 | 362,824.71 | 1.103x | 183,104.0 → 165,903.0 |
| Sampling, repeat | 473,423.08 | 434,769.12 | 1.089x | 216,693.5 → 198,949.0 |
| Mixed raster bounds | 166,901.21 | 92,309.63 | 1.808x | 76,244.5 → 42,092.5 |
| Repeated border indices | 92,398.52 | 47,113.47 | 1.961x | 42,151.5 → 21,595.5 |
| Sampling, small repeated texture | 526,853.78 | 454,222.33 | 1.160x | 240,673.5 → 207,632.5 |
| Unchanged clamp-UV control | 23,367.97 | 24,059.40 | 0.971x | 10,650.0 → 10,981.0 |
| Unchanged repeat-UV control | 177,266.00 | 176,678.86 | 1.003x | 80,950.5 → 81,035.5 |

These are local kernel results, not whole-game FPS measurements. Bounds measurements contain mixed accepted and rejected geometry; an all-visible workload is not measured. The unchanged clamp control regresses about 3%, and controls vary across isolated stages. Different executable layout and shared-machine activity can affect these small kernels; the measurements do not establish gains on other CPUs or build profiles.

Several trials were removed: typed endian-decoder fills, manual fractional-part extraction, changed pixel-slice access, sprite row conversions, integer ceiling conversion, and rejection after horizontal integer clipping. The integer ceiling trial improved with LTO disabled but had no isolated default-LTO benefit. The retained offscreen test replaces that trial and nearly doubles the mixed-bounds throughput under default LTO.

## Method and correctness

Windows x86_64 MSVC, Intel Xeon E5-2696 v4, rustc 1.99.1 / LLVM 23.1.1. All reported archives use `cargo test --release --locked`, the static MSVC runtime from `.cargo/config.toml`, default LTO, and the baseline CPU target. No target flags change between versions.

The existing benchmark helper pins the test thread to logical processor 2 and reads Windows `QueryThreadCycleTime`. Each process discards one warmup batch and reports the median of eleven batches. The Python driver alternates before/after process order and reports the median of six process medians. CPU throughput is before/after CPU cycles; elapsed time is secondary. No compilation or other benchmark started by this task overlaps measured runs. Other activity on the shared machine is uncontrolled.

Coordinates come from a deterministic xorshift sequence over `[-4, 4)`. Sampling uses a non-power-of-two 63×65 RGBA texture with varying alpha, plus a 3×3 texture. Bounds use 640×480 and 32.25-pixel extents. The index probe uses `-1` and `63` with size 63. Inputs and results pass through `black_box`. The four TSVs retain all **336 process/workload records**, allocation counters, elapsed times, and executable SHA-256 values.

Correctness checks compare with frozen production implementations from `2a470896a`:

- Exact sampled float bits for both wrap modes, every float exponent/sign class, special mantissas, zeros, infinities, NaNs, and 100,000 generated float bit patterns.
- Exact raster-bound tuples for 100,000 generated four-coordinate inputs and three clipping/stripe configurations; neighbors of 131,073 integer boundaries and another million generated float patterns.
- Repeat-index equivalence across sizes 0–257 and large/negative-cast size boundaries, indices -1,024–1,024 and integer limits, plus preservation of signed-remainder overflow panics.
- Existing pixel, half-float, alpha, culling, depth, stripe, batching, near-clipping, and offscreen-capture tests.

The final full release suite with LTO disabled passes **194 tests**; 30 performance tests are ignored. All **four new checks pass in debug**. The archived default-LTO target also passes its normal tests. Clippy passes for the library and all integration targets with existing repository warnings and none in the new code. Logs are under `target/direct-sampling/`.

```powershell
cargo test --release --locked --config profile.release.lto=false -p deadlib-render-backend-software --lib --tests
cargo test --locked -p deadlib-render-backend-software --test sampling_kernels sampling_performance -- --test-threads=1
cargo clippy --release --locked --config profile.release.lto=false -p deadlib-render-backend-software --lib --tests
cargo test --release --locked -p deadlib-render-backend-software --test sampling_kernels --no-run --message-format=json
python scripts/benchmark_sampling.py --before target/direct-sampling/before-lto/software.exe --after target/direct-sampling/after-lto/software.exe --output docs/performance/2026-10-26-direct-sampling-final.tsv
```

To reconstruct the archives, keep the test harness and restore the three production function bodies from `2a470896a` for `before-lto`. Enable only the sampler change for `stage1-lto`, then the offscreen rejection for `stage2-lto`, then border wrapping for `after-lto`. Build each with the default-release command above and save the `sampling_kernels` executable reported by Cargo before changing the next function. For isolated measurements, pass consecutive archives to the same Python driver.
