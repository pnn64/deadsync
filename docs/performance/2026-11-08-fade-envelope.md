Active audio fades now dispatch to an FMA-enabled loop on supported x86 CPUs. Fixed mono/stereo layouts expose packed frame interpolation, and a bounded float-to-i32 conversion removes redundant PCM saturation. The portable path remains available, buffers shorter than four frames avoid FMA dispatch, and full-volume, silence and invalid-input exits occur before dispatch. The implementation adds no inline assembly, explicit intrinsics or allocation.

`apply_fade_envelope` supplies finite volume endpoints in [0, 1]. The change retains the f32 interpolation and sample product, then widens the product to f64 before adding the signed half bias. The biased result lies in [-32768.5, 32767.5], so truncation is representable as both i32 and i16. The unchecked conversion has a local safety explanation. FMA entry is guarded by runtime CPU/OS feature detection. Half ties away from zero, clipping, signed zero and incomplete trailing frames preserve their previous outputs.

When either endpoint exceeds 0.9999, the loop retains its per-frame near-unity skip. Otherwise the fixed-layout loop represents that skip as exact unit gain, which leaves every i16 unchanged while allowing vectorization. This selection matters: an intermediate prototype that always used the packed loop slowed the near-unity workload to 0.729x. The selected implementation improves that workload in both the prototype and integrated build.

The integrated Cargo release binaries measure **1.887x** faster stereo fading at 256 frames and **1.851x** at 4096 frames, reducing CPU cost by about **47.0%** and **46.0%**. Independent median wall times are **2.109 to 1.116 microseconds** and **33.953 to 18.357 microseconds**, respectively. These measure the complete fade helper plus buffer reset, not total decoder, mixer, audio-pipeline or frame CPU time. The [integrated raw data](2026-11-08-fade-envelope-raw.csv) retains all 1,920 observations, and the [summary](2026-11-08-fade-envelope-summary.csv) retains every case and range.

| Workload | Before cycles/call | After cycles/call | Median speedup | Six-pair range |
| --- | ---: | ---: | ---: | ---: |
| mono-256 | 3,114.7 | 1,825.7 | 1.701x | 1.657-1.738x |
| stereo-256 | 4,619.3 | 2,445.8 | 1.887x | 1.860-1.942x |
| mono-4096 | 48,133.9 | 28,272.2 | 1.709x | 1.680-1.735x |
| stereo-4096 | 74,366.5 | 40,175.3 | 1.851x | 1.845-1.979x |
| six-256 | 10,717.6 | 6,021.0 | 1.779x | 1.750-1.808x |
| six-4096 | 175,877.1 | 100,823.1 | 1.754x | 1.716-1.825x |
| stereo-tail | 4,605.1 | 2,468.1 | 1.876x | 1.817-2.071x |
| stereo-rise | 4,635.5 | 2,423.3 | 1.941x | 1.866-1.963x |
| full-volume | 96.8 | 103.6 | 0.969x | 0.924-1.218x |
| silence | 126.0 | 144.6 | 0.949x | 0.769-1.081x |
| empty | 26.6 | 26.2 | 1.026x | 0.947-1.062x |
| stereo-1 | 73.4 | 70.3 | 1.046x | 1.029-1.064x |
| near-unity | 7,099.5 | 4,900.0 | 1.441x | 1.408-1.495x |
| stereo-4 | 135.8 | 116.2 | 1.165x | 1.126-1.204x |
| stereo-16 | 355.0 | 223.8 | 1.575x | 1.497-1.663x |
| mono-16 | 243.9 | 181.0 | 1.346x | 1.301-1.435x |

The full-volume control measured **0.969x** and silence measured **0.949x**, about 3.2% and 5.3% more CPU cost by the paired ratios. Their independent median elapsed times increased by about **3.1 ns** and **8.5 ns** per call. These slower controls and their wider process ranges are retained. Empty-buffer and tiny active-fade cases also remain in the data. Reported ranges are observed process extrema, not confidence intervals or guarantees on other machines.

The standalone prototype measured stereo-256 at **1.856x** [1.807, 2.524] and stereo-4096 at **1.902x** [1.881, 2.655]. Its [raw data](2026-11-08-fade-envelope-prototype-raw.csv) retains 2,880 observations across baseline, runtime-selected and forced-portable variants; the [runtime summary](2026-11-08-fade-envelope-prototype-summary.csv) and [portable summary](2026-11-08-fade-envelope-prototype-portable-summary.csv) remain separate. The forced portable path improved stereo-256 by **1.248x** and stereo-4096 by **1.261x** on the same FMA-capable CPU. Those results do not measure an older non-FMA processor.

