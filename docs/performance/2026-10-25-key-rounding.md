# Single-pass color keys and direct PCM rounding - 2026-10-25

Three safe Rust optimizations in `deadlib-assets` and `deadlib-audio-decode`, based on `d0347267e`. Both Lua crates and concurrent unrelated edits are excluded.

## Changes and removed work

1. **Color-key textures:** replace each block's key-presence reduction and conditional second pass with one packed `u32` equality/mask/store per pixel. Remove the block iterator, Boolean reduction, branch, and rescanning. Edge-based off-pink selection, opaque-only matching, empty images, and unused trailing storage retain their original behavior. The function loses ten lines.
2. **Float64 WAV packets:** replace per-sample `round()` with a signed half bias and the existing saturating integer cast. Preserve the original f64 multiplication, nonfinite-source silence, and finite-product overflow saturation. Guard `abs(scaled) < 0.5`: adding a half to the representable value immediately below 0.5 can otherwise round to 1. For larger in-range half-integer thresholds, the spacing below the threshold is no smaller than the spacing below its integer result, so adding the half cannot round a value below the threshold across the integer boundary. Negative values follow symmetrically. Outside the i16 range the cast saturates.
3. **Multichannel resampler output:** remove scalar `round()` from the generic planar-to-interleaved loop. Keep multiplication in f32, widen that already-rounded product, then add a signed half in f64 and cast. Widening retains every f32 bit and prevents a below-threshold value from rounding up across an integer boundary. Very small inputs can round to a half after the addition, which still casts to zero. Infinities still saturate and NaNs still cast to zero. Preserve the existing mono/stereo kernels, shortest-channel limit, channel repetition, and buffer reuse. No floating-point multiply is reassociated or fused.

Production source loses **eight net lines**, excluding the test module declaration. No dependencies, public APIs, CPU feature requirements, or production `unsafe` are added. Existing unsafe benchmark support is reused unchanged. Both rounding changes replace calls inside existing loops; there are no new runtime helpers or dispatch layers.

## Measurement

Intel Xeon E5-2696 v4, Windows x86_64, rustc 1.99.1 / LLVM 23.1.1. All archives use `--release --locked --config profile.release.lto=false` and the repository's static MSVC runtime. Assets and the two audio integration targets are built separately, with identical target selections and benchmark sources before and after.

Before editing each retained kernel, its original production binary was built and benchmarked. The final original-source archives were rebuilt with the final test sources, including the controlled WAV address offset. Final figures compare these archives with the final implementations. All compilation launched by this task finished before the measurements; the host was shared with unrelated work.

Six fresh process runs alternate archive order. The existing harness pins the measuring thread to logical CPU 2, discards a warmup batch, and reports the median of eleven batches using Windows `QueryThreadCycleTime`. Tables show medians across processes and the ratio of before/after median cycles. Ranges are the minimum/maximum ratios paired by process run, not confidence intervals. Raw TSVs retain elapsed nanoseconds and all allocation counters. These are local buffer/kernel measurements, not whole-game FPS claims.

Color-key measurements include restoring the reusable image from a separate source before every operation. The 1024-square workload is consequently dominated by memory traffic. WAV and resampler measurements reuse reserved output buffers and include their normal clear/resize work. Fixtures and allocation counters are outside timing. WAV source/output addresses have a fixed relative page offset of 128 bytes, preventing false 4 KiB aliasing from dominating the PCM16 memcpy control. Inputs, dimensions, output buffers, and runtime encoding choices pass through `black_box`; color-key and resampler entry points also pass through function pointers.

All **564** scoped operation probes report zero allocations, reallocations, frees, requested bytes, and peak bytes in both versions. This work reduces CPU overhead while preserving already allocation-free buffer reuse.

The tables include every workload and control, including slower results. The 1024-square key-free texture loses 1.2% CPU throughput; the new loop always writes. The 64-sample PCM16 controls take 5.7-11.7 additional cycles (15.2-28.0% lower throughput). Mono resampler controls lose 4.4-8.5% throughput. Their algorithms are unchanged; code layout and shared-host variation still affect measurements. Large PCM16 controls vary in both directions across processes. These results do not establish whole-application performance, and no zero-cost assumption is made for unchanged paths.

### Color-key textures

