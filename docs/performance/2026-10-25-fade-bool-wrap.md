# Faded PCM, Boolean validation and repeated texture sampling

Three independent changes against `8cfb89586469b8d9b1b54194cf99a5b8821fbf5e`.
Both Lua crates are excluded. No production unsafe, dependencies, public APIs,
target features or compiler flags were added.

## Work removed

1. **Faded PCM:** replace the inner per-sample `round` call with truncation and
   exact fractional correction. The envelope already bounds the product to the
   PCM range, so no additional clamp is needed. Endpoint calculation, fused
   interpolation, near-unity skipping and incomplete-frame handling stay intact.
2. **Boolean decoding:** OR four eight-byte words before testing their invalid
   bits. Valid 32-byte blocks need one conditional validity check instead of
   four. The existing ordered word scan handles a failing block and the tail;
   there is no separate byte-by-byte error search. The first invalid value,
   decoded prefix and reader position are preserved.
3. **Repeated texture sampling:** return an already in-range index directly for
   non-power-of-two dimensions. UV wrapping already puts almost every index in
   range. Nearest sampling avoids up to two signed remainders per pixel, and
   bilinear sampling avoids up to four. Edge indices retain the original signed
   remainder path. Power-of-two masking and clamp behavior are unchanged.

The production diff is **25 added / 3 removed lines**, including comments. It
removes executed calls, conditional checks and division operations rather than
minimizing source length. There are no new production functions, types, caches
or indirection layers. Allocation churn was already zero and remains zero.

## Measurements

Windows x86-64, Intel Xeon E5-2696 v4 @ 2.20 GHz, rustc 1.99.1
(`58cf8f9c3`, LLVM 23.1.1). Normal release settings: opt-level 3, full LTO,
static MSVC runtime, baseline target CPU.

Every change was benchmarked before implementation and after its individual
trial. For the final comparison, all three baseline production files were
restored from the revision above, built with the final benchmark sources, and
then restored to the retained implementations and rebuilt with identical Cargo
arguments. Executables were archived before running the comparisons.

`tests/support/kernel_bench.rs` pins the benchmark thread to logical CPU 2 and
reads Windows `QueryThreadCycleTime`. Each process reports the median of eleven
measured batches after a warmup. Four process runs alternate before/after order.
The tables give medians of those four process medians. Throughput is before
cycles divided by after cycles; ranges are observed paired run ratios, not
confidence intervals. These are scheduled thread-cycle measurements, not
retired-instruction PMU counts or whole-game FPS measurements.

Inputs, image storage and output capacities are prepared outside timing. The
fade benchmark includes resetting its output from the same input before each
call. Function pointers, channel counts and sampler inputs are opaque to the
optimizer. Allocation probes are separate from cycle timing: **all 464 probes
record zero allocations, reallocations, frees, allocated/freed bytes and peak
churn bytes**. Existing resident buffers are outside those probes.

### Faded PCM

| Channels / frames | Before cycles/call | After cycles/call | CPU throughput |
| --- | ---: | ---: | ---: |
| 1 / 64 | 1,031.70 | 873.46 | 1.181x |
| 2 / 64 | 1,609.13 | 1,324.26 | 1.215x |
| 6 / 64 | 3,760.35 | 3,093.38 | 1.216x |
| 1 / 256 | 3,900.06 | 3,353.24 | 1.163x |
| 2 / 256 | 6,401.77 | 5,271.57 | 1.214x |
| 6 / 256 | 14,598.77 | 12,179.16 | 1.199x |
| 1 / 4096 | 63,878.55 | 49,530.99 | 1.290x |
| 2 / 4096 | 104,143.30 | 80,725.62 | 1.290x |
| 6 / 4096 | 236,366.86 | 198,893.54 | 1.188x |

Every paired run improves each fade workload; observed paired ratios span
1.111–1.408x. An exploratory benchmark that also ran unrelated audio kernels
showed smaller fade improvements, roughly 1.06–1.12x, alongside substantial
variation in unchanged controls. The final dedicated fade test isolates the
operation and uses identical sources in both builds.

### Boolean decoding into reused output storage

| Input | Before cycles/call | After cycles/call | CPU throughput |
| --- | ---: | ---: | ---: |
| 256 valid bytes | 78.25 | 45.97 | 1.702x |
| 4096 valid bytes | 754.98 | 436.30 | 1.730x |
| 65536 valid bytes | 15,787.53 | 12,489.56 | 1.264x |
| 4096 bytes, middle error | 424.15 | 283.14 | 1.498x |
| 4096 bytes, last-byte error | 808.73 | 504.67 | 1.602x |

The valid 4096-byte workload improves in every paired run (1.157–2.076x).
The largest buffer is more variable (0.995–1.418x), so its median improvement
does not mean every run wins. Tiny buffers and immediate errors are slower in
several controls by about 1–6 cycles/call: for example, seven valid bytes cost
30.34 → 34.87 cycles, and an error in their first byte costs 26.48 → 32.50 cycles.
The retained change targets sustained validation throughput; it does not claim
a benefit for every input size or failure position. Single Boolean decoding is
outside this function.

