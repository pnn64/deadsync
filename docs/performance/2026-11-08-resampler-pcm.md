Resampler output conversion now clamps the scaled float and maps NaN to zero before a locally justified `to_int_unchecked::<i32>()` conversion. The fractional correction still rounds half ties away from zero. The prototype assembly replaces eight scalar `cvttss2si` conversions, lane extraction and reconstruction with two packed `cvttps2dq` conversions for each eight-sample stereo block. The code is portable Rust with no inline assembly, intrinsics, CPU dispatch, new allocation or software-renderer changes.

The input product remains `f32`. Clamping to the integral endpoints -32768 and 32767 before rounding preserves clipping, including infinities. NaNs explicitly become zero. Every input to the unchecked conversion is therefore finite and within the i16 range, which is also safely representable as i32. Converting the integer back to f32 is exact. The half-tie correction cannot cross the integral clamp endpoints, so the final i16 cast needs no saturation. Mono output and mono-to-stereo duplication use the same helper; generic multichannel conversion is unchanged.

The integrated Cargo release binaries measure **1.874x** faster conversion for a 256-frame stereo buffer, about **46.6% lower CPU cost**. Independent median wall times are **1.105 to 0.590 microseconds**, saving about **0.514 microseconds per call**. Mono output with 1024 frames improves **1.951x**. These measure the complete `write_resampler_output` helper with reused storage, not the decoder, sinc filter, mixer, complete audio pipeline or frame rate.

The [integrated raw data](2026-11-08-resampler-pcm-raw.csv) retains all 1,560 observations, and the [summary](2026-11-08-resampler-pcm-summary.csv) retains all 13 cases, including controls and slower measurements.

| Workload | Before cycles/call | After cycles/call | Median speedup | Six-pair range |
| --- | ---: | ---: | ---: | ---: |
| stereo-4 | 60.5 | 43.7 | 1.393x | 1.306-1.443x |
| stereo-256 | 2,419.6 | 1,294.5 | 1.874x | 1.419-1.988x |
| stereo-257 | 2,488.3 | 1,291.0 | 1.937x | 1.400-2.374x |
| stereo-1024 | 10,724.6 | 5,949.8 | 1.797x | 1.451-1.990x |
| stereo-4096 | 42,571.1 | 23,238.8 | 1.832x | 1.449-1.880x |
| mono-to-stereo-256 | 1,091.2 | 672.4 | 1.632x | 1.528-2.171x |
| mono-to-stereo-1024 | 5,199.6 | 3,054.6 | 1.648x | 0.635-1.788x |
| mono-256 | 1,082.9 | 570.1 | 1.848x | 1.371-1.988x |
| mono-1024 | 4,110.9 | 2,100.6 | 1.951x | 1.719-2.254x |
| six-to-stereo-256 | 2,486.1 | 1,311.2 | 1.919x | 1.355-2.213x |
| stereo-to-six-256 | 5,317.2 | 5,421.6 | 0.997x | 0.944-1.031x |
| stereo-1 | 44.5 | 41.7 | 1.082x | 1.012-1.198x |
| empty-output | 24.3 | 24.6 | 0.975x | 0.947-1.043x |

Unrelated compiler activity was present during measurement. In particular, one mono-to-stereo 1024-frame process pair measured **0.635x**, while the median was **1.648x**; the slower observation is retained. The empty-output control measured **0.975x**, about 2.6% more CPU cost, and the unchanged stereo-to-six-channel path measured **0.997x**. The observed ranges describe process variability and are not confidence intervals or a guarantee for every workload or machine.

The initial standalone fat-LTO prototype measured stereo-256 at **1.916x** [1.713, 1.951], mono-1024 at **2.013x** [1.841, 2.068], and mono-to-stereo-256 at **1.670x** [1.593, 1.703]. Its [raw data](2026-11-08-resampler-pcm-prototype-raw.csv) and [summary](2026-11-08-resampler-pcm-prototype-summary.csv) remain separate. It compiled source snapshots together with panic abort and one codegen unit; the integrated measurements use separately archived Cargo release test executables with panic unwinding. Both use opt-level 3, fat LTO, static CRT and generic CPU settings on an Intel Xeon E5-2696 v4, Windows x86_64 MSVC, rustc 1.99.1 / LLVM 23.1.1.

