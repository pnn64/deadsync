# Packed resizing, float WAV conversion and SFX gains - 2026-10-25

Three safe Rust optimizations, following `rust-performance.md`'s hot-path, throughput and allocation guidance. Both Lua crates and concurrent Lua edits are excluded.

## Work removed and exact behavior

1. **General RGBA resizing:** pack two independent byte channels into the low bytes of two 32-bit lanes in a `u64`. Run each 24-bit fixed-point interpolation on both lanes together, then extract the bytes. Complementary weights sum to 2^24, so even 255-valued lanes cannot carry into their neighbors. Horizontal interpolation still floors before vertical interpolation adds its half-unit bias and shifts. The channel loop shrinks from four iterations to two, with twelve scalar products instead of twenty-four per output pixel. The existing half-size specialization, coordinate arithmetic, resize stages, output layout and allocation lifetimes remain intact.
2. **32-bit float WAV packets:** widen each sample to f64, multiply by 32767, add a signed half and use the saturating integer cast. The product of a 24-bit significand and a 15-bit integer fits exactly in f64. Around the i16 rounding boundaries, the signed-half addition is also exact; very small values still truncate to zero, and large values still saturate. This removes `round()` per sample. Nonfinite input remains silent. Float64 keeps the original conversion because its greater source precision invalidates that shortcut.
3. **SFX power-of-two gains:** recognize zero and normal powers of two once per voice buffer. Each normalized i16 source times such a gain is exact, including at the smallest normal gain: the source step is at least 2^-141, above f32's 2^-149 subnormal step. Ordinary multiply/add therefore preserves the original fused result. Reuse the existing panned loop and consolidate centered unity/zero loops into one exact-scaling loop. Other gains retain fused arithmetic. Signed zeros, NaN propagation, channel routing, odd tails, scheduling and voice progress are preserved.

The production implementations add **15 net lines**; the centered mixer shrinks by five lines. An additional three-line test-module declaration is excluded from that count. Execution loses channel iterations, intermediate products, per-sample rounding and fused work as described above. No dependency, public API, production `unsafe`, compiler flag or CPU feature requirement is added.

## Measurement

Intel Xeon E5-2696 v4, Windows x86_64, rustc 1.99.1 / LLVM 23.1.1. Release level 3, `--locked --config profile.release.lto=false`, with the repository's static MSVC CRT flags. Both versions use the same three-package build command and feature selection.

Production at `80c1c173face6d492bd49cc1167ee24c2046fa03` was built and measured before each corresponding change. After selecting the retained implementations, the baseline was rebuilt with exactly the final benchmark and correctness test sources; source hashes were checked before and after the build. The resize benchmark was rebuilt and remeasured after removing an unnecessary closure borrow flagged by Clippy. Archived before/after executables are compared independently of Cargo.

Four fresh processes per version alternate before/after order. Each benchmark thread is pinned to logical processor 2. The existing `kernel_bench.rs` discards one warmup batch and selects the median of eleven batches by Windows `QueryThreadCycleTime`. Tables show the median of the four process results. CPU throughput is before/after thread cycles for identical work; elapsed nanoseconds are secondary. The process ratio range is the minimum and maximum paired process ratio, not a confidence interval. No compilation or other benchmark launched by this task overlapped the final timing. The machine is shared; affinity and thread-cycle accounting cannot eliminate hardware contention.

- **WAV:** one complete production `decode_packet_into` call into reserved output storage. Byte fixtures, including clipped and exceptional values, are prepared outside timing. Float64 and PCM16 are controls.
- **Resize:** a complete production `zoom_image` call, including cloning the fixture, allocations, all resize stages and destruction. Coordinates and function pointers are opaque to the optimizer. Exactly 2:1 resizing is a control for the existing specialized path.
- **SFX:** the public `mix_active_sfx` call with one retained stereo voice. Cursor reset and destination clearing are included. Three pans, six gains and two callback sizes are measured; storage preparation stays outside timing.

