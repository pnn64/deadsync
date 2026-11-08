# Alpha masks, video normalization, and panned SFX — 2026-10-25

Three safe Rust changes remove work from software rendering and stereo sound mixing. `deadlib-lua` and `deadlib-lua-legacy`, including unrelated working-tree edits, are excluded.

## Changes and work removed

1. **Alpha sampling:** specialize the existing private samplers and texel reader for either one alpha channel or four RGBA channels. Mask draws now decode and interpolate alpha directly. A bilinear half-float mask sample avoids twelve RGB half conversions and nine RGB fused interpolations, and its intermediate color arrays shrink from four channels to one. Ordinary color draws still instantiate four channels. Coordinate wrapping, bounds checks, transparency rejection, missing-texel fallback, and operation order remain the same. This reuses the existing sampler bodies rather than adding a second sampling pipeline.
2. **YUV420 conversion:** replace repeated `u8 -> f32 -> divide by 255` normalization with a 256-entry, 1 KiB read-only table. Constant evaluation performs the original division, preserving its exact rounding; reciprocal multiplication would not. A 1080p frame avoids 3,110,400 floating-point divisions. Level adjustments, fused color conversion, rounding, saturation, and output writes remain in their original order. No table construction or allocation occurs at runtime.
3. **Panned SFX:** at unity bus gain, add each finite normalized right-channel source sample directly to the selected output channel. Multiplication by one is exact, so this removes the fused multiplication step without changing sample bits. The gain check occurs once per voice buffer. Fixed stereo arrays expose the fast loop's frame width to the compiler. Other gains retain the original fused calculation, and odd trailing samples remain untouched.

Production code grows by **23 net lines**, including the eleven-line constant table and seven-line unity-gain path. Execution loses the RGB intermediate values, redundant interpolations, conversions, and arithmetic described above. No dependency, production `unsafe`, runtime dispatch layer, or public API is added. Benchmark infrastructure reuses the existing allocation counter and Windows cycle/affinity helpers.

Generated release assembly for `write_yuv420` contains indexed normalization loads and **zero floating-point division instructions**. The local excerpt is `target/texture-pipeline/yuv-final.s`.

## Measurement method

Intel Xeon E5-2696 v4, Windows x86_64, rustc 1.99.1 / LLVM 23.1.1. Both versions use release optimization level 3, `--config profile.release.lto=false`, and the repository's static MSVC CRT settings. No new CPU feature requirement is introduced.

Unchanged production sources at `551c81c23` were compiled and measured before each change. The before executables were archived before editing the corresponding implementation. The after executables compile the optimized production sources with the same options. Private kernels are included from their actual source files, following the existing software-backend test convention. The final correctness suite adds the exhaustive binary16 check; timed fixture construction and benchmark bodies are unchanged apart from the four-channel reference's private generic argument.

Four fresh process runs per version alternate before/after order. Each worker is pinned to logical processor 2. Each process discards one warmup batch and reports the median of eleven measured batches. The table reports the median of four process measurements. CPU throughput is the ratio of before/after **thread CPU cycles**, obtained with Windows `QueryThreadCycleTime`; elapsed nanoseconds are secondary. Ratio ranges are the minimum and maximum of corresponding process ratios, not confidence intervals.

No compilation or other benchmark launched by this task overlapped the reported runs. The machine was shared with unrelated work; thread affinity and thread cycle accounting reduce scheduling noise but do not eliminate hardware contention. Preliminary runs with an unintended extra benchmark process were discarded.

- `alpha/.../1024` performs 1,024 alpha queries on a warmed 128x128 texture with varied coordinates. Both byte and binary16 textures, nearest and bilinear filtering, and clamp and repeat wrapping are measured. Texture data, coordinates, sampler, and results are opaque to the optimizer.
- `yuv/WIDTHxHEIGHT` converts one complete frame into an existing RGBA image using limited-range levels and BT.601 coefficients. The fixture varies every input plane. Plane creation and image allocation stay outside timing.
- `panned/frames=N/gain=G` mixes one panned stereo buffer. The integrated `active=16` case calls the real `mix_active_sfx` path with sixteen alternating left/right voices. Cursor reset is included; voice and output storage are prepared before timing. Output samples accumulate across calls.

These are local kernel and mixer results, not whole-game FPS claims. Bilinear mask throughput improves 1.57–2.61x, half-float nearest masks improve 1.43–1.70x, YUV conversion improves 1.13–1.20x for the three larger fixtures, and unity-gain panning improves about 3x. Byte nearest-repeat sampling regresses 10.2% in throughput, about 1.9 ns/query; nearest-clamp is essentially unchanged. Non-unity mixer controls are within about 3% in the final run set. The complete controls and process ranges follow.

## CPU cycles and throughput