The committed opt-in `benchmark_resampler_pcm_conversion` test calls the actual production helper through a non-inlined wrapper. Input planes, output capacity and initial output length are prepared before timing. Input values span normalized audio samples. Each case performs `1_000_000 / max(frames, 1) + 500` conversions per round for ten rounds. Only round zero is excluded. Six independent before/after process pairs alternate binary order, with case rotations 0, 5, 10, 15, 20 and 25. Threads are pinned to logical CPU 2. `QueryThreadCycleTime` measures CPU cycles, and `Instant` measures wall time. Ratios are medians of the six paired process-median ratios; absolute columns are independent medians of process medians. No builds or other tests from this change overlapped timing.

Both archived binaries contain the same test source. SHA-256:

- Before executable: `058cec62667b9a1572aaba9cc0266159b4395044ffa927b5466c55451ca18282`.
- After executable: `ab92110cfc8a647897d77b98d461f91517cf56a53248c04fde2f7a46556081c5`.
- Test module: `bbc0c6fc94332568db8185a423bf219bd24e7b1cc8b6f81e6fca628e559dc4e0`.

Validation:

- **34 library release tests pass before and after**, with five opt-in benchmarks/exhaustive checks ignored during those runs.
- The new normal regression covers **262,167 scalar values**, including neighbors of every half tie, clipping, signed zero, infinities, signed NaN payloads and random float patterns. It compares both the old implementation and the independent `(sample * 32767.0).round() as i16` expression.
- The same values pass through mono, duplicated-mono and stereo output loops, checking **1,310,835 output samples** against the scalar oracle. Another **10,530** cases check input/output channel counts 0 through 8, shortened planes, requested frame bounds, vector tails, empty input, buffer growth and retained pointer/capacity.
- The opt-in exhaustive test in the updated Cargo release binary matches **all 4,294,967,296 f32 bit patterns** against the old conversion (29.15 seconds). All 12 archived-binary benchmark invocations also pass.
- The two focused regression tests pass with opt-level 0, debug assertions and overflow checks. This standalone test harness includes the production resample module and committed tests, omitting only the unused Rubato parameter constructor/import; it is not a full Cargo debug build.
- The same standalone production module passes ARM64 and i686 Windows metadata compile checks. These are focused portability checks, not full cross-target builds or execution tests.
- Clippy completes for all crate targets. Its warnings concern existing type complexity in `opus.rs`/`wav.rs` and the pre-existing duplicate `paired_bench` module. The changed code has no new diagnostics. New-module formatting and staged whitespace checks pass.

Reproduction from the repository root:

```powershell
cargo test --release --locked -j 2 -p deadlib-audio-decode --lib -- --test-threads=1
cargo test --release --locked -p deadlib-audio-decode --lib resample::conversion_tests::exhaustive_pcm_conversion_matches_every_f32_pattern -- --exact --ignored --nocapture --test-threads=1
$env:RESAMPLER_BENCH_ROTATION = '0'
cargo test --release --locked -p deadlib-audio-decode --lib resample::conversion_tests::benchmark_resampler_pcm_conversion -- --exact --ignored --nocapture --test-threads=1
Remove-Item Env:RESAMPLER_BENCH_ROTATION
cargo clippy --release --locked -j 2 -p deadlib-audio-decode --all-targets --no-deps
```

For before/after timing, first build with the old scalar body retained in `original_sample` in the test module, and archive the test executable emitted by `cargo test --release --locked -j 2 -p deadlib-audio-decode --lib --no-run --message-format=json`. Restore the new production body, build and archive again, then run the exact ignored benchmark from each archived binary with the six rotations above, alternating process order. Do not include builds or fixture setup in timing. Local archived executables, manifests, assembly, source snapshots, validation logs and the paired-process runner are retained in `target/resampler-cast-audit-2026-11-08/`.