These are local kernel and complete-buffer results, not whole-game FPS claims. Float32 audio packets improve 1.81-2.31x, exceptional Float32 packets 1.38x, and targeted power-of-two mixer paths 2.41-6.02x. The large general upscale improves 1.103x, with every paired process ratio above one (1.062-1.135x). Its source loop halves the scalar products, but the full-call gain includes cloning and allocation overhead. The unchanged half-size control has large process variation (0.568-3.301x), so its apparent median gain is not attributed to this optimization. Small-resize ratios also overlap neutral. Gain 0.375 controls lose 0.2-6.9% throughput; zero/unity controls range from 1.00-1.10x. Float64 controls are within roughly 5%, while some very fast PCM16 controls regress despite their conversion remaining unchanged (the largest loss is about 225 cycles per 8,192-sample packet). All controls and process ranges are recorded below.

## CPU cycles and throughput

| Workload | Before cycles/op | After cycles/op | CPU throughput | Process ratio range | Before ns/op | After ns/op |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `wav/bytes=4/audio/samples=64` | 501.37 | 277.80 | 1.805x | 1.615-1.920x | 228.75 | 126.62 |
| `wav/bytes=4/audio/samples=8192` | 58143.21 | 25204.72 | 2.307x | 2.173-2.407x | 26670.50 | 11498.36 |
| `wav/bytes=4/clipped/samples=64` | 474.78 | 242.02 | 1.962x | 1.882-2.058x | 216.44 | 111.50 |
| `wav/bytes=4/clipped/samples=8192` | 56940.46 | 24825.10 | 2.294x | 1.821-2.420x | 26041.81 | 11460.65 |
| `wav/bytes=4/exceptional/samples=64` | 239.75 | 174.07 | 1.377x | 1.308-1.386x | 111.53 | 79.38 |
| `wav/bytes=4/exceptional/samples=8192` | 27837.82 | 20229.99 | 1.376x | 1.291-1.505x | 12690.17 | 9223.77 |
| `wav/bytes=8/audio/samples=64` | 438.98 | 425.39 | 1.032x | 0.900-1.167x | 200.16 | 195.13 |
| `wav/bytes=8/audio/samples=8192` | 50764.78 | 49796.82 | 1.019x | 0.806-1.054x | 23276.64 | 22695.90 |
| `wav/bytes=8/clipped/samples=64` | 426.68 | 438.00 | 0.974x | 0.807-1.056x | 195.57 | 200.97 |
| `wav/bytes=8/clipped/samples=8192` | 49711.35 | 51456.56 | 0.966x | 0.910-1.119x | 22795.90 | 23621.31 |
| `wav/bytes=8/exceptional/samples=64` | 319.46 | 315.25 | 1.013x | 0.989-1.186x | 145.62 | 143.75 |
| `wav/bytes=8/exceptional/samples=8192` | 36456.80 | 38412.50 | 0.949x | 0.809-1.019x | 16763.93 | 17525.82 |
| `wav/bytes=2/audio/samples=64` | 23.29 | 24.84 | 0.938x | 0.900-0.952x | 10.57 | 11.27 |
| `wav/bytes=2/audio/samples=8192` | 601.83 | 826.72 | 0.728x | 0.510-1.983x | 268.44 | 372.13 |
| `wav/bytes=2/clipped/samples=64` | 25.68 | 24.96 | 1.029x | 0.900-1.359x | 11.80 | 11.40 |
| `wav/bytes=2/clipped/samples=8192` | 848.32 | 1054.33 | 0.805x | 0.517-1.928x | 382.38 | 475.82 |
| `wav/bytes=2/exceptional/samples=64` | 23.61 | 24.84 | 0.950x | 0.923-0.978x | 10.72 | 11.28 |
| `wav/bytes=2/exceptional/samples=8192` | 781.74 | 797.04 | 0.981x | 0.532-1.734x | 352.05 | 358.20 |
| `resize/16x16/32x32` | 24896.10 | 21898.57 | 1.137x | 0.936-1.209x | 11390.63 | 9954.69 |
| `resize/256x256/512x512` | 6673407.08 | 6051155.51 | 1.103x | 1.062-1.135x | 3046443.75 | 2762764.06 |
| `resize/256x256/128x128` | 252473.02 | 212397.14 | 1.189x | 0.568-3.301x | 115146.88 | 96925.00 |
| `resize/257x129/259x127` | 1097788.11 | 989392.81 | 1.110x | 1.019-1.820x | 500953.12 | 451540.63 |
| `resize/640x360/256x128` | 2258713.34 | 2117248.99 | 1.067x | 1.031-1.075x | 1031065.62 | 966821.88 |
| `resize/1024x512/259x127` | 3975985.29 | 3684352.16 | 1.079x | 0.940-1.195x | 1815873.44 | 1682025.00 |
| `pan/Left/gain=0/frames=256` | 356.69 | 351.52 | 1.015x | 0.989-1.068x | 162.43 | 160.15 |
| `pan/Left/gain=0/frames=4096` | 6196.60 | 5624.24 | 1.102x | 0.997-1.117x | 2824.08 | 2563.53 |
| `pan/Left/gain=1/frames=256` | 340.11 | 339.48 | 1.002x | 0.948-1.154x | 157.26 | 154.81 |
| `pan/Left/gain=1/frames=4096` | 6031.31 | 5732.64 | 1.052x | 0.963-1.094x | 2747.74 | 2612.19 |
| `pan/Left/gain=0.5/frames=256` | 957.23 | 328.37 | 2.915x | 2.598-3.426x | 436.45 | 151.85 |
| `pan/Left/gain=0.5/frames=4096` | 14631.16 | 5747.26 | 2.546x | 2.274-3.200x | 6674.69 | 2620.28 |
| `pan/Left/gain=0.125/frames=256` | 921.69 | 331.19 | 2.783x | 2.342-3.228x | 420.35 | 153.67 |
| `pan/Left/gain=0.125/frames=4096` | 14708.08 | 5587.35 | 2.632x | 2.498-3.046x | 6709.01 | 2547.34 |
| `pan/Left/gain=2/frames=256` | 890.90 | 337.19 | 2.642x | 2.559-2.812x | 408.47 | 153.86 |
| `pan/Left/gain=2/frames=4096` | 14272.44 | 5832.49 | 2.447x | 2.306-2.645x | 6538.11 | 2669.06 |
| `pan/Left/gain=0.375/frames=256` | 892.90 | 943.42 | 0.946x | 0.910-0.983x | 409.42 | 434.25 |
| `pan/Left/gain=0.375/frames=4096` | 14713.70 | 15354.65 | 0.958x | 0.924-0.991x | 6709.84 | 7005.94 |
| `pan/Right/gain=0/frames=256` | 343.92 | 328.24 | 1.048x | 0.886-1.099x | 156.78 | 149.63 |
| `pan/Right/gain=0/frames=4096` | 6004.54 | 5785.72 | 1.038x | 0.984-1.067x | 2737.91 | 2668.34 |
| `pan/Right/gain=1/frames=256` | 338.41 | 329.53 | 1.027x | 0.987-1.154x | 154.35 | 150.25 |
| `pan/Right/gain=1/frames=4096` | 6064.36 | 5822.83 | 1.041x | 0.982-1.095x | 2763.53 | 2654.09 |
| `pan/Right/gain=0.5/frames=256` | 891.40 | 329.01 | 2.709x | 2.498-2.828x | 408.34 | 150.06 |
| `pan/Right/gain=0.5/frames=4096` | 14145.15 | 5863.53 | 2.412x | 2.339-2.749x | 6515.89 | 2673.46 |
| `pan/Right/gain=0.125/frames=256` | 879.24 | 336.49 | 2.613x | 2.527-2.730x | 400.79 | 156.03 |
| `pan/Right/gain=0.125/frames=4096` | 14529.96 | 5930.32 | 2.450x | 2.078-2.806x | 6634.63 | 2704.31 |
| `pan/Right/gain=2/frames=256` | 894.73 | 333.24 | 2.685x | 2.467-2.828x | 408.01 | 151.82 |
| `pan/Right/gain=2/frames=4096` | 14214.42 | 5779.42 | 2.459x | 2.384-2.508x | 6480.43 | 2669.67 |
| `pan/Right/gain=0.375/frames=256` | 872.33 | 937.45 | 0.931x | 0.890-0.937x | 399.74 | 428.62 |
| `pan/Right/gain=0.375/frames=4096` | 14310.46 | 15024.06 | 0.953x | 0.927-1.043x | 6521.72 | 6848.87 |
| `pan/Center/gain=0/frames=256` | 297.73 | 281.72 | 1.057x | 0.998-1.127x | 135.67 | 128.50 |
| `pan/Center/gain=0/frames=4096` | 5011.17 | 4764.91 | 1.052x | 1.011-1.075x | 2283.09 | 2177.25 |
| `pan/Center/gain=1/frames=256` | 307.56 | 290.93 | 1.057x | 1.042-1.088x | 140.61 | 133.02 |
| `pan/Center/gain=1/frames=4096` | 4979.01 | 4732.30 | 1.052x | 0.992-1.075x | 2271.32 | 2156.15 |
| `pan/Center/gain=0.5/frames=256` | 1718.16 | 286.71 | 5.993x | 5.794-6.085x | 783.49 | 130.71 |
| `pan/Center/gain=0.5/frames=4096` | 27681.06 | 4845.87 | 5.712x | 5.562-5.877x | 12656.35 | 2210.14 |
| `pan/Center/gain=0.125/frames=256` | 1700.74 | 282.76 | 6.015x | 5.749-7.055x | 775.80 | 128.86 |
| `pan/Center/gain=0.125/frames=4096` | 27090.26 | 5140.71 | 5.270x | 4.310-5.662x | 12387.50 | 2376.74 |
| `pan/Center/gain=2/frames=256` | 1657.04 | 289.39 | 5.726x | 4.423-6.125x | 757.59 | 131.89 |
| `pan/Center/gain=2/frames=4096` | 27243.41 | 4879.60 | 5.583x | 4.413-5.921x | 12420.28 | 2228.59 |
| `pan/Center/gain=0.375/frames=256` | 1642.75 | 1685.64 | 0.975x | 0.827-1.036x | 748.83 | 769.26 |
| `pan/Center/gain=0.375/frames=4096` | 27032.01 | 27080.81 | 0.998x | 0.986-1.024x | 12325.61 | 12351.33 |