| Workload | Before cycles/op | After cycles/op | CPU throughput | Paired process range |
| --- | ---: | ---: | ---: | ---: |
| `key/1024/dense` | 2380134.62 | 2134349.31 | 1.115x | 1.081-1.124x |
| `key/1024/none` | 2138204.44 | 2164873.62 | 0.988x | 0.978-1.043x |
| `key/1024/sparse` | 2233110.69 | 2149769.31 | 1.039x | 0.991-1.059x |
| `key/256/dense` | 122594.42 | 81324.74 | 1.507x | 1.397-1.598x |
| `key/256/none` | 84803.83 | 79784.60 | 1.063x | 1.004-1.179x |
| `key/256/sparse` | 92259.51 | 82568.58 | 1.117x | 1.007-1.359x |
| `key/32/dense` | 1348.30 | 710.82 | 1.897x | 1.689-2.455x |
| `key/32/none` | 962.46 | 687.11 | 1.401x | 1.277-1.564x |
| `key/32/sparse` | 1005.95 | 685.53 | 1.467x | 1.140-1.888x |

### WAV packets (8-byte samples are the changed Float64 path)

| Workload | Before cycles/op | After cycles/op | CPU throughput | Paired process range |
| --- | ---: | ---: | ---: | ---: |
| `wav/bytes=2/audio/samples=64` | 30.42 | 38.25 | 0.795x | 0.546-0.918x |
| `wav/bytes=2/audio/samples=8192` | 623.42 | 527.16 | 1.183x | 0.631-1.679x |
| `wav/bytes=2/clipped/samples=64` | 32.05 | 37.80 | 0.848x | 0.565-0.980x |
| `wav/bytes=2/clipped/samples=8192` | 541.55 | 823.12 | 0.658x | 0.561-0.997x |
| `wav/bytes=2/exceptional/samples=64` | 30.04 | 41.70 | 0.720x | 0.516-0.836x |
| `wav/bytes=2/exceptional/samples=8192` | 707.08 | 606.33 | 1.166x | 0.676-1.893x |
| `wav/bytes=4/audio/samples=64` | 271.13 | 281.66 | 0.963x | 0.889-0.999x |
| `wav/bytes=4/audio/samples=8192` | 28261.53 | 28965.01 | 0.976x | 0.920-1.059x |
| `wav/bytes=4/clipped/samples=64` | 279.23 | 291.24 | 0.959x | 0.877-1.031x |
| `wav/bytes=4/clipped/samples=8192` | 28654.65 | 29697.26 | 0.965x | 0.918-1.108x |
| `wav/bytes=4/exceptional/samples=64` | 199.09 | 199.55 | 0.998x | 0.920-1.020x |
| `wav/bytes=4/exceptional/samples=8192` | 22503.24 | 22719.15 | 0.990x | 0.897-1.018x |
| `wav/bytes=8/audio/samples=64` | 493.34 | 346.70 | 1.423x | 1.284-1.542x |
| `wav/bytes=8/audio/samples=8192` | 57601.65 | 37765.68 | 1.525x | 1.249-1.728x |
| `wav/bytes=8/clipped/samples=64` | 486.59 | 331.81 | 1.466x | 1.282-1.563x |
| `wav/bytes=8/clipped/samples=8192` | 58324.92 | 37659.54 | 1.549x | 1.323-1.694x |
| `wav/bytes=8/exceptional/samples=64` | 342.31 | 240.95 | 1.421x | 1.246-1.655x |
| `wav/bytes=8/exceptional/samples=8192` | 40868.38 | 29999.53 | 1.362x | 1.288-1.380x |

### Resampler output (mono/stereo layouts are controls)

| Workload | Before cycles/op | After cycles/op | CPU throughput | Paired process range |
| --- | ---: | ---: | ---: | ---: |
| `resampler/1to1/frames=4096` | 13380.06 | 13998.08 | 0.956x | 0.791-1.133x |
| `resampler/1to1/frames=64` | 232.09 | 253.63 | 0.915x | 0.843-1.041x |
| `resampler/1to2/frames=4096` | 16076.58 | 16565.06 | 0.971x | 0.822-1.212x |
| `resampler/1to2/frames=64` | 243.09 | 237.11 | 1.025x | 0.861-1.179x |
| `resampler/2to2/frames=4096` | 37534.50 | 36552.15 | 1.027x | 0.938-1.070x |
| `resampler/2to2/frames=64` | 551.18 | 519.71 | 1.061x | 0.898-1.150x |
| `resampler/2to6/frames=4096` | 110943.51 | 69510.17 | 1.596x | 1.471-1.785x |
| `resampler/2to6/frames=64` | 1750.48 | 1136.49 | 1.540x | 1.444-1.573x |
| `resampler/3to3/frames=4096` | 111027.70 | 59671.49 | 1.861x | 1.705-2.078x |
| `resampler/3to3/frames=64` | 1716.72 | 968.13 | 1.773x | 1.761-1.944x |
| `resampler/4to6/frames=4096` | 152264.42 | 90686.43 | 1.679x | 1.561-1.807x |
| `resampler/4to6/frames=64` | 2532.05 | 1500.97 | 1.687x | 1.320-1.980x |
| `resampler/6to6/frames=4096` | 200485.82 | 111763.93 | 1.794x | 1.569-1.975x |
| `resampler/6to6/frames=64` | 3228.02 | 1819.76 | 1.774x | 1.541-1.914x |
| `resampler/6to8/frames=4096` | 214417.19 | 128102.27 | 1.674x | 1.559-1.736x |
| `resampler/6to8/frames=64` | 3545.38 | 2028.43 | 1.748x | 1.642-1.953x |
| `resampler/8to3/frames=4096` | 106152.64 | 58381.58 | 1.818x | 1.685-2.001x |
| `resampler/8to3/frames=64` | 1777.81 | 936.92 | 1.898x | 1.595-2.034x |
| `resampler/8to8/frames=4096` | 272536.70 | 149181.09 | 1.827x | 1.618-1.889x |
| `resampler/8to8/frames=64` | 4148.98 | 2302.05 | 1.802x | 1.714-1.845x |

