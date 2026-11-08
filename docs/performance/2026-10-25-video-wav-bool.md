# Direct video quantization, stack WAV headers and packed Boolean validation

2026-10-25. Production baseline: `fc59baf80`. Both Lua crates, concurrent unrelated edits and rejected trials are excluded.

## Three retained optimizations

1. **YUV420 quantization:** remove three per-pixel `round()` calls. Widen each original f32 product exactly to f64, add 0.5, then cast to u8. This cannot round a below-threshold f32 across an integer boundary in the byte range. Negative products still cast to zero, positive overflow saturates to 255, and NaNs still produce zero. Every original multiply and ordered fused operation is retained. 1080p CPU throughput improves **1.635x**, with consistent improvements across all seven frame sizes.
2. **WAV format storage:** read standard PCM/float and extensible headers into a 40-byte stack array. Keep the original complete vector-backed read for chunks larger than 40 bytes. Remove **one allocation and free**, requesting 16 or 40 fewer heap bytes for the standard fixtures. Validation still follows the complete read, preserving truncation errors and reader position. Buffered parser throughput improves **2.681x** for 16-byte headers and **2.251x** for 40-byte headers. File-refill measurements do not establish a consistent CPU gain.
3. **Boolean validation:** flatten the outer size check and OR-reduce four safe u128 loads per 64-byte block. Mask bits 1 through 7 of every byte together. A failing block still enters the unchanged ordered u64/byte scan to identify the first invalid byte. Remove half the bulk-loop iterations and the original horizontal 64-bit reduction. Standalone optimized assembly emits SSE2 `movdqu`, `por`, `pand`, `pcmpeqd` and `movmskps`, without new CPU requirements. Valid 4096-element vector decoding improves **1.369x** in pooled medians. No consistent gain is established at 64 KiB.

Only three existing production functions change, plus one test-module declaration. No production functions, types, dependencies, APIs, caches or unsafe blocks are added. Bincode uses `chunks_exact` to preserve its declared Rust 1.86 MSRV. The aggregate mask works independently of endianness; the ordered little-endian error scan is unchanged.

## Measurement and limits

Intel Xeon E5-2696 v4, 22 cores / 44 logical processors; Windows x86_64 MSVC; rustc 1.99.1 (58cf8f9c3), LLVM 23.1.1. Archives use `--release --locked --config profile.release.lto=false`, opt-level 3, static MSVC runtime and no native CPU flags. Each target is built separately with identical target selections and final benchmark sources before/after. Original production functions come from `fc59baf80`.

Six process pairs alternate archive order. The existing harness pins its measuring thread to logical CPU 2, discards a warmup batch and reports the median of eleven batches using Windows `QueryThreadCycleTime`. Tables show medians across processes and their ratio. Ranges are minimum/maximum ratios paired by process run, not confidence intervals. The [complete TSV](2026-10-25-video-wav-bool.tsv) retains elapsed nanoseconds and every allocation counter. No compilation or other benchmark launched by this task overlapped comparisons. The shared host had unrelated compilation/work; affinity and thread cycle accounting do not eliminate hardware contention.

Video converts a complete frame into a reused RGBA allocation; fixture preparation is excluded. Buffered WAV measurements parse and rewind inside an already populated BufReader without a file syscall. Refill cases seek and re-read a cached file. Reader/file creation is excluded. Oversized controls retain the vector path. Boolean measurements use the real reusable-vector decoder, including wire-length decoding, validation, output copying and normal clear/resize work. First/middle/last-error cases retain decoded-prefix behavior. Inputs, outputs and runtime arguments pass through `black_box`; the WAV parser also uses an opaque function pointer. Separate operation probes use the existing scoped System allocator wrapper.