## Allocations

All **480 operation probes** are separate from timed batches. All 432 WAV/mixer probes report zero allocations, reallocations, frees, allocated/freed bytes and peak bytes. The 48 resize probes have identical counters across all versions and processes; they include the input clone and consumed images. No allocation reduction is claimed.

| Resize workload | Allocations before/after | Reallocations before/after | Frees before/after | Requested bytes before/after | Peak bytes before/after |
| --- | ---: | ---: | ---: | ---: | ---: |
| `resize/16x16/32x32` | 3/3 | 0/0 | 3/3 | 5504/5504 | 5504/5504 |
| `resize/256x256/512x512` | 3/3 | 0/0 | 3/3 | 1316864/1316864 | 1316864/1316864 |
| `resize/256x256/128x128` | 2/2 | 0/0 | 2/2 | 327680/327680 | 327680/327680 |
| `resize/257x129/259x127` | 3/3 | 0/0 | 3/3 | 267292/267292 | 267292/267292 |
| `resize/640x360/256x128` | 4/4 | 0/0 | 4/4 | 1286144/1286144 | 1152000/1152000 |
| `resize/1024x512/259x127` | 6/6 | 0/0 | 6/6 | 2891836/2891836 | 2621440/2621440 |

## Correctness and rejected trials

The release suite passes **227 tests across 15 executables**, with 28 opt-in benchmarks ignored by the normal run. The three selected benchmarks are then run explicitly. Focused debug checks pass **17 tests**, including the packed-resize reference with overflow checks enabled. Clippy completes with existing repository warnings; the new benchmark's unnecessary closure borrow was fixed.

