# Software triangle pipeline — 2026-10-26

Three safe Rust optimizations in `deadlib-render-backend-software`, following `rust-performance.md`'s hot-path and CPU-throughput guidance. The production diff changes three existing functions: 13 lines added and eight removed, including six relocated RGB calculations. No production function, type, dependency, public API, CPU requirement, allocation, or unsafe block is added. The Lua crates and other ongoing worktree changes are excluded.

## Execution simplified

1. **Triangle coverage:** combine the three edge predicates with Boolean bitwise operations. This removes short-circuit branches while preserving positive weights, either sign of zero on owned edges, and NaN rejection.
2. **Rejected fragments:** interpolate alpha and test depth before interpolating/scaling RGB in the textured-color rasterizer. Discarded fragments skip six color FMAs and their RGB scaling work. The alpha threshold, depth writes, sample order, emitted fragments, and surviving color calculations are unchanged.
3. **Near-plane classification:** reduce the three all-inside comparisons without short-circuiting. The all-outside test, mixed-triangle clipping loop, intersection arithmetic, and output storage remain unchanged.

## Isolated before/after measurements

Each row compares consecutive source stages: original → coverage → deferred RGB → near-plane classification. Every archive uses the repository's **default release profile, including LTO**. Values are median thread CPU cycles per batch from ten fresh processes per version.

| Change / workload | Before cycles | After cycles | CPU throughput | Paired ratio range |
| --- | ---: | ---: | ---: | ---: |
| Coverage, 4,096 weight triples | 54,140.22 | 44,205.10 | 1.225x | 1.174–1.257x |
| RGB deferral, alpha-rejected triangle | 37,816.01 | 27,673.46 | 1.367x | 1.116–1.643x |
| RGB deferral, depth-rejected triangle | 46,210.24 | 33,527.53 | 1.378x | 1.105–1.600x |
| Classification, 1,024 inside triangles | 52,111.49 | 46,684.35 | 1.116x | 0.482–1.583x |
| Classification, 1,024 mixed triangles | 94,731.80 | 89,497.83 | 1.058x | 0.802–1.355x |
| Classification, 1,024 complete projections | 193,255.49 | 174,214.95 | 1.109x | 0.965–1.345x |

Raw data, including unchanged controls: [coverage](2026-10-26-triangle-coverage.tsv), [RGB deferral](2026-10-26-triangle-rejection.tsv), [classification](2026-10-26-triangle-classification.tsv). Ratio ranges are minimum/maximum ratios of corresponding process runs, not confidence intervals. Classification has substantially more variation than coverage and rejection.

## All three changes together

The [combined comparison](2026-10-26-triangle-final.tsv) uses twenty fresh processes per version, restoring all three original functions in the before executable.

| Workload | Before cycles/batch | After cycles/batch | CPU throughput | Before → after ns/batch |
| --- | ---: | ---: | ---: | ---: |
| Coverage | 53,779.15 | 44,331.87 | 1.213x | 24,530.25 → 20,210.25 |
| Inside clipping | 45,539.67 | 47,735.76 | 0.954x | 20,779.50 → 21,766.50 |
| Mixed clipping | 89,530.76 | 86,866.03 | 1.031x | 40,987.50 → 39,627.00 |
| Complete projection | 174,242.39 | 181,578.08 | 0.960x | 79,489.50 → 82,802.00 |
| Visible rasterization | 46,309.01 | 44,684.71 | 1.036x | 21,208.75 → 20,393.00 |
| Alpha-rejected rasterization | 35,356.51 | 27,935.76 | 1.266x | 16,137.25 → 12,733.50 |
| Depth-rejected rasterization | 43,880.79 | 32,597.94 | 1.346x | 20,014.25 → 14,861.50 |

These are local kernel measurements, not whole-game FPS results. The isolated coverage change slows the raster workloads by approximately 4–6%; deferred RGB work offsets that in the combined build. The isolated classification gain does **not** establish a combined inside-clipping or projection gain: those workloads regress approximately 5% and 4% in the combined comparison. The classification edit is retained for its isolated measured benefit and simpler branch execution, with this limitation explicit. Executable layout and uncontrolled work on the shared machine affect these small kernels; unchanged controls also move between stages.