All slower controls remain visible. The valid 32-byte Boolean workload loses **24.7% throughput**, about **8.20 cycles/op**, because it now uses the ordered tail scan. The valid 7-byte control loses 8.3%, about 3.28 cycles. A first-byte error in a 4096-byte vector loses 14.7%, about 6.75 cycles. The 64 KiB last-error workload loses 10.7% in pooled throughput, with paired ratios spanning both sides of 1.0. Tiny/control paths vary substantially across processes. The Boolean change is retained for its larger cache-resident valid-buffer gain, with these tradeoffs explicit. These are kernel/parser/codec results, not whole-game FPS or loading-time claims.

## Complete CPU results

### Video frames

| Workload | Before cycles/op | After cycles/op | CPU throughput | Paired process range |
| --- | ---: | ---: | ---: | ---: |
| `yuv/16x16` | 10121.31 | 6559.62 | 1.543x | 1.323-1.822x |
| `yuv/1920x1080` | 69380053.25 | 42436812.75 | 1.635x | 1.428-1.969x |
| `yuv/2x2` | 214.86 | 160.08 | 1.342x | 1.254-1.588x |
| `yuv/30x34` | 42167.29 | 26560.62 | 1.588x | 1.380-1.984x |
| `yuv/32x32` | 37679.22 | 23512.81 | 1.602x | 1.326-1.908x |
| `yuv/640x360` | 7533596.75 | 4782493.38 | 1.575x | 1.445-1.950x |
| `yuv/64x64` | 142200.92 | 87635.38 | 1.623x | 1.316-2.006x |

### WAV format parsing

| Workload | Before cycles/op | After cycles/op | CPU throughput | Paired process range |
| --- | ---: | ---: | ---: | ---: |
| `format/buffered=false/extensible=false/bytes=16` | 9061.12 | 8892.39 | 1.019x | 0.977-1.173x |
| `format/buffered=false/extensible=false/bytes=4096` | 10805.11 | 10489.41 | 1.030x | 0.860-1.253x |
| `format/buffered=false/extensible=true/bytes=16385` | 12487.58 | 12724.19 | 0.981x | 0.943-1.137x |
| `format/buffered=false/extensible=true/bytes=40` | 9185.97 | 8847.77 | 1.038x | 0.958-1.109x |
| `format/buffered=true/extensible=false/bytes=16` | 264.04 | 98.50 | 2.681x | 2.564-3.236x |
| `format/buffered=true/extensible=false/bytes=4096` | 568.73 | 569.48 | 0.999x | 0.933-1.196x |
| `format/buffered=true/extensible=true/bytes=16385` | 2105.64 | 2106.63 | 1.000x | 0.945-1.209x |
| `format/buffered=true/extensible=true/bytes=40` | 307.28 | 136.51 | 2.251x | 2.143-2.536x |

### Boolean decoding