WAV differential checks exercise every i16 half-rounding boundary and adjacent f32 values, both signs and every exponent, NaNs/infinities, extreme finite values and one million arbitrary bit patterns. Full packet output is compared against the original f64/round expression, with zero allocator churn. Existing WAV packet alignment, truncation, cursor and seek tests pass.

Mixer checks cover every i16 sample, every normal power-of-two gain, three pans, exceptional destination floats, NaN payloads, signed zeros, odd tails and voice progress. Existing direct panned-mixer checks also cover nonfinite gains and unequal lengths. Resize checks compare complete images against the existing scalar reference over small and degenerate axes, multistage resizing, every byte value in independent channels and uneven weights. Existing exact half-size, oversized-coordinate failure, decoder and texture tests pass.

Discarded trials: branch-free table removal, word-at-a-time step classification and broader newline word scanning did not consistently improve chart scanning; a general f64 WAV rounding correction did not consistently improve Float64 conversion; widening software RGBA packing did not improve throughput and slowed blending controls. None of those production edits or their temporary benchmark files is retained. Their local trial binaries/logs remain under `target/direct-scan`.

## Reproduction and archives

```powershell
cargo test --release --locked --config profile.release.lto=false -p deadlib-audio-core -p deadlib-audio-decode -p deadlib-assets --lib --tests
cargo clippy --release --locked --config profile.release.lto=false -p deadlib-audio-core -p deadlib-audio-decode -p deadlib-assets --lib --tests
cargo test --locked -p deadlib-audio-core --lib mixer::tests
cargo test --locked -p deadlib-audio-decode --test wav_float_kernel float32_packets
cargo test --locked -p deadlib-assets --lib streamed_zoom_matches_original_rounding_and_multistep_resizes
python scripts/benchmark_slice_kernels.py --before target/direct-scan/before/wav_float_kernel.exe --after target/direct-scan/after/wav_float_kernel.exe --filter benchmark_wav_float --output docs/performance/2026-10-25-packed-gain-wav.tsv --logs target/direct-scan/final-runs --runs 4
python scripts/benchmark_slice_kernels.py --before target/direct-scan/before/resize_kernel.exe --after target/direct-scan/after/resize_kernel.exe --filter benchmark_resize_kernel --output docs/performance/2026-10-25-packed-gain-resize.tsv --logs target/direct-scan/final-runs --runs 4
python scripts/benchmark_slice_kernels.py --before target/direct-scan/before/panned_kernel.exe --after target/direct-scan/after/panned_kernel.exe --filter benchmark_panned_mix --output docs/performance/2026-10-25-packed-gain-mix.tsv --logs target/direct-scan/final-runs --runs 4
```

