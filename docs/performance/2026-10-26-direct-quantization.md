# Direct note, depth, and color quantization - 2026-10-26

Three safe Rust optimizations following the hot-path, throughput, and unsafe guidance in `rust-performance.md`. Baseline production expressions come from `c35aab1b9` and are unchanged by the concurrent Lua commit `2ac6aa7a1`. Both Lua libraries and unrelated worktree edits are excluded.

1. `deadsync-gameplay/src/replay.rs`: replace general f32 rounding in note classification with a widened product, signed half bias, and saturating i32 cast. Preserve the existing lookup table and Euclidean remainder. CPU throughput improves **1.412x** geometrically over three workloads. [Raw classification results](2026-10-26-direct-quantization-notes.tsv).
2. `deadlib-render-backend-software/src/lib.rs`: quantize validated UNORM16 depth directly through i32. Preserve the f32 product/division and restore the original sign, including negative zero. Remove general f32 rounding from the depth-test path. CPU throughput improves **1.226x** geometrically over three quantized-depth workloads. Unquantized controls are reported separately below. [Raw depth results](2026-10-26-direct-quantization-depth.tsv).
3. `deadlib-present/src/color.rs`: convert color channels through a widened half bias and saturating u8 cast. The cast supplies range clamping and NaN-to-zero conversion, removing the explicit clamp and round for each of four channels. Preserve uppercase text, opaque-alpha omission, and exact String capacity. Complete formatting/destruction improves **1.082x** geometrically over three workloads. [Raw color results](2026-10-26-direct-quantization-colors.tsv).

All three keep the original multiplication in f32 before widening. Adding the half in f32 would prematurely round some values just below half-integer boundaries. The f64 bias retains those distinctions. For classification, all non-saturating results fit i32; for validated depth the rounded result fits 0..=65535. Color casts saturate out-of-range values exactly as the removed clamp/round sequence did. No float reassociation is used.

The changes replace three general rounding expressions, executed once per classification/depth check and four times per color. Four runtime channel clamps are eliminated. Production code adds two executable lines and five explanatory comment lines. There are no new helpers, abstraction layers, dependencies, unsafe blocks, or CPU target requirements.

Measurements use Rust 1.99.1 / LLVM 23.1.1, Windows x86-64 MSVC, Intel Xeon E5-2696 v4 @ 2.20 GHz, release opt-level 3, full LTO, static CRT, and the default target CPU. `tests/support/kernel_bench.rs` pins the benchmark thread to logical CPU 2 and reads `QueryThreadCycleTime`. Each process discards one warmup, then selects the median of eleven CPU-cycle samples. Results below are the median of six independent processes per version, with before/after process order alternating. All builds and correctness checks finish before final measurements. Fixture construction and allocation probes remain outside timing.

One batch contains 4,096 production classification calls, 4,096 production depth checks, or 1,024 complete color formats including String destruction. Opaque calls prevent constant folding; reference expressions are used only for correctness. The TSVs retain CPU cycles, elapsed nanoseconds, allocation/free/reallocation counts, byte counts, peak live bytes, and executable SHA-256 hashes. The measurements cover these CPU kernels on the stated host.

| Workload / batch | Before thread cycles | After thread cycles | CPU throughput |
| --- | ---: | ---: | ---: |
| color-hex/bits | 203,497.71 | 188,188.70 | 1.081x |
| color-hex/opaque | 197,401.10 | 174,257.02 | 1.133x |
| color-hex/translucent | 192,442.60 | 185,928.21 | 1.035x |
| depth/false/TestFailWrite | 44,247.18 | 36,493.33 | 1.212x |
| depth/false/TestWrite | 43,279.55 | 35,109.76 | 1.233x |
| depth/false/Write | 40,685.42 | 32,504.65 | 1.252x |
| depth/true/TestFailWrite | 72,939.49 | 57,239.75 | 1.274x |
| depth/true/TestWrite | 68,204.14 | 58,752.46 | 1.161x |
| depth/true/Write | 71,732.97 | 57,604.49 | 1.245x |
| quantization/bits | 117,221.05 | 62,438.60 | 1.877x |
| quantization/chart | 75,361.67 | 63,145.76 | 1.193x |
| quantization/signed | 72,898.51 | 58,051.90 | 1.256x |