Every allocation probe reports **zero allocations, reallocations, frees, requested bytes, and peak bytes** before and after. The four TSVs retain all **700 process/workload records**, executable SHA-256 values, elapsed times, and allocation counts.

Rejected trials include carrying an inside-flag array through the clipping loop (mixed clipping regressed), packing four projection divisions into an existing SIMD vector (complete projection regressed), and explicit SIMD coverage masks (the coverage kernel regressed). None remains in production.

## Method and correctness

Windows x86_64 MSVC, Intel Xeon E5-2696 v4, rustc 1.99.1 / LLVM 23.1.1. Builds use `--release --locked`, the static MSVC runtime from `.cargo/config.toml`, the baseline CPU target, and default LTO for every reported measurement.

The existing benchmark helper pins the current thread to logical processor 2 and reads `QueryThreadCycleTime`. Each process discards one warmup and reports the median of eleven batches. The driver alternates process order; the tables use the median of process medians. CPU throughput is before/after CPU cycles; elapsed time is secondary. Fixture setup, allocation probes, and compilation are outside measured batches. No compilation started by this task overlaps the reported comparisons; other work on the shared machine is uncontrolled.

Coverage uses deterministic xorshift weights `[a, b, 1-a-b]`, with `a` and `b` in `[0, 1)`. Clipping uses z in `[-0.5, 0.5)` or `[-2, 2)` with w=1. Complete projection uses generated triangles and a matrix with varying homogeneous w. Each raster batch draws one 32×32 right triangle through the production textured-color rasterizer with a nonzero-alpha texture. The visible, alpha-rejected, and depth-rejected fixtures differ only in vertex alpha and the depth buffer. Inputs and outputs pass through `black_box`.

Frozen references come from `b2494f569`. Correctness coverage includes:

- Coverage predicates for signed zeros, subnormals, infinities, NaNs, all eight edge-ownership combinations, and 100,000 generated float-bit triples.
- Exact clipping lengths and all four output vertices' float bits for 4,096 exceptional combinations and 95,904 generated triangles, including unused output slots.
- Exact projected vertex bits and culling decisions for 20,000 generated triangle/matrix cases and three cull modes, including clipping, exceptional positions, and arbitrary matrix float bits.
- Exact fragment-write indices/color bits and final depth bits for every linear/mask/opaque specialization, all six depth modes, both depth precision settings, and eight alpha inputs including NaN and signed zero.
- Existing pixel, alpha, depth, near-clipping, offscreen capture, texture sampling, stripe, batching, and material tests.

The full release suite with LTO disabled passes **223 tests**; 34 performance tests are ignored. All **four new debug checks pass**. The measured default-LTO after archive passes its normal tests (29 passed, four ignored). Clippy passes with existing repository warnings and no diagnostics in the new test sources. New Rust files pass `rustfmt --check`; the benchmark driver passes Python compilation. Logs and executable archives are under `target/triangle-pipeline/`.

```powershell
cargo test --release --locked --config profile.release.lto=false -p deadlib-render-backend-software --lib --tests
cargo test --locked -p deadlib-render-backend-software --test triangle_pipeline backend::triangle_pipeline -- --test-threads=1
cargo clippy --release --locked --config profile.release.lto=false -p deadlib-render-backend-software --lib --tests
cargo test --release --locked -p deadlib-render-backend-software --test triangle_pipeline --no-run --message-format=json
python scripts/benchmark_sampling.py --runs 20 --test-filter benchmark_triangle_pipeline --log-dir target/triangle-pipeline/runs --before target/triangle-pipeline/final-before/software.exe --after target/triangle-pipeline/classification-after/software.exe --output docs/performance/2026-10-26-triangle-final.tsv
```

To reconstruct the archives, keep the committed test harness and frozen references. Restore `covered`, `rasterize_triangle_tex_color_impl`, and `clip_tmesh_near` from `b2494f569` for `final-before`; enable only the coverage change for `final-coverage`; add RGB deferral for `classification-before`; add classification for `classification-after`. Build each stage with the default-release command above and save the `triangle_pipeline` executable reported by Cargo before changing the next function. Compare consecutive archives with the same driver and `--runs 10` for isolated results.