To reconstruct archives in separate checkouts, start the baseline at `80c1c173face6d492bd49cc1167ee24c2046fa03`. Copy the retained `wav_float_kernel.rs`, `panned_kernel.rs`, `resize_kernel.rs` and `preparation_perf.rs` test sources into it, and append `#[cfg(test)] mod resize_kernel;` to its `decode.rs`. In each checkout build all three packages together with the release command above plus `--no-run --message-format=json`. Copy the `compiler-artifact` executable paths for `deadlib_assets`, `panned_kernel` and `wav_float_kernel` to the archive directories; name the assets executable `resize_kernel.exe`. Do not benchmark during compilation.

The committed TSV files contain every final process measurement, allocation counter and executable hash. Local source snapshots, build/suite/lint logs and full process logs are under `target/direct-scan`.

| Executable | SHA-256 before | SHA-256 after |
| --- | --- | --- |
| `wav_float_kernel` | `c7352d27446fe1d193c24bd28943a3ad1e4094e0afeaa7f05146efe7d86d936d` | `e0f98c577d4f597084198f2a5f7f21c436852fa1a780b3b3452f1983a4155fe1` |
| `resize_kernel` | `bfb0c75c1f2b0c6450aa654bff0fe0ec3230bf0781c79f2f0d689aba99f86548` | `7a46024bf7fe1889210db817b6b7a06c29818241691c5d5e31e43cf88b82bfdc` |
| `panned_kernel` | `d712adbc7bb039558f4a7de1570f239575f73f1fdbcebe63938ce8a283156091` | `5ed7cdb45457846d7be68bc57fffb6b33a112b29fd229f8912aa7e8353832576` |

Identical benchmark/correctness source hashes used for both builds:

| Source | SHA-256 |
| --- | --- |
| `crates/deadlib-assets/src/decode/resize_kernel.rs` | `7b2c17edd8916c017fc6dd2afd3cd9f779b7dacc57e4c8d948cb0b68fbf789e8` |
| `crates/deadlib-assets/src/decode/preparation_perf.rs` | `760f5f8914edbffa49df0c957729c7b3f326960e85d7788bd1425393c45f7179` |
| `crates/deadlib-audio-core/tests/panned_kernel.rs` | `014da3aeaa2781f1af63b986a1514f61e21f853ed67e0b7a433737fc4c285684` |
| `crates/deadlib-audio-decode/tests/wav_float_kernel.rs` | `6e73e76dfda0816458cb3a943b02a5ca433495810dc752660c14b65f2cd402be` |