### Texture sampling: cycles per 1024 RGBA samples

| Dimensions / repeat filter | Before cycles | After cycles | CPU throughput |
| --- | ---: | ---: | ---: |
| 127×127 / nearest | 42,327.06 | 29,487.16 | 1.435x |
| 127×127 / linear | 133,323.96 | 98,824.29 | 1.349x |
| 128×127 / nearest | 37,736.43 | 29,818.99 | 1.266x |
| 128×127 / linear | 119,622.78 | 93,931.85 | 1.274x |
| 640×360 / nearest | 44,725.27 | 31,065.68 | 1.440x |
| 640×360 / linear | 143,062.55 | 108,077.17 | 1.324x |

All six targeted workloads improve in every paired run. The 128×128
power-of-two repeat controls are 1.011x nearest and 0.972x linear. Clamp
controls span 0.968–1.009x; those are not counted as gains. Targeted paired
ratios span 1.034–2.002x. Individual clamp-control run ratios span
0.721–1.381x, limiting the precision of smaller effects on this shared host.

Raw cycles, nanoseconds, all slower controls, allocation counters and executable
SHA-256 hashes are retained in the [fade TSV](2026-10-25-fade-bool-wrap-fade.tsv),
[Boolean TSV](2026-10-25-fade-bool-wrap-bool.tsv) and
[sampling TSV](2026-10-25-fade-bool-wrap-sampling.tsv).

## Correctness and checks

Faded products are finite and bounded by the i16 input and an envelope in
[0, 1]. Truncation leaves an exactly representable fractional part in (-1, 1).
Comparisons at ±0.5 correct ties away from zero; the result still fits in i16.
The differential tests cover every i16 value across rising, falling and extreme
envelopes, plus 600 varied buffers with zero through ten channels, incomplete
frames, and interpolation/near-unity boundaries. The frozen reference was
verified against unchanged production before applying the fade optimization.

Boolean tests cover every invalid byte value at every position across two
32-byte blocks and their tail, first-error precedence, partial decoded output,
reader consumption, borrowed decoding, array/vector paths and truncated input.
OR reduction detects an invalid bit in any word; the original ordered scan
still selects the earliest invalid byte.

The repeat-index shortcut is used only when `0 <= i < m`, where `i % m == i`.
Index tests cover dimensions 1–513, negative and positive ranges, zero-sized
axes, large dimensions and i32 extremes. Existing renderer tests cover sampling
with nonfinite coordinates, incomplete texel storage, every binary16 alpha
encoding, raster paths, edge wrapping, depth and retained rendering.

Validation:

```powershell
cargo test --release -p deadlib-audio-decode -p deadlib-bincode -p deadlib-render-backend-software
cargo test -p deadlib-audio-decode --test fade_equivalence -p deadlib-bincode --test bool_validation -p deadlib-render-backend-software --test texture_pipeline
cargo clippy -p deadlib-audio-decode -p deadlib-bincode -p deadlib-render-backend-software --lib --tests
```

The full release run passes 268 tests, including doctests and the renderer
checks included by its integration test binaries. The focused debug run passes
33 tests. Clippy completes with existing warnings; warnings introduced in the
new test code were corrected. `git diff --check` passes for the changed files.

## Reproduction

Use a separate checkout at the baseline revision and the retained revision.
Copy these final test sources into the baseline checkout before building:

- `crates/deadlib-audio-decode/tests/slice_kernels.rs`
- `crates/deadlib-audio-decode/tests/fade_equivalence.rs`
- `crates/deadlib-bincode/tests/direct_kernels.rs`
- `crates/deadlib-render-backend-software/tests/texture_pipeline.rs`

Build both checkouts with the same toolchain, configuration and command:

```powershell
cargo test --release -p deadlib-audio-decode --test slice_kernels --test fade_equivalence -p deadlib-bincode --test direct_kernels -p deadlib-render-backend-software --test texture_pipeline --no-run --message-format=json
```

Archive the `compiler-artifact.executable` paths from each build. With the
archives named as in this run, reproduce the comparisons from the retained
checkout using the existing runner:

```powershell
python scripts/benchmark_slice_kernels.py --before target/direct-loops/final-before-slice_kernels.exe --after target/direct-loops/final-after-slice_kernels.exe --filter benchmark_fade_envelope --output target/direct-loops/repeat-fade.tsv
python scripts/benchmark_slice_kernels.py --before target/direct-loops/final-before-direct_kernels.exe --after target/direct-loops/final-after-direct_kernels.exe --filter benchmark_bool_codec --output target/direct-loops/repeat-bool.tsv
python scripts/benchmark_slice_kernels.py --before target/direct-loops/final-before-texture_pipeline.exe --after target/direct-loops/final-after-texture_pipeline.exe --filter benchmark_repeat_sampling --output target/direct-loops/repeat-sampling.tsv
```
