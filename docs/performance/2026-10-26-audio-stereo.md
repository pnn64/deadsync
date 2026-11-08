# Independent stereo correlation, 2026-10-26

Stereo SOLA still used duplicated serial scoring loops after the mono scorer gained exact grouped PCM arithmetic. Stereo now calls the existing mono search for each channel. Equal dual-mono search buffers and patterns reuse the left result. This removes the fused scorer, its two activity flags, the shared candidate traversal, and duplicated tie/zero handling: **119 production lines removed, seven added, 112 net removed**. There are no new allocations, dependencies, caches, or unsafe operations.

The original fused traversal minimized each channel's score independently. Applying the same minimum search to each channel therefore selects the same pair of offsets. The reused scorer preserves every PCM score, including large sums that retain scalar addition order, and its strict comparison preserves earliest ties. Dual-mono equality leaves NaNs on the independent path; signed zeros have equal absolute-difference scores. The stereo unit test now checks the brute-force scalar oracle rather than calling the same mono implementation as its expected result.

The baseline production source matches commit `12175488e`. This builds on the [previous three optimizations](2026-10-26-audio-sola.md).

**Measurements.** Intel Xeon E5-2696 v4, Windows x86_64, rustc 1.99.1 / LLVM 23.1.1. Both production sources use identical optimization level 3, fat LTO, one codegen unit, static CRT, and generic CPU flags. The test thread is pinned to logical CPU 2. Each process has one warmup and eleven measured batches; eight fresh processes per version alternate execution order. Medians use scheduled thread CPU cycles from `QueryThreadCycleTime`; wall times are retained in the raw data. Fixture construction stays outside timing, and streaming buffers are warmed. All **304 workload measurements report zero warmed allocations, reallocations, or allocated bytes**.

Search labels give the correlation length; the candidate count is length + 1. Complete streams feed 12,001 source frames in 997-frame packets, pull at most 256 frames, and include PCM fingerprinting. Append and crossfade cases are unchanged controls. CPU throughput is median-before / median-after. Paired ranges include all eight process pairs.

| Workload | Before cycles/op | After cycles/op | CPU throughput | Paired range |
| --- | ---: | ---: | ---: | ---: |
| `sola/append-16ch` | 588,396.33 | 569,742.27 | 1.033x | 0.979-1.117x |
| `sola/append-6ch` | 207,841.62 | 196,599.98 | 1.057x | 0.994-1.134x |
| `sola/append-8ch` | 278,935.62 | 270,496.00 | 1.031x | 0.875-1.067x |
| `sola/crossfade-1440` | 391.20 | 387.24 | 1.010x | 0.875-1.077x |
| `sola/crossfade-256` | 79.20 | 91.75 | 0.863x | 0.758-0.989x |
| `sola/left-silent/360` | 312,664.08 | 118,159.58 | 2.646x | 2.342-3.277x |
| `sola/left-silent/720` | 1,254,524.81 | 462,589.38 | 2.712x | 2.442-3.050x |
| `sola/mono/360` | 116,118.94 | 111,643.18 | 1.040x | 0.989-1.161x |
| `sola/mono/720` | 472,672.64 | 465,950.52 | 1.014x | 0.915-1.098x |
| `sola/right-silent/360` | 294,678.77 | 114,386.93 | 2.576x | 2.203-2.781x |
| `sola/right-silent/720` | 1,252,456.72 | 469,901.52 | 2.665x | 2.270-2.771x |
| `sola/stereo/360` | 332,041.73 | 229,861.08 | 1.445x | 1.345-1.734x |
| `sola/stereo/720` | 1,350,480.69 | 889,880.42 | 1.518x | 1.362-1.596x |
| `sola/stream-1ch-noise-1.2` | 587,491.75 | 584,912.50 | 1.004x | 0.957-1.150x |
| `sola/stream-2ch-left-silent-1.2` | 1,793,589.50 | 743,007.50 | 2.414x | 2.120-2.593x |
| `sola/stream-2ch-noise-0.8` | 3,059,226.25 | 2,136,503.00 | 1.432x | 1.312-1.524x |
| `sola/stream-2ch-silence-1.2` | 104,756.50 | 110,737.75 | 0.946x | 0.868-1.128x |
| `sola/stream-2ch-tone-1.2` | 219,061.00 | 213,189.25 | 1.028x | 0.882-1.135x |
| `sola/stream-6ch-noise-1.2` | 3,628,170.25 | 3,513,207.25 | 1.033x | 0.878-1.083x |

Ordinary stereo searches improve 1.445-1.518x; searches with one silent channel improve 2.576-2.712x. Complete stereo noise and left-silent streams improve 1.432x and 2.414x. The complete silence fixture measures 0.946x, and the unchanged 256-sample crossfade control measures 0.863x. Unchanged controls vary between builds; these data do not identify the cause. Full [raw measurements](2026-10-26-audio-stereo-retained.tsv) and [compiler/source/binary manifests](2026-10-26-audio-stereo-retained.manifest.json) preserve every workload.

An [initial independent-search run](2026-10-26-audio-stereo-independent.tsv) gave comparable stereo search gains, before strengthening the test oracle. A later [first-candidate equality shortcut](2026-10-26-audio-stereo-first-match.tsv) improved the silence fixture only 1.025x while adding a branch and comparison; it was reverted. The retained implementation is the smaller independent-search version.

**Correctness.** Original and retained sources produce identical 6,714,176-byte output captures, compared byte for byte. The captures contain every f32 sample bit and each pull boundary across 1/2/6/8 channels, 8/44.1/48/96 kHz, packet sizes 137/997/4096, pull limits 7/256/997, live rate changes 1.2/0.8/2.0/0.5, EOF drains, and resets. SHA-256 remains `60b5056470be9ea45ebddce67fcda5a7fe492a2e17fd3c19a78d4c1cacc044b4`. [Capture evidence](../testing/2026-10-26-audio-stereo-identity.json).

All 44 audio unit tests and the playback integration test pass. Four benchmark/capture tests remain ignored by the default suite; the new benchmark and capture tests run separately. Clippy performance checks and rustfmt pass. Metadata compilation passes for aarch64 Linux and i686 Windows; these targets were not executed. [Commands, output, and current source hashes](../testing/2026-10-26-audio-stereo-checks.json). The existing unused `is_direct` warning in the old processing test baseline remains.

Reproduce with `python scripts/benchmark_audio_sola.py --build NAME --source PATH`, then `--compare BEFORE AFTER --runs 8 --output PATH.tsv`. `--verify BEFORE AFTER --output PATH.json` rebuilds archived sources and compares full PCM bytes and pull boundaries. Archives live under `target/audio-sola`; source files exported from Git receive the same test include automatically when needed.