Allocation counts and byte counts are identical for every retained workload. Classification and depth remain allocation-free. Formatting retains 1,024 allocations/frees and zero reallocations per batch: 7,168 bytes for opaque colors, 9,210 bytes for translucent colors, and 8,704 bytes for arbitrary-bit colors. Peak live output storage is respectively 7, 9, and 9 bytes. The pre-existing edit to `tests/support/perf.rs` supplied the same allocation instrumentation to both versions and remains outside this commit.

Correctness checks cover all float exponent/sign classes; one million generated beat bit patterns; note-row half ties and neighbors; integral-spacing and i32 saturation boundaries; all 65,535 UNORM16 midpoints and their adjacent f32 inputs; all six depth modes; existing depth values including NaN; signed zero; invalid/nonfinite depths; color channel half ties and neighbors; saturation/NaNs; 100,000 generated color inputs; exact output text and String capacity. Existing software-renderer pixel/depth, batching, near-clipping, sampler, offscreen-capture, presentation, and gameplay suites also pass.

Validation: **1,503 debug tests and 1,503 release tests passed**, with 44 opt-in benchmarks ignored in each full run. The final test-harness lint cleanup also passes focused release checks, the expanded debug classification check, and Clippy on all targets of the three changed crates. Clippy completes with existing repository warnings; new benchmark warnings were fixed. New files are formatted, Python compilation and whitespace checks pass.

Discarded trials retain only measurements: lane-mask counters plus eager hold qualification produced 0.862x geometric throughput across 24 workloads ([rows](2026-10-26-direct-quantization-rejected-rows.tsv)); blend dispatch flattening produced 0.962x across five modes ([blend](2026-10-26-direct-quantization-rejected-blend.tsv)); replacing Boolean scanner chunks with array chunks produced 0.944x across 69 workloads ([Boolean decode](2026-10-26-direct-quantization-rejected-bool.tsv)). Their production changes and temporary harnesses were removed. The six TSVs contain **928 records**, including slower controls and rejected trials.

To reproduce, keep the added test harnesses in both versions and restore only the production expressions being compared. Archive executable paths from Cargo's JSON compiler-artifact records under separate before/after directories, named `quantization_kernel.exe`, `depth_kernel.exe`, and `color_hex_kernel.exe`. Build each pair with identical Cargo options. The final classification/color pairs use the first build command below; the depth pair uses the second. Before measurements, run the archived baseline benchmarks, apply the corresponding production edits, rebuild/archive the after binaries, and finish correctness checks.

```powershell
cargo test --release -p deadlib-present -p deadsync-gameplay --test color_hex_kernel --test quantization_kernel --no-run --message-format=json
cargo test --release -p deadlib-present -p deadsync-gameplay -p deadlib-render-backend-software --test depth_kernel --test quantization_kernel --test color_hex_kernel --no-run --message-format=json
python scripts/benchmark_direct_quantization.py --before target/direct-round/isolated-before --after target/direct-round/retained --target quantization --runs 6 --output target/direct-round/notes.tsv
python scripts/benchmark_direct_quantization.py --before target/direct-round/isolated-before --after target/direct-round/retained --target depth --runs 6 --output target/direct-round/depth.tsv
python scripts/benchmark_direct_quantization.py --before target/direct-round/isolated-before --after target/direct-round/retained --target color --runs 6 --output target/direct-round/colors.tsv
cargo test -p deadlib-present -p deadsync-gameplay -p deadlib-render-backend-software
cargo test --release -p deadlib-present -p deadsync-gameplay -p deadlib-render-backend-software
cargo clippy -p deadlib-present -p deadsync-gameplay -p deadlib-render-backend-software --all-targets
```