| Workload | Before cycles/op | After cycles/op | CPU throughput | Paired process range |
| --- | ---: | ---: | ---: | ---: |
| `bool/len=0/error=none` | 30.59 | 32.05 | 0.954x | 0.904-1.190x |
| `bool/len=1/error=first` | 42.14 | 41.56 | 1.014x | 0.896-1.042x |
| `bool/len=1/error=last` | 39.79 | 41.34 | 0.963x | 0.938-1.060x |
| `bool/len=1/error=middle` | 39.72 | 42.42 | 0.936x | 0.861-0.992x |
| `bool/len=1/error=none` | 29.77 | 31.49 | 0.945x | 0.893-1.076x |
| `bool/len=7/error=first` | 38.48 | 42.05 | 0.915x | 0.902-0.981x |
| `bool/len=7/error=last` | 51.02 | 51.38 | 0.993x | 0.917-1.114x |
| `bool/len=7/error=middle` | 41.43 | 45.89 | 0.903x | 0.634-0.993x |
| `bool/len=7/error=none` | 36.23 | 39.51 | 0.917x | 0.859-0.976x |
| `bool/len=8/error=first` | 36.58 | 38.00 | 0.963x | 0.874-1.070x |
| `bool/len=8/error=last` | 38.17 | 39.52 | 0.966x | 0.942-1.027x |
| `bool/len=8/error=middle` | 37.02 | 38.62 | 0.958x | 0.899-1.026x |
| `bool/len=8/error=none` | 25.17 | 27.93 | 0.901x | 0.819-1.097x |
| `bool/len=9/error=first` | 37.02 | 36.90 | 1.003x | 0.940-1.092x |
| `bool/len=9/error=last` | 39.09 | 39.36 | 0.993x | 0.970-1.119x |
| `bool/len=9/error=middle` | 38.07 | 37.45 | 1.017x | 0.910-1.131x |
| `bool/len=9/error=none` | 29.41 | 28.85 | 1.019x | 0.896-1.085x |
| `bool/len=16/error=first` | 37.98 | 35.27 | 1.077x | 0.835-1.169x |
| `bool/len=16/error=last` | 41.23 | 40.11 | 1.028x | 0.880-1.117x |
| `bool/len=16/error=middle` | 39.45 | 36.87 | 1.070x | 0.922-1.110x |
| `bool/len=16/error=none` | 30.89 | 28.20 | 1.096x | 0.855-1.191x |
| `bool/len=31/error=first` | 34.45 | 34.27 | 1.005x | 0.880-1.205x |
| `bool/len=31/error=last` | 47.76 | 48.44 | 0.986x | 0.849-1.150x |
| `bool/len=31/error=middle` | 41.98 | 37.81 | 1.110x | 0.909-1.217x |
| `bool/len=31/error=none` | 38.94 | 38.95 | 1.000x | 0.737-1.195x |
| `bool/len=32/error=first` | 39.94 | 38.53 | 1.036x | 0.834-1.105x |
| `bool/len=32/error=last` | 46.74 | 39.70 | 1.177x | 0.793-1.335x |
| `bool/len=32/error=middle` | 40.26 | 38.30 | 1.051x | 0.863-1.174x |
| `bool/len=32/error=none` | 24.96 | 33.16 | 0.753x | 0.631-0.940x |
| `bool/len=33/error=first` | 42.33 | 37.19 | 1.138x | 0.999-1.255x |
| `bool/len=33/error=last` | 37.70 | 42.22 | 0.893x | 0.708-1.045x |
| `bool/len=33/error=middle` | 43.20 | 40.72 | 1.061x | 0.872-1.282x |
| `bool/len=33/error=none` | 34.41 | 36.19 | 0.951x | 0.688-1.099x |
| `bool/len=63/error=first` | 39.67 | 38.77 | 1.023x | 0.843-1.695x |
| `bool/len=63/error=last` | 57.38 | 57.30 | 1.001x | 0.854-1.147x |
| `bool/len=63/error=middle` | 44.78 | 43.32 | 1.034x | 0.861-1.311x |
| `bool/len=63/error=none` | 48.92 | 45.95 | 1.065x | 0.622-1.140x |
| `bool/len=64/error=first` | 41.98 | 38.55 | 1.089x | 0.822-1.607x |
| `bool/len=64/error=last` | 52.39 | 63.45 | 0.826x | 0.799-0.906x |
| `bool/len=64/error=middle` | 39.41 | 42.61 | 0.925x | 0.693-2.852x |
| `bool/len=64/error=none` | 34.50 | 31.55 | 1.093x | 0.839-1.420x |
| `bool/len=65/error=first` | 36.67 | 43.02 | 0.852x | 0.709-1.110x |
| `bool/len=65/error=last` | 44.94 | 46.75 | 0.961x | 0.799-1.527x |
| `bool/len=65/error=middle` | 41.09 | 43.64 | 0.942x | 0.500-1.266x |
| `bool/len=65/error=none` | 38.04 | 30.98 | 1.228x | 0.919-2.218x |
| `bool/len=127/error=first` | 40.00 | 35.86 | 1.115x | 0.868-1.260x |
| `bool/len=127/error=last` | 65.96 | 71.98 | 0.916x | 0.734-1.076x |
| `bool/len=127/error=middle` | 49.50 | 56.91 | 0.870x | 0.727-1.059x |
| `bool/len=127/error=none` | 56.86 | 56.73 | 1.002x | 0.734-1.313x |
| `bool/len=128/error=first` | 41.16 | 38.03 | 1.082x | 0.751-1.307x |
| `bool/len=128/error=last` | 58.31 | 64.25 | 0.908x | 0.755-1.129x |
| `bool/len=128/error=middle` | 51.98 | 45.17 | 1.151x | 0.934-1.264x |
| `bool/len=128/error=none` | 42.33 | 34.84 | 1.215x | 0.923-1.583x |
| `bool/len=129/error=first` | 38.58 | 38.52 | 1.001x | 0.887-1.353x |
| `bool/len=129/error=last` | 59.27 | 49.29 | 1.202x | 0.997-2.973x |
| `bool/len=129/error=middle` | 46.20 | 53.12 | 0.870x | 0.500-1.025x |
| `bool/len=129/error=none` | 41.16 | 43.48 | 0.947x | 0.705-1.349x |
| `bool/len=256/error=first` | 40.17 | 38.05 | 1.056x | 0.851-1.171x |
| `bool/len=256/error=last` | 63.75 | 69.03 | 0.924x | 0.696-1.117x |
| `bool/len=256/error=middle` | 50.30 | 57.75 | 0.871x | 0.732-1.079x |
| `bool/len=256/error=none` | 50.53 | 47.62 | 1.061x | 0.793-1.652x |
| `bool/len=4096/error=first` | 39.13 | 45.88 | 0.853x | 0.730-1.000x |
| `bool/len=4096/error=last` | 457.66 | 470.03 | 0.974x | 0.871-1.260x |
| `bool/len=4096/error=middle` | 309.68 | 228.94 | 1.353x | 0.949-1.685x |
| `bool/len=4096/error=none` | 541.10 | 395.37 | 1.369x | 0.954-1.653x |
| `bool/len=65536/error=first` | 52.68 | 50.48 | 1.044x | 0.889-1.285x |
| `bool/len=65536/error=last` | 12453.33 | 13938.25 | 0.893x | 0.809-1.198x |
| `bool/len=65536/error=middle` | 6429.16 | 6501.59 | 0.989x | 0.856-1.211x |
| `bool/len=65536/error=none` | 12482.96 | 12209.69 | 1.022x | 0.984-1.335x |

