# Binary16 encoding, decoding, and capture conversion — 2026-10-25

Three safe Rust optimizations simplify software-renderer pixel conversions. Both Lua crates and the pre-existing Lua working-tree changes are excluded.

## Changes and work removed

1. **Encode float attachments:** one unsigned range check selects the common normal binary16 conversion, replacing three ordered exponent/special-value checks. Remove the signed exponent calculation; derive the remaining subnormal shift directly from the float bits. The original ties-to-even rounding and signed canonical NaNs remain unchanged.
2. **Decode float textures:** replace the four-arm exponent match with a zero-exponent check and direct exponent rebiasing. Subnormals and zero use exact integer-significand scaling by `2^-24`, removing leading-zero counting, variable normalization shifts, and the separate zero case. The result is a normal f32 even for the smallest nonzero binary16 subnormal. Sign bits and all NaN payload bits remain unchanged.
3. **Convert float captures to bytes:** go directly from binary16 bits to a normalized byte. Remove the intermediate float decode, clamp, multiply, and floating-point round for every channel. Negative values and NaNs produce zero, values at least one saturate to 255, and values below `2^-9` round to zero. For the remaining range, the 11-bit significand times 255 fits in 19 bits and was exactly representable in the original f32 calculation. Integer rounding therefore produces precisely the same byte. The original half-pixel and depth copies, including padding, remain intact.

Production changes remove 21 lines and add 36, a net **15 lines**, including comments and the direct byte converter. Execution loses redundant checks, normalization work, and the float conversion pipeline. No production `unsafe`, dependency, CPU feature requirement, public API, runtime lookup table, or dispatch layer is added. Existing benchmark helpers supply allocation counters, thread-cycle accounting, and affinity.

## Measurement method

Intel Xeon E5-2696 v4, Windows x86_64, rustc 1.99.1 / LLVM 23.1.1. All stages use release optimization level 3, `--config profile.release.lto=false`, and the repository's static MSVC CRT configuration. These are local CPU-kernel results, not whole-game FPS measurements.

Each optimization was benchmarked before editing its production implementation and again after the edit. Source snapshots and executables were archived at every stage. The final comparison rebuilds all four stages with the **same final test harness**:

- `before`: unchanged production source at `3f3152be3`.
- `encode`: only change 1.
- `decode`: changes 1 and 2.
- `after`: all three changes.

Four fresh processes per stage rotate execution order, so each stage occupies each position once. Every benchmark thread is pinned to logical processor 2. Each measurement discards one warmup batch and reports the median of eleven batches ranked by thread CPU cycles, using Windows `QueryThreadCycleTime`. The tables report medians across four processes. CPU throughput is before cycles divided by after cycles. Ratio ranges are the minimum and maximum corresponding process ratios, not confidence intervals. Elapsed nanoseconds are secondary.

Fixture creation and allocation probes stay outside timing. Each operation consumes opaque inputs and writes observable output. No task-owned compilation overlaps the reported runs. The machine is shared; affinity and thread CPU accounting do not eliminate all hardware contention.

- Encoding/decoding operations convert 4,096 values into preallocated output arrays. `normal` spans positive binary16 values from `0x2000` through `0x3c00`; `subnormal` includes zero and signed subnormals; `mixed` permutes across the full 16-bit encoding space, including infinities and NaNs. Encoding inputs are prepared with the frozen original decoder.
- Capture operations call the actual `copy_target_pixels::<true>` on an entire float target. `normal` and `mixed` use the same distributions as above, with different offsets for each RGBA channel. Output allocation is excluded; the actual half-storage copy and byte conversion are included.
- Writer controls call the actual `half_writer` over 4,096 pixels with Alpha or Multiply blending. Output accumulates between calls; multiplication eventually decays to zero. Blending and float conversions remain in the measurement.

## Isolated changes

| Change / workload | Before stage | After stage | Before cycles/op | After cycles/op | CPU throughput |
| --- | --- | --- | ---: | ---: | ---: |
| Encoding: 4,096 normal values | `before` | `encode` | 15816.74 | 13108.32 | 1.207x |
| Decoding: 4,096 normal values | `encode` | `decode` | 14227.77 | 7346.89 | 1.937x |
| Capture: normal 1920x1080 | `decode` | `after` | 129195359.16 | 61740630.17 | 2.093x |
| Capture: mixed 1920x1080 | `decode` | `after` | 121152586.16 | 25226037.67 | 4.803x |

## All stages and controls

Cycles/op below cover a complete array or capture, rather than one channel.

