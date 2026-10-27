# Fixed-layout SOLA unpacking, 2026-10-26

SOLA now unpacks 6/8/16-channel PCM through fixed-width frame arrays. Bulk input throughput improves 1.49-1.55x, and 137/997-frame packet throughput improves 1.51-1.68x. Whole-stream results are mostly flat: six-channel noise improves 1.3% in both the main and slow-rate fixtures. Some unchanged mono/stereo controls are slower. All measurements and ranges are retained below.

**Execution simplified.** The measured layouts replace runtime `skip(index).step_by(channels)` traversal with `as_chunks::<CHANNELS>()` and one indexed sample per frame. The compiler sees a constant source stride. One generic helper produces the three direct layout kernels; they stay outside the packet-control function. The source adds twenty net production lines. No new unsafe code or intermediate sample buffer is introduced.

Each kernel reserves the same frame count for each channel, then visits frame blocks equivalent to the original 4096-sample blocks. A safe slice-to-array conversion checks the channel-state count once; every mapped frame has the selected width. Complete-frame and scalar-tail behavior stay the same, including release handling of incomplete source frames. Source cursor accounting and compaction remain in the caller. Other channel counts retain the generic path.

**Protocol.** Baseline source matches commit `a2e296304`. Separately frozen production sources, matching test harnesses, allocator counters, timing helpers, and rate fixtures compile with rustc 1.99.1 / LLVM 23.1.1, optimization level 3, fat LTO, one codegen unit, static CRT, and generic CPU settings. Host: Intel Xeon E5-2696 v4, Windows x86_64. Benchmark threads pin to logical CPU 2, warm once, then report the median of eleven QueryThreadCycleTime batches. Eight fresh processes per stage rotate execution order. Fixture construction stays outside timing, and vectors are reused. The retained root source/test hashes were checked before each final comparison; the two production source hashes are distinct and helper hashes match.

Bulk input appends a 12,001-frame packet. Whole-stream fixtures feed that clip in 997-frame packets and pull 256 output frames, including identical fingerprinting work. Packet-only fixtures time 137- and 997-frame pushes and hash the unpacked planar PCM before timing. These measure SOLA operations, not total application latency or FPS. Throughput is median-before divided by median-after; ranges include every process pair.

[Main raw data](2026-10-26-audio-fixed-layout.tsv), [source/compiler/binary manifests](2026-10-26-audio-fixed-layout.manifest.json).

| Workload | Before cycles/op | After cycles/op | Throughput | Paired range |
| --- | ---: | ---: | ---: | ---: |
| `sola/append-16ch` | 471,564.00 | 305,055.27 | 1.546x | 1.512-1.572x |
| `sola/append-6ch` | 162,890.42 | 109,240.69 | 1.491x | 1.415-1.509x |
| `sola/append-8ch` | 220,471.47 | 148,060.47 | 1.489x | 1.454-1.504x |
| `sola/crossfade-1440` | 701.80 | 676.40 | 1.038x | 0.973-1.135x |
| `sola/crossfade-256` | 156.37 | 156.37 | 1.000x | 0.888-1.142x |
| `sola/left-silent/360` | 93,812.22 | 93,657.93 | 1.002x | 0.863-1.055x |
| `sola/left-silent/720` | 349,756.11 | 356,889.84 | 0.980x | 0.954-1.035x |
| `sola/mono/360` | 88,575.10 | 88,955.81 | 0.996x | 0.977-1.020x |
| `sola/mono/720` | 359,986.86 | 361,729.16 | 0.995x | 0.983-1.018x |
| `sola/right-silent/360` | 88,369.33 | 89,226.76 | 0.990x | 0.917-1.014x |
| `sola/right-silent/720` | 355,710.07 | 358,049.05 | 0.993x | 0.954-1.034x |
| `sola/stereo/360` | 179,619.60 | 183,673.48 | 0.978x | 0.945-1.008x |
| `sola/stereo/720` | 710,981.06 | 716,142.75 | 0.993x | 0.957-1.013x |
| `sola/stream-1ch-noise-1.2` | 444,432.75 | 438,506.25 | 1.014x | 0.938-1.156x |
| `sola/stream-2ch-left-silent-1.2` | 597,314.50 | 592,759.75 | 1.008x | 0.947-1.090x |
| `sola/stream-2ch-noise-0.8` | 1,683,235.75 | 1,732,733.00 | 0.971x | 0.942-1.026x |
| `sola/stream-2ch-silence-1.2` | 108,103.75 | 108,103.75 | 1.000x | 0.792-1.014x |
| `sola/stream-2ch-tone-1.2` | 192,117.50 | 198,373.50 | 0.968x | 0.848-1.090x |
| `sola/stream-6ch-noise-1.2` | 2,809,216.25 | 2,774,479.75 | 1.013x | 0.976-1.060x |

**Packet sizes.** The same fixed-layout kernels improve both packet sizes; this is not limited to the large bulk fixture. [Raw data](2026-10-26-audio-fixed-layout-packets.tsv), [manifests](2026-10-26-audio-fixed-layout-packets.manifest.json).

