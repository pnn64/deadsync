# Native PCM storage in SOLA, 2026-10-26

SOLA source buffers now retain i16 samples. This halves their sample storage and removes packet-time i16-to-f32 conversion; bulk input improves about 1.28-1.33x. Total initialized worker heap drops about 49% in the measured channel layouts. Whole-stream CPU gains are smaller and mixed, and crossfade conversion costs more than the previous f32-buffer implementation. The tables below retain those tradeoffs.

Previously decoding i16 packets produced normalized f32 source vectors, which the search retained until compaction. The worker now copies native PCM into those vectors and normalizes only when emitting output. Mono uses `extend_from_slice`; stereo and wider layouts deinterleave raw samples. Compaction moves half as many bytes. The mono conversion helper and unreachable first-score NaN handling are removed. Source cursors remain frame indices, so erasure, lookahead, rate changes, resets, and EOF control flow keep their original meaning.

**Exact arithmetic.** The old source vectors contained only i16 / 32768, with no interpolated output fed back into them. Each native absolute difference fits u16. Eight differences sum to at most 524,280, fitting u32 and converting to f32 exactly. Multiplication by 2^-15 restores the old score units. When the prefix is at most 496, the combined score remains below 512, where every multiple of 2^-15 is exactly representable. Larger prefixes retain the old per-sample floating addition order; rejection still occurs every eight samples. Strict comparison preserves earliest ties. Emission uses the same normalization and fused interpolation expression as before. There is no approximate conversion or floating fast-math flag.

**CPU specialization and soundness.** Native input adds conversion work to crossfades. An AVX2+FMA entry point shares the ordinary safe slice/Vec implementation, with Rust runtime CPU/OS checks before its unsafe call. The previous FMA-only entry point and scalar fallback remain. The added unsafe boundary concerns target-feature availability, with no unchecked memory access or manual vector loads. Under the new representation it improves the 1440-sample crossfade kernel 1.747x and the 256-sample kernel 1.235x versus FMA-only code. Both kernels remain slower than the original f32-buffer versions. Miri with tree borrows passes all four AVX2/FMA feature combinations, covering weighted and identity output. [Miri evidence](../testing/2026-10-26-audio-native-buffer-miri.json).

**Retained allocation measurements.** Constructor plus one 12,001-frame push, with inputs and outputs outside the allocator probe. Allocations are held until counting finishes. State handles and the f32 fade table stay the same; only source sample storage shrinks. Figures are requested allocation bytes, not process RSS.

| Channels | Allocation calls before -> after | Bytes before | Bytes after | Reduction |
| --- | ---: | ---: | ---: | ---: |
| 6 | 8 -> 8 | 294,024 | 150,012 | 48.98% |
| 8 | 10 -> 10 | 390,112 | 198,096 | 49.22% |
| 16 | 18 -> 18 | 774,464 | 390,432 | 49.59% |

The source payload itself changes from four to two bytes per sample. All warmed allocation probes in the final main and rate comparisons report zero allocations, reallocations, and allocated bytes.

**Protocol.** The production baseline matches commit `77df6706f`. Separately archived sources and typed test harnesses compile with rustc 1.99.1 / LLVM 23.1.1, optimization level 3, fat LTO, one codegen unit, static CRT, and generic CPU settings. Host: Intel Xeon E5-2696 v4, Windows x86_64. Each process pins its test thread to logical CPU 2, warms once, then takes a median over eleven CPU-cycle batches using `QueryThreadCycleTime`. Eight fresh processes per stage rotate execution order. Fixture construction stays outside timing; buffers are reused. Whole-stream cases feed 12,001 source frames in 997-frame packets, pull 256 frames, and include identical output fingerprinting work. These measure SOLA fixtures, not total application latency or FPS.

The final main comparison contains the original f32 version, the first native FMA-only version, and the retained AVX2-capable version. [Raw data](2026-10-26-audio-native-complete.tsv), [source/test/compiler/binary manifests](2026-10-26-audio-native-complete.manifest.json). The rate sweep compares original and retained versions using the same public push/pull fixture. [Rate data](2026-10-26-audio-native-complete-rates.tsv), [manifests](2026-10-26-audio-native-complete-rates.manifest.json).

**Original versus retained, all main workloads.** Throughput is median-before / median-after; paired ranges include every process pair. Search labels identify the correlation length, with length + 1 candidate offsets. Append labels unpack the entire 12,001-frame input packet.

| Workload | Before cycles/op | After cycles/op | CPU throughput | Paired range |
| --- | ---: | ---: | ---: | ---: |
| `sola/append-16ch` | 613,074.65 | 477,327.62 | 1.284x | 1.186-1.335x |
| `sola/append-6ch` | 219,221.35 | 165,350.39 | 1.326x | 0.785-1.354x |
| `sola/append-8ch` | 293,487.79 | 222,378.36 | 1.320x | 0.957-1.341x |
| `sola/crossfade-1440` | 405.35 | 727.41 | 0.557x | 0.472-0.857x |
| `sola/crossfade-256` | 99.57 | 179.95 | 0.553x | 0.500-0.713x |
| `sola/left-silent/360` | 128,647.57 | 123,372.74 | 1.043x | 1.009-1.079x |
| `sola/left-silent/720` | 491,789.72 | 488,277.75 | 1.007x | 0.984-1.065x |
| `sola/mono/360` | 122,360.95 | 120,611.81 | 1.015x | 0.964-1.047x |
| `sola/mono/720` | 492,119.03 | 485,859.81 | 1.013x | 0.979-1.049x |
| `sola/right-silent/360` | 122,306.10 | 120,810.74 | 1.012x | 0.966-1.050x |
| `sola/right-silent/720` | 494,458.05 | 486,933.30 | 1.015x | 0.998-1.039x |
| `sola/stereo/360` | 252,174.62 | 244,509.27 | 1.031x | 0.989-1.091x |
| `sola/stereo/720` | 985,177.74 | 975,602.09 | 1.010x | 0.985-1.059x |
| `sola/stream-1ch-noise-1.2` | 589,028.25 | 580,742.00 | 1.014x | 0.977-1.095x |
| `sola/stream-2ch-left-silent-1.2` | 778,182.50 | 763,311.00 | 1.019x | 0.874-1.286x |
| `sola/stream-2ch-noise-0.8` | 2,285,873.25 | 2,214,645.00 | 1.032x | 1.009-1.052x |
| `sola/stream-2ch-silence-1.2` | 110,792.75 | 108,433.00 | 1.022x | 0.914-1.189x |
| `sola/stream-2ch-tone-1.2` | 215,603.75 | 218,457.25 | 0.987x | 0.966-1.138x |
| `sola/stream-6ch-noise-1.2` | 3,783,137.50 | 3,548,437.00 | 1.066x | 1.022-1.082x |