## Correctness and validation

All **152 release tests** pass (105 asset tests and 47 audio-decode tests). Nine focused debug tests also pass. Added tests also pass against the original archived implementations. Clippy completes for both crates and their tests; existing warnings remain outside the changed code. New Rust files pass `rustfmt --check`, and the task's files pass `git diff --check`.

Color-key tests cover edge selection, transparent near-matches, zero dimensions, partial SIMD-sized tails, and trailing allocation bytes, alongside the existing native pixel fixture comparisons. Float64 checks cover neighbors around every i16 half-rounding boundary, every exponent and both signs with representative mantissas, and two million random bit patterns. Resampler checks cover every in/out channel count from 0 through 9, empty requests, unequal lengths, channel repetition, NaNs/infinities, rounding-boundary neighbors, and one million arbitrary f32 bit patterns. Existing tests retain packet alignment, malformed-input errors, seeking, and decoding behavior.

Discarded trials: repeated small block copies in channel mapping improved wider copies but slowed common 2-to-6 and 2-to-8 layouts, so channel mapping is unchanged. A fraction-correction Float64 converter produced only modest gains and was replaced by the guarded bias implementation. Trial outputs remain under `target/direct-rounding`; committed measurements contain only the retained implementations.

## Reproduction

Create an original-source checkout at `d0347267e`; copy in the final `key_kernel.rs`, `wav_float_kernel.rs`, and `resampler_output_kernel.rs` test files, and append `#[cfg(test)] mod key_kernel;` to the original assets `decode.rs`. Keep all production kernels at their original versions. Build it and the final checkout with these identical selections:

```powershell
cargo test --release --locked --config profile.release.lto=false -p deadlib-assets --lib --no-run --message-format=json
cargo test --release --locked --config profile.release.lto=false -p deadlib-audio-decode --test wav_float_kernel --test resampler_output_kernel --no-run --message-format=json
```

Copy the Cargo `compiler-artifact.executable` paths into separate archive directories as `assets.exe`, `wav_float_kernel.exe`, and `resampler_output_kernel.exe`. Finish building both versions before measuring. Then run:

```powershell
python scripts/benchmark_direct_rounding.py --archive before=target/direct-rounding/baseline-final --archive after=target/direct-rounding/after --target key --runs 6 --output docs/performance/2026-10-25-key-rounding-key.tsv
python scripts/benchmark_direct_rounding.py --archive before=target/direct-rounding/baseline-final --archive after=target/direct-rounding/after --target wav --runs 6 --output docs/performance/2026-10-25-key-rounding-wav.tsv
python scripts/benchmark_direct_rounding.py --archive before=target/direct-rounding/baseline-final --archive after=target/direct-rounding/after --target resampler --runs 6 --output docs/performance/2026-10-25-key-rounding-resampler.tsv
cargo test --release --locked --config profile.release.lto=false -p deadlib-assets --lib --tests
cargo test --release --locked --config profile.release.lto=false -p deadlib-audio-decode --lib --tests
cargo test --locked -p deadlib-assets --lib key_preserves_edges_alpha_and_trailing_storage
cargo test --locked -p deadlib-audio-decode --test wav_float_kernel --test resampler_output_kernel
cargo clippy --release --locked --config profile.release.lto=false -p deadlib-assets -p deadlib-audio-decode --lib --tests
```

Local source snapshots, executables, build logs, test logs, trials, and individual process logs are under `target/direct-rounding`. Committed raw data: [color keys](2026-10-25-key-rounding-key.tsv), [WAV](2026-10-25-key-rounding-wav.tsv), [resampler](2026-10-25-key-rounding-resampler.tsv). [SHA-256 metadata](2026-10-25-key-rounding-hashes.json) identifies production source snapshots, benchmark sources, and all six executables.
