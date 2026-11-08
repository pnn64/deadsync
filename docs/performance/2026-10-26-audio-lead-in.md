# Allocation-free lead-in silence, 2026-10-26

A negative music start calls `push_silence`, which allocated and zeroed a 2048-frame interleaved buffer even for a one-frame lead-in. Mono through eight-channel output now borrows a prefix of one immutable 32 KiB static. This removes the allocation, buffer clearing, and destruction from each call for these layouts. Wider output retains the owned buffer. The same 2048-frame chunks reach the existing 256-frame block writer, keeping source-time accumulation, generation checks, and cancellation control flow intact. The buffer selection adds nine net production lines and introduces no unsafe code, lazy initialization, locks, or per-worker cache state.

The fixed read-only buffer occupies 32,768 bytes. The archived release unit-test executable grows from 2,335,744 to 2,371,584 bytes (35,840 bytes), including the added allocation regression test. This trades fixed shared storage for 4-32 KiB of transient heap work per call; it is not a claim of lower total process memory. The benchmark helpers share the existing cycle-counter module rather than loading a duplicate copy.

The baseline worker source matches commit `b4ecd1e98` exactly. For each nonempty tested call, allocation counters change as follows:

| Layout | Allocations before -> after | Allocated bytes before -> after | Frees before -> after |
| --- | ---: | ---: | ---: |
| Mono | 1 -> 0 | 4,096 -> 0 | 1 -> 0 |
| Stereo | 1 -> 0 | 8,192 -> 0 | 1 -> 0 |
| Six channels | 1 -> 0 | 24,576 -> 0 | 1 -> 0 |
| Eight channels | 1 -> 0 | 32,768 -> 0 | 1 -> 0 |

**Measurement protocol.** Actual `deadlib-audio` release unit-test binaries, using optimization level 3, fat LTO, one codegen unit, debug symbols, static CRT, and the generic CPU target. Host: Intel Xeon E5-2696 v4, Windows x86_64, rustc 1.99.1 / LLVM 23.1.1. The benchmark thread is pinned to logical CPU 2. Eight fresh processes per version alternate order. Each process has one warmup and eleven measured batches; the table uses medians of scheduled thread cycles from `QueryThreadCycleTime`.

Each timed batch submits lead-ins through 64 preconstructed, separate writer pools. Construction, destruction, and rendering are outside timing. The single-call allocation probe precedes cycle batches, so the shared buffer is warm. Both versions use the same scoped counting System allocator. The figures describe submission work; full application startup and first-ever page-fault latency are not isolated. Wall times are retained in the raw data. CPU throughput is median-before / median-after, and ranges include every process pair.

| Layout / lead-in frames | Before cycles/call | After cycles/call | CPU throughput | Paired range |
| --- | ---: | ---: | ---: | ---: |
| `lead-in/1ch/1` | 1,392.46 | 557.32 | 2.498x | 1.900-2.697x |
| `lead-in/1ch/2048` | 2,651.16 | 1,929.20 | 1.374x | 1.119-1.583x |
| `lead-in/1ch/731` | 1,876.04 | 1,008.32 | 1.861x | 1.511-2.107x |
| `lead-in/1ch/8192` | 6,612.43 | 6,003.66 | 1.101x | 0.917-1.245x |
| `lead-in/2ch/1` | 1,534.78 | 593.33 | 2.587x | 1.299-3.500x |
| `lead-in/2ch/2048` | 3,434.83 | 2,573.99 | 1.334x | 1.153-1.574x |
| `lead-in/2ch/731` | 2,181.28 | 1,344.44 | 1.622x | 1.338-1.811x |
| `lead-in/2ch/8192` | 10,076.42 | 8,812.58 | 1.143x | 0.845-1.278x |
| `lead-in/6ch/1` | 2,803.78 | 931.15 | 3.011x | 2.060-4.280x |
| `lead-in/6ch/2048` | 10,584.01 | 7,420.14 | 1.426x | 0.814-2.130x |
| `lead-in/6ch/731` | 5,472.08 | 2,959.83 | 1.849x | 1.171-2.647x |
| `lead-in/6ch/8192` | 31,954.39 | 25,004.14 | 1.278x | 0.594-1.661x |
| `lead-in/8ch/1` | 4,595.79 | 965.45 | 4.760x | 2.590-6.440x |
| `lead-in/8ch/2048` | 11,077.88 | 11,563.19 | 0.958x | 0.700-1.391x |
| `lead-in/8ch/731` | 7,246.93 | 4,664.38 | 1.554x | 0.976-2.022x |
| `lead-in/8ch/8192` | 35,198.88 | 38,818.92 | 0.907x | 0.575-1.176x |

Stereo at 731 frames improves 1.622x and mono at 731 frames improves 1.861x. One-frame cases improve 2.498-4.760x. The retained eight-channel 2048- and 8192-frame fixtures measure 0.958x and 0.907x throughput; paired ranges show substantial variability. The allocation removal holds across every retained measurement, including those slower fixtures. Full [raw measurements](2026-10-26-audio-lead-in-retained.tsv) and [source/compiler/binary manifests](2026-10-26-audio-lead-in-retained.manifest.json) accompany this report.

**Correctness.** Before and retained binaries produce identical 2,245,760-byte captures, compared byte for byte. Captures include rendered i16 samples, returned f64 source-time bits, and every played-map frame position, frame count, source-time start and step. They cover 1/2/3/6/8 channels, frame counts 0/1/255/256/257/731/2048/2049/8192, and rates 0.05/1.0/1.2/8.0 at 48 kHz: 180 cases. SHA-256: `d3e7111ac0ff4f4fc8633235c562eaa3f3e20c8d73693acaa1b91968751b9f21`. [Identity evidence](../testing/2026-10-26-audio-lead-in-identity.json).

An independent transport test verifies zero PCM, queued block counts, generation tags, and exact f64 timing for those cases plus nine-channel output, covering the owned-buffer fallback. An allocation regression test submits mono, stereo, three-, six- and eight-channel lead-ins through real writer pools with no allocator work. All 46 unit tests and the playback integration test pass in both debug and release. Six benchmark/capture tests remain ignored by the default suite; the new benchmark and capture tests were run separately. Clippy performance checks and rustfmt pass. The existing unused `is_direct` warning in the old processing test baseline remains. [Check commands, logs, and source hashes](../testing/2026-10-26-audio-lead-in-checks.json).

**Exploration.** An initial 8 KiB constant covered mono/stereo only and retained wider allocations ([measurements](2026-10-26-audio-lead-in.tsv)). Expanding to 32 KiB removed allocations through 7.1 ([three-stage comparison](2026-10-26-audio-lead-in-common.tsv)). Clippy's large-constant rule then prompted explicit immutable static storage; the final binaries above were rebuilt and measured after that change and benchmark-module reuse. Intermediate constants are not retained.

Reproduce with `python scripts/benchmark_audio_lead_in.py --build NAME` before each source change, then `--compare BEFORE AFTER --runs 8 --output PATH.tsv` and `--verify BEFORE AFTER --output PATH.json`. Archives and source snapshots are under `target/audio-lead-in`. The runner records only its three Cargo profile/target controls, source hashes, compiler version and binary hash. Release comparison uses `CARGO_PROFILE_RELEASE_DEBUG=1` and `CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1`; workspace release settings supply optimization level 3 and fat LTO.
