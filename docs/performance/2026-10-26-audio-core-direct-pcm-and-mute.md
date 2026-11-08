# Audio core: direct PCM and muted rendering

This batch retains three optimizations: copying unity-gain music directly to i16 output, filling muted i16 output without float conversion, and writing muted f32 samples without PCM normalization or per-frame gain multiplication. The baseline follows `b67903f9b`. Music block traversal, generation transitions, pool ownership, timing records, callback reports, and gain ramp arithmetic are preserved. No public API, dependency, or unsafe code was added.

Each change was isolated with the same 32-workload harness. The four archived stages are `pcm-three-before`, `pcm-three-unity`, `pcm-three-i16-mute`, and `pcm-three-final`. Their source manifests differ only in `src/render.rs`. The table compares consecutive stages, so each row measures the named change independently.

| Change and workload | Before cycles/callback | After cycles/callback | CPU throughput |
| --- | ---: | ---: | ---: |
| Unity i16, stereo, 4,096 samples | 14,587.31 | 2,327.24 | 6.27x |
| Unity i16, stereo, 512 samples | 1,984.58 | 435.15 | 4.56x |
| Silent i16, 4,096 samples | 1,294.96 | 412.24 | 3.14x |
| Muted i16, steady gain, stereo, 4,096 samples | 15,029.45 | 2,245.82 | 6.69x |
| Muted i16, gain ramp, stereo, 4,096 samples | 33,325.32 | 21,706.67 | 1.54x |
| Muted i16, gain ramp, six channels, 12,288 samples | 81,910.76 | 25,971.45 | 3.15x |
| Muted f32, gain ramp, six channels, 12,288 samples | 45,912.76 | 30,105.38 | 1.53x |
| Muted f32, gain ramp, six channels, 1,536 samples | 5,793.49 | 3,881.12 | 1.49x |
| Muted f32, steady gain, stereo, 4,096 samples | 4,293.97 | 3,926.04 | 1.09x |

[Confirmed measurements](2026-10-26-audio-core-three-rendering-confirmed.tsv), the adjacent [manifest](2026-10-26-audio-core-three-rendering-confirmed.manifest.json), and [summary selections](2026-10-26-audio-core-rendering-summary.json) contain all stages, workloads, and checks. All measured callbacks have zero allocation, reallocation, free, and byte-churn counts before and after. These are conditional callback measurements, not whole-game speedup estimates.

Measurement uses Windows `QueryThreadCycleTime`, logical CPU 2, one warmup and eleven timed batches per workload, and medians across six runs. Stage order rotates between runs. The machine uses Windows 11 and rustc 1.99.1 on x86-64 MSVC, with the existing release optimization, LTO, and static runtime settings. Fixture allocation is outside timing; feeding pooled music, rendering, and draining played-map records are inside timing. Manifests record compiler details, source hashes, executable hashes, Cargo settings, and the lockfile.

The fast paths have costs on some controls. Comparing the original and final stages, non-unit i16 music at 512 samples increases from 1,956.89 to 2,059.82 cycles (5.3%); f32 non-unit music increases from 643.33 to 697.19 cycles (8.4%); i16 SFX increases from 2,354.37 to 2,470.30 cycles (4.9%). At 4,096 samples, non-unit i16 and f32 music increase by 2.2% and 1.0%, respectively. Float stereo ramps remain on the existing conversion path and stay near baseline. All controls are included in the confirmed data.

For unity i16 rendering, the music gain must be settled, `stream_gain * target_gain` must equal one, and no active SFX may need float mixing. The renderer then copies each consumed PCM block directly into the device buffer and zeroes the underrun tail. This removes the float scratch pass, PCM normalization and scaling, float-to-i16 quantization, and scratch-buffer ownership transfers from that path. The same traversal still consumes blocks and publishes their timing records. Compile-time modes select the output operation without a runtime output enum inside the traversal.

Muted i16 rendering uses that traversal without reading sample values, then fills the output with zero. Eligibility requires a zero stream volume (including negative zero), a finite nonnegative current gain, and a finite positive target gain. Advancing toward that target keeps gains positive for each rendered sample, so all resulting i16 samples are zero. The existing sequential `advance_gain` operation still runs once per device frame until settled, including underrun frames. It runs after block traversal because muted output does not depend on its magnitude; no closed-form ramp approximation was introduced.

Muted f32 rendering uses the same gain guard. It writes `f32::from(sample) * stream_volume`, retaining the sign of every zero while removing normalization and multiplication by the positive gain. The same exact ramp progression follows. This specialization is used for settled gains and for non-stereo ramps. Active stereo ramps retain the original loop because splitting that loop did not improve its throughput. SFX mixing and final clipping follow the existing path. The per-chunk level snapshot retains the original atomic-read order, including the additional target read when applying a gain snap.

Correctness checks compare an actual 2,588,580-byte trace across all four final stages and the original pre-change renderer capture. Every byte matches, with SHA-256 `d20cd6571f3061a9330779228c16490a3e8ce04137a1c0144961c8defa255389`. The trace includes i16 samples, exact f32 bits, underrun and gap reports, stream-frame totals, voice state, outstanding pool credits, drained generations, and every played timing record. It covers 336 callbacks over channel counts 0, 1, 2, 3, 6, and 8; empty and incomplete frames; chunk/block boundaries; underruns; generation publication; gain ramps and snaps; negative-zero volume; later unmuting; and paused, resumed, and scheduled SFX. The comparison script rejects unequal trace bytes before timing.

An exhaustive public-renderer test round-trips all 65,536 i16 values at effective unity gains `(1, 1)`, `(2, 0.5)`, `(0.5, 2)`, and `(3, 1/3)`. Existing gain differential checks also include tiny positive targets, signed zero, and extreme finite values. Final validation passes 130 release tests with thirteen ignored benchmarks/trace generators, 56 debug library tests, and the public-renderer test in debug mode. Clippy passes with warnings denied and the same two pre-existing lint categories allowed: `chunks_exact_to_as_chunks` and `too_many_arguments`. Changed Rust files pass formatting; Python compilation, scoped whitespace checks, executable hashes, trace bytes, and final source hashes pass. Full command output is in [validation](../testing/2026-10-26-audio-core-pcm-validation.json).

Rejected candidates remain measured, with their production changes removed. A [runtime output enum](2026-10-26-audio-core-direct-pcm.tsv) added fallback overhead and was replaced by [compile-time specialization](2026-10-26-audio-core-specialized-pcm.tsv). [Centered-SFX FMA normalization](2026-10-26-audio-core-sfx-confirmed.tsv) improved centered mixing by 3–6% but regressed panned controls; an [outlining comparison](2026-10-26-audio-core-sfx-layout.tsv) did not resolve all regressions. A [wider-precision SFX candidate](2026-10-26-audio-core-sfx-wide-candidate.tsv) was substantially slower. A [split private muted-conversion loop](2026-10-26-audio-core-muted-split.tsv) helped surround output but slowed stereo ramps, motivating the retained renderer-level paths. `mixer.rs` and the panned-mixing tests are unchanged in this batch.

Archived source snapshots and executables are under `target/audio-core-kernels`; adjacent committed manifests identify them. To repeat the final comparison against the local original archive:

```powershell
python scripts/benchmark_audio_core.py --build render-current --target rendering_kernels
python scripts/benchmark_audio_core.py --compare pcm-three-before render-current --runs 6 --output target/audio-core-kernels/render-repeat.tsv
```

For a fresh baseline, use the production `render.rs` from `b67903f9b` with the current rendering harness. The benchmark script verifies source stability during compilation. Final source and binary hashes match the `pcm-three-final` archive.
