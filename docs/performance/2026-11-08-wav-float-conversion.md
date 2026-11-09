Float WAV packet decoding now bounds the rounded sample before converting through i32, allowing LLVM to vectorize the float32 and float64 loops. Separate non-inlined float packet helpers keep their vector-register saves out of the integer decoder's prologue. The implementation adds no inline assembly, intrinsics, runtime dispatch, CPU feature requirement or allocation.

The change preserves the existing scaling and signed-half rounding bias. Float32 input still widens to f64 before multiplication. Float64 retains its under-half guard, which avoids rounding a value just below a half up to one when adding the bias. Both paths still map nonfinite input to silence. The helper uses `max(-32768.0).min(32767.0)` so even NaN or overflow infinity becomes finite and representable before `to_int_unchecked::<i32>()`. Truncation and narrowing to i16 are then valid. A local Clippy allowance explains why replacing min/max with NaN-preserving `clamp` would invalidate that safety argument.

Integrated Cargo release measurements show **1.562x** faster float32 decoding and **1.433x** faster float64 decoding at 8192 samples, reducing CPU cost by about **36.0%** and **30.2%**. This is the normal 4096-frame stereo packet size. Independent median wall times fall from **15.777 to 9.970 microseconds** for float32 and **20.280 to 14.283 microseconds** for float64.

These measurements cover the complete `decode_packet_into` path, including format dispatch, output clearing and vector extension into retained storage. They do not measure disk I/O, whole-file decoding, mixer CPU, total playback CPU or frame time. The [integrated raw data](2026-11-08-wav-float-conversion-raw.csv) retains all 2400 observations; the [summary](2026-11-08-wav-float-conversion-summary.csv) retains every case and range.

| Workload | Before cycles/packet | After cycles/packet | Median speedup | Six-pair range |
| --- | ---: | ---: | ---: | ---: |
| f32-audio-64 | 336.7 | 201.9 | 1.663x | 1.599-1.691x |
| f32-audio-8192 | 34,563.8 | 21,846.0 | 1.562x | 1.524-1.617x |
| f32-clipped-64 | 335.3 | 201.7 | 1.650x | 1.548-1.777x |
| f32-clipped-8192 | 34,468.9 | 21,975.6 | 1.555x | 1.498-1.629x |
| f32-exceptional-64 | 242.8 | 201.0 | 1.190x | 1.133-1.230x |
| f32-exceptional-8192 | 28,010.2 | 22,265.3 | 1.252x | 1.233-1.278x |
| f64-audio-64 | 398.2 | 285.8 | 1.381x | 1.344-1.427x |
| f64-audio-8192 | 44,427.8 | 31,286.7 | 1.433x | 1.394-1.466x |
| f64-clipped-64 | 402.0 | 286.6 | 1.402x | 1.293-1.438x |
| f64-clipped-8192 | 44,556.2 | 31,164.9 | 1.422x | 1.310-1.459x |
| f64-exceptional-64 | 300.5 | 285.2 | 1.036x | 0.992-1.072x |
| f64-exceptional-8192 | 34,800.8 | 31,326.1 | 1.103x | 1.080-1.156x |
| pcm8-audio-64 | 36.9 | 34.9 | 1.051x | 0.978-1.091x |
| pcm8-audio-8192 | 1,351.5 | 1,341.4 | 1.022x | 0.978-1.099x |
| pcm16-audio-64 | 32.3 | 29.8 | 1.097x | 0.937-1.119x |
| pcm16-audio-8192 | 1,314.3 | 1,374.7 | 0.960x | 0.777-1.135x |
| pcm24-audio-64 | 102.4 | 102.9 | 0.994x | 0.950-1.022x |
| pcm24-audio-8192 | 12,300.3 | 12,287.1 | 1.002x | 0.948-1.027x |
| pcm32-audio-64 | 38.6 | 37.1 | 1.036x | 0.937-1.131x |
| pcm32-audio-8192 | 2,618.3 | 2,573.5 | 1.015x | 0.977-1.087x |

The large PCM16 memcpy control measures **0.960x**, with a broad **0.777-1.135x** process range. Its independent median cost rises from 1314.3 to 1374.7 cycles per packet. This is a noisy, slightly slower observation, not evidence of identical integer PCM performance. Other integer controls range from 0.994x to 1.097x by their paired medians. All controls are retained; observed ranges are process extrema, not confidence intervals. No integer conversion arithmetic changed.

The earlier standalone prototype measured **1.563x** float32 and **1.424x** float64 at 8192 samples. Its tiny exceptional float64 packets measured **0.952x**; the integrated build measures **1.036x** for that case, with a range crossing parity. The exceptional fixture includes signed zero, NaN, infinities, extreme finite values and 0.25. Keep this build-sensitive small-packet behavior in view. The [prototype raw data](2026-11-08-wav-float-conversion-prototype-raw.csv) and [prototype summary](2026-11-08-wav-float-conversion-prototype-summary.csv) remain separate from the integrated results; its PCM16 controls also showed wider variation.