**Additional whole-stream rates.** The fixture is the same 12,001-frame clip at 48 kHz, including EOF behavior at boundary speeds.

| Workload | Before cycles/op | After cycles/op | CPU throughput | Paired range |
| --- | ---: | ---: | ---: | ---: |
| `sola/rate-2ch-noise-0.05` | 35,431,855.75 | 34,036,054.00 | 1.041x | 1.026-1.058x |
| `sola/rate-2ch-noise-0.5` | 3,779,900.00 | 3,635,139.50 | 1.040x | 1.013-1.062x |
| `sola/rate-2ch-noise-2` | 905,163.00 | 873,280.75 | 1.037x | 0.975-1.106x |
| `sola/rate-2ch-noise-8` | 102,451.75 | 115,018.00 | 0.891x | 0.763-0.991x |
| `sola/rate-2ch-tone-0.5` | 518,349.25 | 521,422.25 | 0.994x | 0.969-1.054x |
| `sola/rate-6ch-noise-0.5` | 11,593,002.50 | 11,163,550.25 | 1.038x | 1.009-1.086x |

The main stereo noise fixture improves 1.032x and six-channel noise improves 1.066x. The additional noise-rate fixtures from 0.05x through 2x improve around 4%; the 8x short-clip fixture measures 0.891x throughput. Tone fixtures are approximately unchanged. Crossfades are slower than with pre-normalized f32 buffers. The retention reasons are halved source storage, faster measured input handling, and fewer bytes moved during compaction. Compaction was not timed separately. The complete CPU tradeoff is recorded rather than claiming a universal speedup.

**Correctness.** Original and retained binaries produce identical 6,714,176-byte captures, compared byte for byte. Captures contain every emitted f32 sample bit and pull boundary across 1/2/6/8 channels, 8/44.1/48/96 kHz, packet sizes 137/997/4096, pull limits 7/256/997, live rate changes 1.2/0.8/2.0/0.5, EOF drains, and resets. SHA-256 remains `60b5056470be9ea45ebddce67fcda5a7fe492a2e17fd3c19a78d4c1cacc044b4`. [Capture evidence](../testing/2026-10-26-audio-native-buffer-identity.json).

Tests compare scoring to the original normalized-float scalar oracle, including full-scale values and rounding/rejection boundaries. Every i16 value passes through every channel at widths 1/2/3/4/6/8/16, and the full PCM range is checked against the original floating crossfade formula. The input fixtures now use native PCM because the private storage/search interface has that type; the public interleaved-i16 input and planar-f32 output APIs retain their behavior. The retained crate passes 47 unit tests plus playback integration in both debug and release. Seven benchmark/capture tests remain ignored by the default suite; the new timed and capture tests were executed separately. Clippy performance checks and rustfmt pass. Metadata compilation passes for aarch64 Linux and i686 Windows, without execution on those targets. [Check logs and source hashes](../testing/2026-10-26-audio-native-buffer-checks.json).

**Exploration.** The [initial native-buffer comparison](2026-10-26-audio-native-buffer-trial.tsv) established exact output and storage savings. [AVX2 trials](2026-10-26-audio-native-avx2.tsv) reduced the conversion regression. A [safe bit-based conversion](2026-10-26-audio-native-bits.tsv) improved only the short crossfade fixture substantially and gave no sustained whole-stream gain, so it was reverted. Moving identity conversion under SIMD dispatch gave too little benefit ([main](2026-10-26-audio-native-identity.tsv), [rates](2026-10-26-audio-native-identity-rates.tsv)) and was also reverted. Intermediate [native source runs](2026-10-26-audio-native-buffer-retained.tsv), [test-complete runs](2026-10-26-audio-native-buffer-final.tsv), and [rate sweeps](2026-10-26-audio-native-rates.tsv) are retained as exploratory evidence; final claims use the two eight-process comparisons above.

The runner now freezes each source's typed test harness alongside the kernel source, so historical f32 and current i16 versions can both be rebuilt and verified. Use `python scripts/benchmark_audio_sola.py --build NAME`, optionally `--source PATH --test-source MATCHING_TEST_PATH`, then `--compare BEFORE AFTER --runs 8 --output PATH.tsv`. Add `--benchmark benchmark_sola_rates` for boundary-rate fixtures. `--verify BEFORE AFTER --output PATH.json` rebuilds frozen sources and tests and compares complete output bytes. Snapshots and binaries are under `target/audio-sola`.