## Allocations

All **1008** operation probes are in the TSV. Every video and Boolean probe reports zero allocations, reallocations, frees, requested bytes and peak bytes in both versions. No allocation reduction is claimed for those changes.

WAV probes are identical across all six processes, for both buffered and refill cases. There are no reallocations. Counts include temporary-buffer destruction.

| WAV chunk | Allocations before/after | Frees before/after | Requested and peak heap bytes before/after |
| --- | ---: | ---: | ---: |
| PCM, 16 bytes | 1 / 0 | 1 / 0 | 16 / 0 |
| Extensible, 40 bytes | 1 / 0 | 1 / 0 | 40 / 0 |
| PCM, 4096 bytes | 1 / 1 | 1 / 1 | 4096 / 4096 |
| Extensible, 16385 bytes | 1 / 1 | 1 / 1 | 16385 / 16385 |

The small-header path uses 40 bytes of stack storage; the large-header vector fallback remains.

## Correctness and rejected trials

All **267 release tests pass** across 20 executables; 37 benchmark tests are ignored normally. Selected benchmarks were run separately. Clippy completes for all three changed crates and tests with existing warnings.

- YUV comparisons retain original ordered fused arithmetic, full/limited levels, BT.601/BT.709 coefficients, table-threshold sizes and nonfinite behavior. New tests cover neighbors of every byte half-rounding boundary, every exponent and both signs with representative mantissas, and one million arbitrary combinations of y/u/v/coefficient bit patterns.
- WAV differential checks use a frozen starting parser. They compare fields, exact error strings, stream positions and remaining bytes across standard/extensible sizes, truncations, tags, channel/rate/alignment/width fields, valid-bit counts and GUIDs. Existing complete-file decoding, packet, malformed-input and seek tests also pass.
- Boolean tests cover every invalid value 2 through 255 at every position of a 193-byte buffer, including a later competing error, vector prefixes, array errors and subsequent reader position. Valid/truncated arrays cover word and 32/64/128-byte boundaries. Existing owned/borrowed decoding, limits and reusable-buffer tests pass. OR reduction finds an invalid block exactly when some byte contains a bit above bit zero; no unchecked operations are added.