The original prototype assembly calls `fmaf` for every active frame and uses scalar `cvttsd2si` PCM conversions. The FMA helper uses packed `vdivps`, `vfmadd213ps` and `vcvttpd2dq` for common fixed layouts. The division remains a division, not an approximate reciprocal. Generic layouts use scalar FMA per frame and packed channel conversion where profitable. Assembly excerpts and the full measured assembly remain in the local audit directory. The production fade functions match the measured prototype after normalizing comments, whitespace and optional trailing parameter commas.

Measurements used an Intel Xeon E5-2696 v4, Windows x86_64 MSVC, rustc 1.99.1 / LLVM 23.1.1, opt-level 3, fat LTO, static CRT and generic CPU settings. The prototype used one codegen unit and panic abort; the integrated measurements use Cargo release test executables with panic unwinding. Prototype runs occurred amid unrelated compiler activity. No builds or correctness tests from this change overlapped the integrated timed runs.

The committed `benchmark_fade_envelope` calls the actual production helper through a non-inlined wrapper. Fixture allocation is outside timing. Each timed call copies the original samples into a retained output buffer before applying the fade, preventing repeated calls from reducing the fixture to silence; this copy cost is included in every result. Cases include mono/stereo/six-channel buffers, rising/falling fades, tails, near-unity behavior and early-exit controls. Every case checks output against the frozen scalar `.round()` oracle before timing.

Each process measures ten rounds per case, with `500_000 / max(frames, 1) + 200` calls per round. Only round zero is excluded. Six fresh before/after process pairs alternate binary order and rotate case order by 0, 5, 10, 15, 20 and 25. The test thread is pinned to logical CPU 2. `QueryThreadCycleTime` measures CPU cycles and `Instant` measures elapsed time. Speedups are medians of paired process-median ratios; absolute columns are independent medians of process medians.

The benchmark wrapper and workload source are identical between archived binaries. The after binary additionally contains the two new regression tests and shares the existing timing helper through one test-only module. SHA-256:

- Before executable: `61451befb40e2f8de19dc5b6ccd1f4d424edb0e208a9d008fb137bdfed06e982`.
- After executable: `d2d9fd424e9bc6202a4669bc15c1c558479b9ce0ef399e30e7556d4e3af75565`.
- Benchmark wrapper/workload: `3d9c5b78ca671babb39c97ce257cbf8ac9f247891bcf34d8539712f7e23a046a`.

Validation:

- **34 baseline library release tests pass; 36 updated library release tests pass.** Six opt-in benchmarks/exhaustive checks are ignored in each ordinary run. All 12 archived-binary benchmark invocations pass separately.
- The new regressions compare **239,047,600 PCM samples across 20,576 envelope cases** against the frozen scalar `.round()` oracle, exercising both the runtime-selected implementation and the production portable helper. Coverage includes every i16 value, rising/falling fades, signed 64-bit endpoint and unsigned 64-bit start extremes, channel counts 0 through 16 plus a 65,536-channel case, empty/short buffers, partial trailing frames, and random samples/lengths. Guard samples around slices and varied offsets are also checked.
- The existing **two Cargo `fade_equivalence` integration tests pass**, including independent interpolation/rounding, near-unity and extreme-envelope coverage. Existing fade module tests pass in the library suite.
- The two new regression tests also pass at opt-level 0 with debug assertions and overflow checks. This focused rustc harness includes the production resample module and committed test files, omitting only the unused Rubato parameter constructor/import; it is not a full Cargo debug build.
- The same focused production module passes ARM64 and i686 Windows metadata compile checks. These are compile checks, not full cross-target builds or runtime tests.
- Clippy completes for all crate targets with existing type-complexity warnings in `opus.rs`/`wav.rs` and the pre-existing duplicated `paired_bench` module. The new and changed code has no diagnostics. New-test formatting and staged whitespace checks pass.

Reproduction from the repository root:

```powershell
cargo test --release --locked -j 2 -p deadlib-audio-decode --lib -- --test-threads=1
cargo test --release --locked -j 2 -p deadlib-audio-decode --test fade_equivalence -- --test-threads=1
$env:FADE_BENCH_ROTATION = '0'
cargo test --release --locked -p deadlib-audio-decode --lib resample::fade_performance::benchmark_fade_envelope -- --exact --ignored --nocapture --test-threads=1
Remove-Item Env:FADE_BENCH_ROTATION
cargo clippy --release --locked -j 2 -p deadlib-audio-decode --all-targets --no-deps
```

For comparative timing, archive the test executable from `cargo test --release --locked -j 2 -p deadlib-audio-decode --lib --no-run --message-format=json` before and after the production change, retaining the same benchmark wrapper/workloads. Run the exact ignored benchmark directly from each binary with all six rotations above, alternating process order. Keep builds and other checks outside timing. Local binaries, manifests, source snapshots, prototype assembly, rejected variants, validation logs and the runner are retained in `target/fade-envelope-audit-2026-11-08/`.
