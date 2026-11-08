# Integer PCM correlation prefixes, 2026-10-26

SOLA correlation now accumulates the exact low-range prefix in integer PCM units, normalizing once when returning or entering the scalar floating-point remainder. Correlation search throughput improves 1.32-1.41x in the eight-process comparison. Whole-stream noise fixtures improve 1.27-1.33x in the main comparison; the separate rate sweep records all six results below. Warmed allocator activity remains zero.

**Execution simplified.** Previously every eight-sample group converted its integer sum to f32, normalized it, added to a floating score, checked the 496-unit limit, and checked the rejection threshold. The new loop carries a u32 sum and checks one combined integer stopping threshold. It removes repeated conversion, normalization, floating addition, and separate threshold checks from the exact-range block loop. Only the existing high-range remainder uses per-sample floating accumulation. The production change adds nine net lines to express these two arithmetic phases. There is no new unsafe code; crossfade dispatch and its unsafe feature boundaries are unchanged.

**Exactness and bounds.** Source samples are i16. A normalized absolute difference is exactly its u16 PCM difference multiplied by 2^-15. Existing caller thresholds come from those sums, so every finite threshold is also an integer multiple of 2^-15. Converting that threshold to PCM units preserves comparisons while the candidate prefix remains below 512 normalized units. Infinity and larger thresholds saturate to u32::MAX, which cannot be reached in this phase.

Before each grouped addition the sum is at most 496 * 32768 = 16,252,928. Eight differences add at most 524,280, making the largest grouped result 16,777,208, below 2^24. Thus neither integer overflow nor f32 rounding occurs during conversion. The combined stop is the smaller of the candidate threshold and 496 * 32768 + 1. Rejection occurs at the same eight-sample boundary as before. A prefix equal to 496 still permits one more exact group; a prefix above it resumes the original per-sample f32 addition order, including scalar tails and early rejection. Searches still use strict comparison and preserve earliest ties. No approximate score or changed search range is introduced.

**Protocol.** The baseline source matches commit `21f230da6`. Archived source, typed test harness, allocator counter, timing helper, and rate fixture snapshots compile with rustc 1.99.1 / LLVM 23.1.1, optimization level 3, fat LTO, one codegen unit, static CRT, and generic CPU settings. Host: Intel Xeon E5-2696 v4, Windows x86_64. Each process pins its benchmark thread to logical CPU 2, warms once, then reports the median of eleven CPU-cycle batches using QueryThreadCycleTime. Eight fresh processes per stage rotate execution order. Fixtures and buffers are prepared before timing. Whole-stream fixtures feed 12,001 frames in 997-frame packets, pull 256 frames, and include identical output fingerprinting. These are SOLA fixture measurements, not total application latency or FPS. Miri validation ran concurrently on this shared host; repeated rotating process runs and the complete paired ranges expose host variation. Crossfades and input handling serve as unchanged controls.

[Main measurements](2026-10-26-audio-integer-score-frozen.tsv), [source/compiler/binary manifests](2026-10-26-audio-integer-score-frozen.manifest.json). Throughput is median-before divided by median-after. Paired ranges include all eight process pairs. Search labels specify correlation length, with length + 1 candidate offsets.

| Workload | Before cycles/op | After cycles/op | Throughput | Paired range |
| --- | ---: | ---: | ---: | ---: |
| `sola/append-16ch` | 460,726.19 | 457,358.28 | 1.007x | 0.964-1.034x |
| `sola/append-6ch` | 161,341.06 | 160,337.04 | 1.006x | 0.948-1.023x |
| `sola/append-8ch` | 218,630.58 | 216,551.33 | 1.010x | 0.966-1.024x |
| `sola/crossfade-1440` | 669.00 | 676.18 | 0.989x | 0.945-1.793x |
| `sola/crossfade-256` | 143.94 | 168.70 | 0.853x | 0.795-1.096x |
| `sola/left-silent/360` | 118,529.99 | 89,768.64 | 1.320x | 1.211-1.372x |
| `sola/left-silent/720` | 471,616.33 | 349,601.77 | 1.349x | 1.312-1.372x |
| `sola/mono/360` | 117,494.22 | 86,843.11 | 1.353x | 1.321-1.404x |
| `sola/mono/720` | 479,974.52 | 346,357.27 | 1.386x | 1.299-1.425x |
| `sola/right-silent/360` | 116,921.47 | 88,269.86 | 1.325x | 1.232-1.405x |
| `sola/right-silent/720` | 478,630.02 | 339,473.92 | 1.410x | 1.354-1.414x |
| `sola/stereo/360` | 236,350.04 | 178,086.53 | 1.327x | 1.248-1.359x |
| `sola/stereo/720` | 952,945.50 | 697,440.71 | 1.366x | 1.292-1.403x |
| `sola/stream-1ch-noise-1.2` | 573,828.25 | 446,737.25 | 1.284x | 1.243-1.324x |
| `sola/stream-2ch-left-silent-1.2` | 748,220.50 | 590,674.50 | 1.267x | 1.139-1.301x |
| `sola/stream-2ch-noise-0.8` | 2,171,074.50 | 1,636,866.50 | 1.326x | 1.288-1.348x |
| `sola/stream-2ch-silence-1.2` | 108,762.25 | 108,048.75 | 1.007x | 0.921-1.124x |
| `sola/stream-2ch-tone-1.2` | 219,829.25 | 195,574.50 | 1.124x | 1.118-1.193x |
| `sola/stream-6ch-noise-1.2` | 3,488,184.25 | 2,750,335.00 | 1.268x | 1.224-1.305x |