Assembly generated from the production WAV module shows the original scalar `cvttsd2si` conversions replaced by packed `mulpd`, `addpd`, `maxpd`, `minpd` and `cvttpd2dq` in the float helpers. The float32 loop handles four samples per iteration through two double-precision vectors, and the float64 loop handles two. Scalar tails remain. The packet dispatcher no longer saves the float helpers' callee-saved vector registers. Original/candidate and production assembly remain in the local audit directory.

Measurements used an Intel Xeon E5-2696 v4, Windows x86_64 MSVC, rustc 1.99.1 / LLVM 23.1.1, release opt-level 3, fat LTO, static CRT and generic CPU settings. Integrated measurements use Cargo release test executables with panic unwinding. The prototype used one codegen unit and panic abort. No builds or correctness tests from this task overlapped integrated timed runs; unrelated machine activity was not stopped.

The committed `benchmark_wav_float_conversion` uses the actual production helper through a non-inlined wrapper. Fixtures and retained output storage are prepared outside timing. Every fixture is checked against an independent scalar rounding oracle before timing. PCM input/output relative page offsets are fixed at 128 bytes to reduce address-aliasing variation in memcpy controls. Each process measures ten rounds per case, with `max(8_000_000 / samples, 1000)` calls per round. Only round zero is excluded. Six fresh before/after process pairs alternate binary order and rotate case order by 0, 3, 6, 9, 12 and 15. The test thread is pinned to logical CPU 2; `QueryThreadCycleTime` measures CPU cycles and `Instant` measures elapsed time. Speedups are medians of six paired process-median ratios. Absolute columns are independent medians of process medians.

The benchmark and regression-test source is identical between archived binaries. The baseline contains the original production decoder plus the new test-only module. SHA-256:

- Before executable: `ade1a2c9047ff501f9f6c68d042f88053a91c1baa73301cc1d0a1a24f275e4b6`.
- After executable: `c10d626c158a862475b16e8f0eabb2ecfe34d0ae0735a4dcc7599889d09ad458`.
- Benchmark/regression module: `73bc857256e09ffa62f3c9f8132ff739224bcf8979769d1ded56f98ed6fa6272`.

Validation:

- **37 baseline and 37 updated library release tests pass**, with eight opt-in benchmarks/exhaustive checks ignored in ordinary runs. All twelve archived-binary benchmark invocations pass separately.
- The committed exhaustive check, run against the updated Cargo release binary, matches **all 4,294,967,296 f32 bit patterns** against the frozen original conversion.
- The new packet regression compares **1,926,208 exact PCM samples across 6656 packet/storage cases**, covering empty buffers, short vector tails, 32 byte offsets, clipping, subnormals, nonfinite values and full packet boundaries. It also verifies unchanged capacity/pointers and unchanged output on every possible partial-sample remainder.
- Both focused Cargo integration suites, `wav_float_kernel` and `wav_format_kernel`, pass **seven tests each** (including shared WAV tests). They cover independent float32/float64 rounding boundaries, random bit patterns, allocation churn, known integer edges, file packet boundaries, seeking, parsing and error behavior.
- The focused float WAV integration harness passes **seven tests at opt-level 0**, with debug assertions and overflow checks, including the new regression. This is a direct rustc build of the existing integration test and production WAV module, not a full Cargo debug build.
- The production WAV module passes ARM64 and i686 Windows metadata compile checks. These are focused compile checks, not full cross-target builds or runtime tests.
- `cargo check --release` and Clippy for all crate targets complete. Clippy reports type-complexity and duplicate test-helper module warnings. No arithmetic or unsafe-code diagnostic is reported. Changed WAV files pass formatting and whitespace checks. Crate-wide `cargo fmt --check` reports two pre-existing wrapping differences in `resample.rs`; that unrelated file is unchanged.

Reproduction from the repository root:

```powershell
cargo test --release --locked -j 2 -p deadlib-audio-decode --lib -- --test-threads=1
cargo test --release --locked -j 2 -p deadlib-audio-decode --test wav_float_kernel --test wav_format_kernel -- --test-threads=1
cargo test --release --locked -j 2 -p deadlib-audio-decode --lib wav::float_performance::exhaustive_float32_packets_match_original -- --exact --ignored --nocapture --test-threads=1
$env:WAV_FLOAT_BENCH_ROTATION = '0'
cargo test --release --locked -j 2 -p deadlib-audio-decode --lib wav::float_performance::benchmark_wav_float_conversion -- --exact --ignored --nocapture --test-threads=1
Remove-Item Env:WAV_FLOAT_BENCH_ROTATION
cargo check --release --locked -j 2 -p deadlib-audio-decode
cargo clippy --release --locked -j 2 -p deadlib-audio-decode --all-targets --no-deps
```

For comparative timing, archive the library test executable from `cargo test --release --locked -j 2 -p deadlib-audio-decode --lib --no-run --message-format=json` before and after the production change while retaining the identical benchmark module. Run the exact ignored benchmark directly from each binary with all six rotations, alternating process order. Keep builds and other checks outside timing. Local binaries, manifests, source snapshots, assembly, checks and the runner are retained in `target/wav-cast-audit-2026-11-08/`.