| Workload | Before cycles/op | After cycles/op | Throughput | Paired range |
| --- | ---: | ---: | ---: | ---: |
| `sola/packet-16ch-137` | 5,392.32 | 3,350.38 | 1.609x | 1.596-1.718x |
| `sola/packet-16ch-997` | 37,932.34 | 24,444.67 | 1.552x | 1.527-1.591x |
| `sola/packet-6ch-137` | 2,050.10 | 1,245.40 | 1.646x | 1.433-2.082x |
| `sola/packet-6ch-997` | 13,707.60 | 9,076.24 | 1.510x | 1.411-1.537x |
| `sola/packet-8ch-137` | 2,806.34 | 1,673.26 | 1.677x | 1.484-1.724x |
| `sola/packet-8ch-997` | 18,187.21 | 12,068.21 | 1.507x | 1.455-1.569x |

**Whole-stream rates.** [Raw data](2026-10-26-audio-fixed-layout-rates.tsv), [manifests](2026-10-26-audio-fixed-layout-rates.manifest.json).

| Workload | Before cycles/op | After cycles/op | Throughput | Paired range |
| --- | ---: | ---: | ---: | ---: |
| `sola/rate-2ch-noise-0.05` | 26,678,084.75 | 26,672,980.00 | 1.000x | 0.916-1.013x |
| `sola/rate-2ch-noise-0.5` | 2,887,468.00 | 2,851,689.50 | 1.013x | 0.963-1.037x |
| `sola/rate-2ch-noise-2` | 691,644.50 | 675,127.25 | 1.024x | 0.973-1.066x |
| `sola/rate-2ch-noise-8` | 115,072.75 | 114,963.25 | 1.001x | 0.928-1.033x |
| `sola/rate-2ch-tone-0.5` | 465,230.25 | 460,182.00 | 1.011x | 0.721-1.066x |
| `sola/rate-6ch-noise-0.5` | 8,799,096.75 | 8,687,755.25 | 1.013x | 1.003-1.028x |

The robust retention benefit is faster unpacking in all three measured layouts and packet sizes. Search and crossfade algorithms are unchanged. Whole-stream CPU benefit is modest because correlation dominates these fixtures; the full table includes the roughly 3% slower stereo noise/tone controls in the main run. There is no claim of a universal end-to-end speedup.

**Allocations.** Every warmed main, packet, and rate probe reports zero allocations, reallocations, and allocated bytes. Constructor plus the bulk push retains the same allocation calls and bytes before/after: 6ch 8 calls / 150,012 bytes; 8ch 10 calls / 198,096 bytes; 16ch 18 calls / 390,432 bytes. The change improves CPU execution and leaves the native PCM storage footprint intact.

**Correctness.** Extended before/after captures are byte-identical: 12,478,424 bytes containing every emitted f32 bit and pull boundary. The matrix includes 1/2/5/6/8/16 channels, 8/44.1/48/96 kHz, packet sizes 137/997/4096, pull limits 7/256/997, live rates 1.2/0.8/2.0/0.5, EOF drains, and resets. Five channels exercise the generic fallback. SHA-256: `ee2fb635533d85a3baf45924f68a89d6eb5597a68b0bce5d9fdffdf0b23a4a4d`. [Capture evidence](../testing/2026-10-26-audio-fixed-layout-identity.json).

Every i16 value is checked in each channel at widths 1/2/3/4/5/6/7/8/9/12/16. A compact production-path test crosses the fixed-layout block boundary and repeats with retained buffers. The crate passes 48 unit tests plus playback integration in debug and release; eight benchmark/capture tests are ignored by the default suite, with relevant timed/capture tests invoked separately. Rustfmt, Clippy performance checks, and aarch64 Linux/i686 Windows metadata compilation pass. Those target checks compile metadata without executing target binaries. [Native check logs and source hashes](../testing/2026-10-26-audio-fixed-layout-checks.json). The compact block-boundary/reuse test is checked under Miri with tree borrows: [Miri evidence](../testing/2026-10-26-audio-fixed-layout-miri.json).

**Exploration.** Writing min/max PCM differences explicitly gave no sustained benefit ([data](2026-10-26-audio-search-minmax.tsv)); generated baseline assembly already uses signed vector min/max and subtraction. Checked SSE4.1 searches were flat, while AVX2 searches were slower ([CPU trials](2026-10-26-audio-search-cpu-trial.tsv)), so neither unsafe dispatch prototype was retained. A fully integer search and explicit-bound variant caused partial scalarization and regressed ([integer trial](2026-10-26-audio-search-pcm-trial.tsv), [bound trial](2026-10-26-audio-search-bounded-trial.tsv)); both were dropped.

The [first fixed-layout trial](2026-10-26-audio-append-fixed-trial.tsv) improved bulk input but regressed whole-stream work. The retained variant limits specialization to measured 6/8/16-channel layouts and keeps their loops out of the packet-control function ([comparison](2026-10-26-audio-append-outlined-trial.tsv), [packet trial](2026-10-26-audio-append-packets-trial.tsv)). Final claims use the three eight-process comparisons above. An attempted final snapshot had lost the worktree edits and measured the baseline twice; it was rejected, and final archives were rebuilt after staging and verifying the intended source hashes. That invalid comparison is not used as evidence.

Reproduce with `python scripts/benchmark_audio_sola.py --build NAME`, then `--compare layout-original layout-retained --runs 8 --output PATH.tsv`. Add `--benchmark benchmark_sola_packets` or `--benchmark benchmark_sola_rates` for the additional matrices. `--verify layout-original layout-retained --output PATH.json` rebuilds frozen sources/tests/helpers and compares complete output bytes. Archives remain under `target/audio-sola`.