| Workload | Before cycles/op | After cycles/op | CPU throughput | Process ratio range | Before ns/op | After ns/op |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `alpha/half=false/Clamp/Linear/1024` | 103667.45 | 60617.15 | 1.710x | 1.444-1.912x | 47430.07 | 27774.42 |
| `alpha/half=false/Clamp/Nearest/1024` | 30680.71 | 30367.31 | 1.010x | 0.822-1.172x | 14055.86 | 13887.69 |
| `alpha/half=false/Repeat/Linear/1024` | 109787.73 | 69823.29 | 1.572x | 1.428-1.686x | 50353.12 | 31981.25 |
| `alpha/half=false/Repeat/Nearest/1024` | 38370.06 | 42749.34 | 0.898x | 0.809-0.954x | 17547.46 | 19569.72 |
| `alpha/half=true/Clamp/Linear/1024` | 192978.66 | 73831.74 | 2.614x | 2.464-2.875x | 88389.65 | 33778.51 |
| `alpha/half=true/Clamp/Nearest/1024` | 58757.40 | 34511.66 | 1.703x | 1.655-1.816x | 26934.18 | 15794.73 |
| `alpha/half=true/Repeat/Linear/1024` | 188592.09 | 81362.47 | 2.318x | 2.072-2.619x | 86475.39 | 37198.05 |
| `alpha/half=true/Repeat/Nearest/1024` | 65212.08 | 45620.85 | 1.429x | 1.177-1.724x | 29817.58 | 20863.09 |
| `panned/active=16/frames=2048/gain=1` | 116189.23 | 39323.94 | 2.955x | 2.499-3.535x | 53262.90 | 17980.47 |
| `panned/frames=2048/gain=0` | 7571.33 | 7485.83 | 1.011x | 0.909-1.131x | 3465.73 | 3428.30 |
| `panned/frames=2048/gain=0.375` | 7238.39 | 7314.72 | 0.990x | 0.885-1.071x | 3314.93 | 3354.09 |
| `panned/frames=2048/gain=1` | 7468.73 | 2452.28 | 3.046x | 2.884-3.693x | 3418.68 | 1124.07 |
| `panned/frames=64/gain=0` | 259.45 | 255.11 | 1.017x | 0.950-1.140x | 119.52 | 116.50 |
| `panned/frames=64/gain=0.375` | 246.02 | 239.51 | 1.027x | 0.951-1.212x | 111.91 | 109.66 |
| `panned/frames=64/gain=1` | 261.86 | 83.36 | 3.141x | 2.751-3.595x | 119.07 | 37.84 |
| `yuv/1920x1080` | 87642619.25 | 72903139.00 | 1.202x | 1.135-1.286x | 40168700.00 | 33421350.00 |
| `yuv/2x2` | 239.79 | 202.53 | 1.184x | 1.107-1.260x | 109.86 | 92.70 |
| `yuv/640x360` | 9971528.12 | 8857511.00 | 1.126x | 1.015-1.310x | 4569250.00 | 4059000.00 |
| `yuv/64x64` | 168019.60 | 147978.99 | 1.135x | 0.995-1.223x | 77042.42 | 67771.51 |

## Allocations and correctness

All **152 operation probes** (19 workloads × 4 processes × 2 versions) report zero allocations, reallocations, frees, requested bytes, and peak bytes. Allocation probes run separately from timed batches. The retained changes preserve allocation-free execution rather than claiming an allocation reduction.

The six-crate release suite passed **1,171 tests** across eighteen executables, with 25 benchmark tests ignored in the normal run. The three selected benchmarks were then run explicitly. The final stereo-array spelling was additionally checked with the 16 passing panned-mixer tests. Clippy completed successfully for both changed crates with existing repository warnings; the new fast path introduces no warning.

Differential checks compare alpha sampling bit-for-bit with the RGBA path for every binary16 encoding, exceptional coordinates, NaN payloads, signed zeros, clamp/repeat boundaries, transparent texels, and missing storage. YUV output is compared byte-for-byte with the original ordered fused arithmetic for all byte values in the planes, BT.601/BT.709 coefficients, full/limited levels, zeros, infinities, and NaNs. Panned mixing checks every i16 source sample against the original fused loop, including exceptional destination floats, NaN payloads, signed zeros, varied gains and pans, unequal lengths, and odd tails. Existing raster, depth, offscreen, scheduling, bus-generation, and gameplay tests also pass.

## Reproduction

```powershell
cargo test --release --locked --config profile.release.lto=false -p deadlib-audio-core -p deadlib-render-core -p deadlib-render-backend-software -p deadsync-core -p deadsync-rules -p deadsync-gameplay --lib --tests
cargo clippy --release --locked --config profile.release.lto=false -p deadlib-audio-core -p deadlib-render-backend-software --lib --tests
cargo test --release --locked --config profile.release.lto=false -p deadlib-render-backend-software --test texture_pipeline -- 'pipeline::benchmark_' --ignored --nocapture --test-threads=1
cargo test --release --locked --config profile.release.lto=false -p deadlib-audio-core --test panned_mixer -- 'kernel::benchmark_' --ignored --nocapture --test-threads=1
python scripts/benchmark_texture_audio_kernels.py --before target/texture-pipeline/before --after target/texture-pipeline/after --output docs/performance/2026-10-25-texture-audio-kernels.tsv
```

To reconstruct before/after archives, use `551c81c23` in a separate worktree, copy the two new integration test files into it, and replace `::<false, 4>` with `::<false>` in its copied `texture_pipeline.rs` to match the original private sampler signatures. Build each tree with the same release options and package selection, and save its `texture_pipeline.exe` and `panned_mixer.exe` from Cargo's `compiler-artifact` JSON paths. Do not run the comparison during compilation.

Local archived executables, source snapshots, initial measurements, full process logs, suite/lint logs, and assembly are under `target/texture-pipeline`. The committed [TSV](2026-10-25-texture-audio-kernels.tsv) contains every final process measurement and allocation counter.

| Executable | SHA-256 before | SHA-256 after |
| --- | --- | --- |
| `texture_pipeline` | `aeb24fc0d4066f38f962c3f42cb1570e96490f37fb84a9d88fd32f2287acc170` | `cb1a0165e20d5da4d7819a415b37aec0f1ff8c771f2bfd2585620fadafaaae20` |
| `panned_mixer` | `45c92c6be9bf4a0ab02be812cc4d7844f4192807d9f78a5448cd115730b4f616` | `d8f75eebc18898e8d19dd540a4f76cad576f2e08aa3571e1254355de9b67615f` |