Original production paths were benchmarked before editing. The [88 initial observations](2026-10-25-video-wav-bool-stages.tsv) retain before/first-after probes. Examples: 1080p 59,907,587.50 -> 40,368,248.00 cycles; 16-byte WAV with refills 9,002.46 -> 8,278.77 cycles and 1 -> 0 allocations; valid 4096-byte Boolean decoding 545.60 -> 451.15 cycles. The initial Boolean spelling used array chunks; the final equivalent slice-chunk spelling preserves MSRV and is measured in the repeated experiment. Final WAV benchmarks add buffered cases; final Boolean benchmarks add boundary lengths, identically for both archives.

Rejected: consuming large WAV extensions in 1024-byte pieces removed all format allocations but increased the 16385-byte trial from 10,032.80 to 25,952.80 cycles. The retained stack/vector split avoids that extra reading and copying. A thumbnail filename trial removed three temporary strings and saved three allocations / 24 requested bytes, but six process pairs showed a 4.8% throughput loss on the 128px 4x4 fixture. That production change and its added tests were reverted; artifacts remain only under ignored `target/direct-video-wav`.

## Reproduction

```powershell
cargo test --release --locked --config profile.release.lto=false -p deadlib-audio-decode -p deadlib-render-backend-software -p deadlib-bincode --lib --tests
cargo clippy --release --locked --config profile.release.lto=false -p deadlib-audio-decode -p deadlib-render-backend-software -p deadlib-bincode --lib --tests
cargo test --release --locked --config profile.release.lto=false -p deadlib-audio-decode --test wav_format_kernel --no-run --message-format=json
cargo test --release --locked --config profile.release.lto=false -p deadlib-render-backend-software --test texture_pipeline --no-run --message-format=json
cargo test --release --locked --config profile.release.lto=false -p deadlib-bincode --test direct_kernels --no-run --message-format=json
python scripts/benchmark_video_wav_bool.py --archive before=target/direct-video-wav/before --archive after=target/direct-video-wav/after --runs 6 --output docs/performance/2026-10-25-video-wav-bool.tsv
```

Copy executables from Cargo's compiler-artifact JSON paths into each archive directory as `wav_format_kernel.exe`, `texture_pipeline.exe` and `direct_kernels.exe`. Rebuild before with identical final test sources by replacing only `parse_format_chunk`, `rgba_pixel` and `invalid_bool` with definitions from `fc59baf80`; keep final test files/module declarations. Restore the final functions to build after. Build targets separately as above, and finish compilation before comparing them.

Local source snapshots, archive-build helpers, standalone assembly extracts, process logs, staged logs, release tests and Clippy logs are under ignored `target/direct-video-wav`.

| Executable | SHA-256 before | SHA-256 after |
| --- | --- | --- |
| `texture_pipeline` | `5a42aaf42e66e6eb1b6500e73f0f8660459c111807b078e3e64b52576d8ed717` | `fc91403465b0eaf58b454b76d798e10fe483935c2fb501c4ce376cd3f4919ac3` |
| `wav_format_kernel` | `5f750f6636c37ecf02e17563b27f117c19f9791a94523b8a1a36b8b4375c6bd0` | `3e4388ca9d2724bc300eba9cbf497f8dcda9e2f8e4c7cd497ed1d206f717b505` |
| `direct_kernels` | `de734d32d0ab72187a366c730fa317cd0b9f9a6cdad3524aafcd5d39d18241ec` | `43057599bdcbcd77cbc1528054cf2e9a60b84de939bd1801c923ed942106cff1` |

Nothing was pushed or published. The commit excludes both Lua crates, unrelated work and rejected production trials.