**Rate sweep.** Identical whole-stream fixtures at slow, fast, and boundary speeds. [Measurements](2026-10-26-audio-integer-score-frozen-rates.tsv), [manifests](2026-10-26-audio-integer-score-frozen-rates.manifest.json).

| Workload | Before cycles/op | After cycles/op | Throughput | Paired range |
| --- | ---: | ---: | ---: | ---: |
| `sola/rate-2ch-noise-0.05` | 33,873,020.00 | 26,196,118.25 | 1.293x | 1.277-1.329x |
| `sola/rate-2ch-noise-0.5` | 3,648,309.75 | 2,829,574.50 | 1.289x | 1.243-1.409x |
| `sola/rate-2ch-noise-2` | 870,372.00 | 683,413.25 | 1.274x | 1.212-1.587x |
| `sola/rate-2ch-noise-8` | 115,347.25 | 114,908.00 | 1.004x | 0.840-1.117x |
| `sola/rate-2ch-tone-0.5` | 505,288.75 | 440,810.75 | 1.146x | 1.078-1.231x |
| `sola/rate-6ch-noise-0.5` | 11,009,461.50 | 8,485,046.75 | 1.298x | 1.225-1.341x |

Every main search and noise-stream process pair is faster. Unchanged controls have mixed medians and sometimes wide paired variation; the tables retain those ranges. The 8x short-clip and silence fixtures spend little time searching and change only slightly. Warmed main and rate probes all report zero allocations, reallocations, and allocated bytes. Source storage, allocation counts, output layout, and input conversion are unchanged in this iteration.

**Correctness.** The retained and original binaries produce identical 6,714,176-byte captures containing every output f32 bit and pull boundary across channel counts 1/2/6/8, sample rates 8/44.1/48/96 kHz, packet sizes 137/997/4096, pull limits 7/256/997, live rate changes, EOF drains, and resets. SHA-256: `60b5056470be9ea45ebddce67fcda5a7fe492a2e17fd3c19a78d4c1cacc044b4`. [Full-byte comparison](../testing/2026-10-26-audio-integer-score-identity.json).

The independent scalar oracle normalizes each PCM input before subtracting and adding in the original order. Expanded score checks cover empty input, short tails, the exact 496-unit transition, thresholds one PCM quantum apart, values crossing 512, full-scale PCM, low-amplitude PCM, and very large/infinite limits. Search tests also compare selected mono/stereo offsets to the scalar oracle. The crate passes 47 unit tests plus playback integration in both debug and release; seven benchmark/capture tests remain ignored by the default suite and the relevant ones were invoked separately. Native crate tests, playback integration, Clippy performance checks, formatting, and aarch64 Linux/i686 Windows metadata checks are recorded in [check logs](../testing/2026-10-26-audio-integer-score-checks.json). A compact matrix covering empty/short tails, exact 496-unit transitions, scalar rounding above 512, and rejection limits passes Miri with tree borrows. It appends a test to the archived production source after relocating the test include, with source hashes checked: [Miri evidence](../testing/2026-10-26-audio-integer-score-miri-targeted.json). The oversized full interpreted stress matrix was [intentionally cancelled](../testing/2026-10-26-audio-integer-score-miri.json) after that targeted matrix passed; the complete stress matrix passes native debug/release tests. Cross-target checks compile metadata only and do not execute those targets.

**Rejected experiments.** Applying normalization after crossfade interpolation, including an integer-difference variant, did not give a consistent overall benefit, so the original interpolation formula remains. [Initial comparison](2026-10-26-audio-crossfade-scale-trial.tsv), [integer-difference comparison](2026-10-26-audio-crossfade-scale-integer.tsv). Integer prefix accumulation was measured first with separate stopping checks ([trial](2026-10-26-audio-integer-score-trial.tsv)), then with the combined limit ([comparison](2026-10-26-audio-integer-score-limit.tsv)). An earlier [main comparison](2026-10-26-audio-integer-score.tsv) and [rate sweep](2026-10-26-audio-integer-score-rates.tsv) predated frozen support files. File timestamps showed that the shared allocator helper changed between those builds, so both stages were rebuilt with identical helper snapshots. Final claims use only the frozen eight-process comparisons above. The runner now snapshots and hashes these dependencies, reuses them when verifying archived sources, and refuses comparisons with missing or mismatched support hashes.

Reproduce with `python scripts/benchmark_audio_sola.py --build NAME`, `--compare score-frozen-before score-frozen-final --runs 8 --output PATH.tsv`, and `--benchmark benchmark_sola_rates` for the rate sweep. `--verify score-frozen-before score-frozen-final --output PATH.json` rebuilds the frozen source and its matching tests before comparing every output byte. Archives remain under `target/audio-sola`.