| Workload | Before | Encode | Decode | After | Total CPU throughput | Process ratio range |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `half/capture/mixed/1920x1080` | 125293965.00 | 121244628.66 | 121152586.16 | 25226037.67 | 4.967x | 4.598-5.129x |
| `half/capture/mixed/512x512` | 15346782.00 | 14797555.67 | 14739095.83 | 3033928.67 | 5.058x | 4.702-5.544x |
| `half/capture/mixed/64x64` | 229358.29 | 222461.06 | 224006.33 | 44324.74 | 5.174x | 4.871-5.544x |
| `half/capture/normal/1920x1080` | 135967774.67 | 133346067.17 | 129195359.16 | 61740630.17 | 2.202x | 2.062-2.228x |
| `half/capture/normal/512x512` | 16617138.16 | 16316423.00 | 16528093.67 | 7707998.17 | 2.156x | 2.122-2.189x |
| `half/capture/normal/64x64` | 253160.33 | 259164.74 | 243575.86 | 118131.61 | 2.143x | 1.978-2.243x |
| `half/decode/mixed/4096` | 16100.76 | 15870.29 | 7102.59 | 7288.49 | 2.209x | 2.159-2.414x |
| `half/decode/normal/4096` | 14552.63 | 14227.77 | 7346.89 | 7056.70 | 2.062x | 1.743-2.191x |
| `half/decode/subnormal/4096` | 23948.76 | 23252.29 | 7237.80 | 7146.70 | 3.351x | 3.187-3.676x |
| `half/encode/mixed/4096` | 19402.26 | 16376.90 | 15412.63 | 15037.72 | 1.290x | 1.203-1.349x |
| `half/encode/normal/4096` | 15816.74 | 13108.32 | 13262.63 | 12759.31 | 1.240x | 1.199-1.306x |
| `half/encode/subnormal/4096` | 30838.88 | 30307.47 | 30217.47 | 29974.70 | 1.029x | 0.979-1.062x |
| `half/writer/Alpha/4096` | 311400.26 | 311229.05 | 298774.63 | 312543.86 | 0.996x | 0.921-1.101x |
| `half/writer/Multiply/4096` | 318347.42 | 295440.40 | 284419.31 | 290334.84 | 1.096x | 1.027-1.131x |

Normal/mixed encoding, all decoding cases, and capture improve in every measured process. Subnormal encoding and the Alpha writer control are essentially unchanged. The Multiply writer has a smaller gain. Writer controls include blending work, and these results do not imply equivalent rendering speedups.

## Correctness and allocations

All **224 operation probes** (14 workloads x 4 stages x 4 processes) report zero allocations, reallocations, frees, requested/freed bytes, and peak bytes. These optimizations preserve allocation-free execution; they do not claim allocation reductions.

The two-crate release suite passes **151 tests across nine executables**, with 22 benchmark tests ignored in its normal run. All four staged integration binaries also pass their correctness suites. Clippy completes successfully with existing repository warnings; none originates in the changed conversions or new test code.

Differential checks compare every one of the 65,536 binary16 encodings with the frozen original decoder **by f32 bits**, including signed zero and signaling/quiet NaN payloads. Complete float-target captures exercise every encoding in every channel and compare the output bytes to the original float pipeline, for both alpha modes and full/padded viewports with depth. Encoding checks cover 524,288 distributed f32 bit patterns, every midpoint between adjacent finite binary16 values and its two neighboring f32 values for both signs, and existing overflow/ties-to-even fixtures. Existing raster, depth, texture, batching, striped-rendering, and sampler checks pass.

All integer shifts in the direct byte converter are bounded: its nontrivial path has exponents 6–14, so shifts are 11–19 and the rounded product fits u32. Decoder scaling is exact and never produces an f32 subnormal. Existing bit-level NaN construction is preserved. No new unsafe soundness obligation is introduced.

## Reproduction

```powershell
cargo test --release --locked --config profile.release.lto=false -p deadlib-render-core -p deadlib-render-backend-software --lib --tests
cargo clippy --release --locked --config profile.release.lto=false -p deadlib-render-core -p deadlib-render-backend-software --lib --tests
cargo test --release --locked --config profile.release.lto=false -p deadlib-render-backend-software --test half_kernels -- 'half_kernels::benchmark_' --ignored --nocapture --test-threads=1
python scripts/benchmark_half_kernels.py --before target/half-kernels/before/half_kernels.exe --encode target/half-kernels/encode/half_kernels.exe --decode target/half-kernels/decode/half_kernels.exe --after target/half-kernels/after/half_kernels.exe --output docs/performance/2026-10-25-half-kernels.tsv
```

To reconstruct stage archives, use separate worktrees at `3f3152be3` and this commit. Copy `tests/half_kernels.rs` and `tests/support/half_original.rs` from this commit into the same crate paths in the original worktree. No harness edits are needed. Build the original tree for `before`; apply only the `encode_half` change for `encode`, then only the `decode_half` change for `decode`; build this commit for `after`. Use the same release arguments throughout and archive the `half_kernels` executable paths emitted by Cargo's `--message-format=json` compiler artifacts. Run the comparison after all builds finish.

The committed [TSV](2026-10-25-half-kernels.tsv) contains every final measurement and allocation counter. Local source snapshots, executables, initial and final build/test logs, lint output, and process logs are under `target/half-kernels`.

| Stage | Executable SHA-256 |
| --- | --- |
| `before` | `4fb53393fd0d06d07b11275f2488eb8dd775879ad5b4a8ea3d466b56971d8948` |
| `encode` | `5502d0f2f969e0f41260ce856445ff29e4949b96e0159734f2d20c15f56c9e70` |
| `decode` | `29bdfc88d958519e7d5f8bcecf7482297e8d76fa4a9079cada10bfb70dc1d108` |
| `after` | `bf8247ca48511103ad12d16c36bd3694b7c52a4ab2ead5bdd3f9b3fa1254aad8` |
