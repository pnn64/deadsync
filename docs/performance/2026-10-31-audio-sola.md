# SOLA search and source-buffer optimization, 2026-10-31

Base: `4c32f54405ca0d4267213a425dbfd34da99ac154`. Production changes are confined to `deadlib-audio/src/stream/stretch.rs`. The other changes are benchmarks, correctness checks, and their runner.

## Changes

1. **Return directly for an exact first candidate.** Shared source windows use pointer identity; separately allocated equal PCM uses slice equality. This replaces a normalized difference reduction for the initial window and silent regions. All other candidates retain the same search range and earliest-tie selection.

2. **Reclaim source history when a push needs its storage.** Window retirement now only advances cursors. A push that would exceed capacity compacts the live PCM once, before reserving. This removes the periodic compaction threshold, its field, `compact`, `compact_storage`, `compact_buffer`, and `prefix_can_satisfy_reserve`. It also avoids reserving space for dead history when a larger packet needs growth. Retained Vec capacity continues to bound storage.

3. **Compare bounded searches entirely in integer PCM.** If candidate zero's float score is at most 512, all possible improving candidates have exact integer sums. The search compares u32 scores directly and reduces 32 samples per block, allowing wider SIMD reductions and fewer horizontal sums and branches. Searches starting above that limit retain the original grouped scorer and its ordered float rounding.

There is no new unsafe code or production dependency. The existing crossfade feature-dispatch safety boundaries remain. The production diff adds one net line, including comments (69 inserted, 68 deleted); the removed buffer helpers and field offset the two specialized integer routines. An attempted signed min/max rewrite was dropped after assembly inspection showed the same SIMD instructions as `i16::abs_diff`.

## Exactness

An i16 absolute difference is an integer PCM quantum scaled by 2^-15. Every nonnegative cumulative sum up to 2^24 quanta (512 normalized units) is exactly representable in f32. Any candidate whose float score improves a best score at or below 512 must stay in that exact range. A larger sum reaches at least 512 and cannot subsequently decrease, so it cannot win. The integer path therefore preserves the selected offset, including ties, even if a rejected candidate would round above that boundary. Rejection may occur after a different block, but its partial score is private and discarded.

The integer accumulator is below its rejection threshold, at most 2^24, before each block. Adding at most 32 * 65,535 cannot overflow u32. Final tails contain at most 31 samples. The original float scorer remains unchanged for the case where rounded scores can determine the winner. Source-buffer reclamation moves only the live suffix and preserves all logical positions relative to `data_start`.

## Incremental measurements

Each row compares the stage immediately before and after its change. Values are medians across six process runs. Throughput is before/after cycles; ranges include every paired process ratio. All measured warmed operations allocate and reallocate zero times.

| Change / workload | Before cycles/op | After cycles/op | CPU throughput | Paired range |
| --- | ---: | ---: | ---: | ---: |
| `sola/first-same-source-360` | 187.05 | 11.93 | 15.679x | 14.153-18.461x |
| `sola/first-same-source-720` | 326.66 | 11.93 | 27.382x | 27.276-45.620x |
| `sola/first-silence-360` | 178.80 | 81.34 | 2.198x | 1.228-2.811x |
| `sola/first-silence-720` | 340.69 | 133.09 | 2.560x | 2.466-2.893x |
| `sola/retire-1ch-65536` | 27,592.25 | 8,717.39 | 3.165x | 3.044-3.261x |
| `sola/retire-2ch-65536` | 90,413.01 | 52,720.07 | 1.715x | 1.516-1.854x |
| `sola/retire-6ch-65536` | 731,997.29 | 377,077.45 | 1.941x | 1.725-2.072x |
| `sola/mono/360` | 62,661.05 | 35,558.77 | 1.762x | 1.523-2.176x |
| `sola/stereo/720` | 784,391.87 | 527,366.91 | 1.487x | 1.361-1.754x |
| `sola/stream-2ch-noise-0.8` | 1,788,864.50 | 958,914.23 | 1.866x | 1.405-2.962x |
| `sola/stream-6ch-noise-1.2` | 2,913,574.25 | 1,765,624.80 | 1.650x | 1.268-2.697x |

## Protocol and limits

Windows x86_64, Intel Core i7-1250U, rustc 1.97.1 / LLVM 22.1.6. Each frozen production source compiles with optimization level 3, fat LTO, one codegen unit, a static CRT, and generic CPU settings. The thread is pinned to logical CPU 2. `QueryThreadCycleTime` measures scheduled thread cycles; these are Windows cycle-accounting measurements, not PMU instruction counts. Each process warms once and reports the median of eleven batches. Six fresh processes per stage rotate order across all four stages. No builds overlap the final measurements. Inputs and retained output buffers are prepared outside timing; warmed reset/push/pull work is included where shown. The final batches are longer than the initial screening runs to reduce short-batch variation.

The stages are `oct31-before`, `oct31-identity`, `oct31-capacity`, and `oct31-final`. Source, fixture, support, compiler, and executable hashes are retained in each TSV's matching manifest. The runner checks identical benchmark fixtures/support and executable integrity before comparison. The baseline source was checked against the base commit after newline normalization; the final archive matches the working source bytes.

`NA` marks optional fields that the workload does not measure, such as initial storage in a query-only benchmark.

First-candidate cases isolate a shared window, separately allocated silence, and a nonmatching noise control at 360/720 samples. Retirement cases include reset, refilling one retained source packet, deinterleaving, and retiring its 1,440-frame windows down to the final partial window. The 65,536-frame cases stress large pending source buffers; these gains do not describe typical 997-frame packets or whole-player speed. Main stream cases feed 12,001 source frames in 997-frame packets, pull 256 frames at a time, and hash the output. Hashing remains inside both variants. The rate sweep covers 0.05x, 0.5x, 2x, and 8x.

The full tables below retain unchanged input/crossfade controls and smaller-buffer cases, including ratios below one. The 12,001-frame stereo retirement fixture uses 5.1% more cycles in the final median; whole-stream silence is effectively unchanged. Several search controls vary substantially across intermediate stages even where their search code is unchanged. These measurements do not distinguish host variation from code-generation effects. No across-the-board gain, whole-game FPS gain, or reduction in warmed allocation counts is claimed.

For the complete 12,001-frame noise streams, baseline-to-final CPU throughput improves 1.442-1.655x. Stereo at 0.8x processes 7,561 -> 12,515 source frames per million CPU cycles; six-channel audio at 1.2x processes 4,713 -> 6,797. These figures include output hashing.

## First results

[Raw measurements](2026-10-31-audio-sola-first.tsv), [source/compiler/binary manifests](2026-10-31-audio-sola-first.manifest.json). Cycles per operation; the last two columns compare baseline to final.

| Workload | Before | Identity | Capacity | Final | CPU throughput | Paired range |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `sola/first-noise-360` | 75,145.47 | 68,761.37 | 75,415.27 | 39,420.92 | 1.906x | 1.593-2.281x |
| `sola/first-noise-720` | 282,680.80 | 271,350.94 | 252,536.09 | 171,356.11 | 1.650x | 1.165-1.704x |
| `sola/first-same-source-360` | 187.05 | 11.93 | 12.13 | 10.07 | 18.566x | 16.090-20.658x |
| `sola/first-same-source-720` | 326.66 | 11.93 | 10.30 | 9.28 | 35.220x | 29.744-63.432x |
| `sola/first-silence-360` | 178.80 | 81.34 | 76.73 | 77.28 | 2.314x | 1.994-2.707x |
| `sola/first-silence-720` | 340.69 | 133.09 | 163.60 | 155.14 | 2.196x | 2.068-2.631x |

## Retire results

[Raw measurements](2026-10-31-audio-sola-retire.tsv), [source/compiler/binary manifests](2026-10-31-audio-sola-retire.manifest.json). Cycles per operation; the last two columns compare baseline to final.

| Workload | Before | Identity | Capacity | Final | CPU throughput | Paired range |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `sola/retire-1ch-12001` | 892.58 | 876.67 | 787.44 | 808.48 | 1.104x | 1.042-1.304x |
| `sola/retire-1ch-65536` | 27,527.65 | 27,592.25 | 8,717.39 | 9,037.83 | 3.046x | 2.739-3.778x |
| `sola/retire-2ch-12001` | 9,219.32 | 9,211.42 | 10,345.03 | 9,689.81 | 0.951x | 0.790-1.239x |
| `sola/retire-2ch-65536` | 90,160.85 | 90,413.01 | 52,720.07 | 50,287.54 | 1.793x | 1.476-1.944x |
| `sola/retire-6ch-12001` | 64,444.81 | 64,919.54 | 64,334.62 | 64,316.89 | 1.002x | 0.880-1.048x |
| `sola/retire-6ch-65536` | 736,162.85 | 731,997.29 | 377,077.45 | 358,504.28 | 2.053x | 1.813-2.098x |

## Kernels results

[Raw measurements](2026-10-31-audio-sola-kernels.tsv), [source/compiler/binary manifests](2026-10-31-audio-sola-kernels.manifest.json). Cycles per operation; the last two columns compare baseline to final.

| Workload | Before | Identity | Capacity | Final | CPU throughput | Paired range |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `sola/append-16ch` | 176,495.21 | 177,841.56 | 175,489.15 | 174,654.19 | 1.011x | 0.858-1.316x |
| `sola/append-6ch` | 64,082.84 | 64,246.50 | 63,976.98 | 64,327.64 | 0.996x | 0.762-1.401x |
| `sola/append-8ch` | 85,744.40 | 85,038.92 | 84,947.49 | 85,006.29 | 1.009x | 0.761-1.370x |
| `sola/crossfade-1440` | 462.58 | 448.85 | 470.70 | 445.22 | 1.039x | 0.865-1.548x |
| `sola/crossfade-256` | 99.06 | 93.00 | 93.12 | 90.88 | 1.090x | 0.900-2.116x |
| `sola/left-silent/360` | 66,214.43 | 65,529.62 | 64,808.80 | 38,298.69 | 1.729x | 1.178-4.263x |
| `sola/left-silent/720` | 321,802.88 | 271,737.27 | 390,664.23 | 258,661.41 | 1.244x | 0.744-1.820x |
| `sola/mono/360` | 65,331.87 | 66,295.66 | 62,661.05 | 35,558.77 | 1.837x | 1.538-2.371x |
| `sola/mono/720` | 322,037.39 | 307,350.09 | 370,140.14 | 179,540.23 | 1.794x | 1.085-2.349x |
| `sola/right-silent/360` | 64,631.63 | 69,281.13 | 62,989.66 | 41,875.97 | 1.543x | 1.164-2.940x |
| `sola/right-silent/720` | 343,207.69 | 267,481.09 | 392,280.48 | 214,215.71 | 1.602x | 1.422-1.750x |
| `sola/stereo/360` | 138,442.42 | 131,926.37 | 131,205.17 | 73,718.52 | 1.878x | 1.616-3.521x |
| `sola/stereo/720` | 652,297.53 | 522,696.46 | 784,391.87 | 527,366.91 | 1.237x | 0.885-1.800x |
| `sola/stream-1ch-noise-1.2` | 410,514.19 | 317,358.14 | 466,388.97 | 253,081.95 | 1.622x | 1.574-1.766x |
| `sola/stream-2ch-left-silent-1.2` | 562,615.12 | 484,447.41 | 652,304.09 | 377,388.05 | 1.491x | 1.397-1.824x |
| `sola/stream-2ch-noise-0.8` | 1,587,258.14 | 1,347,556.06 | 1,788,864.50 | 958,914.23 | 1.655x | 1.618-1.831x |
| `sola/stream-2ch-silence-1.2` | 118,698.32 | 99,266.68 | 144,067.11 | 118,792.43 | 0.999x | 0.923-1.213x |
| `sola/stream-2ch-tone-1.2` | 190,631.89 | 170,900.29 | 210,134.10 | 170,344.53 | 1.119x | 1.097-1.242x |
| `sola/stream-6ch-noise-1.2` | 2,546,610.65 | 2,121,528.56 | 2,913,574.25 | 1,765,624.80 | 1.442x | 1.291-1.583x |

## Rates results

[Raw measurements](2026-10-31-audio-sola-rates.tsv), [source/compiler/binary manifests](2026-10-31-audio-sola-rates.manifest.json). Cycles per operation; the last two columns compare baseline to final.

| Workload | Before | Identity | Capacity | Final | CPU throughput | Paired range |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `sola/rate-2ch-noise-0.05` | 22,217,338.75 | 22,535,820.00 | 23,545,514.25 | 13,964,721.50 | 1.591x | 1.462-1.918x |
| `sola/rate-2ch-noise-0.5` | 2,346,995.50 | 2,763,435.00 | 2,684,954.75 | 1,439,259.25 | 1.631x | 1.528-2.328x |
| `sola/rate-2ch-noise-2` | 567,705.50 | 565,117.25 | 653,665.50 | 365,814.00 | 1.552x | 1.065-2.244x |
| `sola/rate-2ch-noise-8` | 116,684.00 | 116,684.00 | 124,023.75 | 116,589.75 | 1.001x | 0.881-1.163x |
| `sola/rate-2ch-tone-0.5` | 387,645.00 | 389,150.50 | 405,241.25 | 334,854.75 | 1.158x | 1.138-1.641x |
| `sola/rate-6ch-noise-0.5` | 7,779,717.75 | 7,146,566.00 | 7,722,692.50 | 4,804,605.00 | 1.619x | 1.116-2.342x |

## Correctness and checks

All four measured executables emit identical **31,546,568-byte** captures containing every output f32 bit and pull boundary. SHA-256: `ead2ca10d21ef08fb1a5d6c654e5ffb8af6d416929a81877ddb9cb65db837913`. The matrix covers 1/2/5/6/8/16 channels, 8/44.1/48/96 kHz, source lengths 12,001/65,536, packets 137/997/4,096/65,536, pull limits 7/256/997, live rate changes, EOF drains, and repeated resets. [Full-byte comparison](../testing/2026-10-31-audio-sola-identity.json).

Each frozen stage passes 28 standalone SOLA tests, including the independent scalar oracle, full-scale and low-amplitude samples, 496/512-unit boundaries, scalar tails, earliest ties, independent stereo channels, and retirement/refill checks with changing packet sizes. The release crate suite passes 50 unit tests (baseline: 48); the debug suite also passes all 50 and the playback integration test covering cuts, fades, channels, timing, live controls, and loops. Ten benchmark/capture tests are ignored in normal crate runs; the four relevant measurement suites and the capture test were run explicitly.

Clippy with `-D clippy::perf`, targeted rustfmt checks, Python syntax compilation, source/binary hash audits, and `git diff --check` pass. Clippy reports existing non-performance warnings in dependencies and an unused method in the prior processing oracle. [Check commands and output](../testing/2026-10-31-audio-sola-checks.json). All 888 timed rows report zero warmed allocations, reallocations, and requested bytes. Initial retained-source storage is also recorded in the kernel TSV.

## Reproduction

Use `py -3` on this Windows host, or a compatible Python executable elsewhere. Build the baseline from the base commit's exported `stretch.rs`, passing the current `crates/deadlib-audio/src/stream/stretch/perf_tests.rs` as `--test-source`. A source export in its own directory uses the current rate fixtures and support unless matching frozen files are present beside it.

```text
py -3 scripts/benchmark_audio_sola.py --build before --source PATH_TO_BASE_STRETCH --test-source crates/deadlib-audio/src/stream/stretch/perf_tests.rs
py -3 scripts/benchmark_audio_sola.py --build after
py -3 scripts/benchmark_audio_sola.py --compare before after --runs 6 --output PATH.tsv
py -3 scripts/benchmark_audio_sola.py --compare before after --benchmark benchmark_sola_first_match --runs 6 --output FIRST.tsv
py -3 scripts/benchmark_audio_sola.py --compare before after --benchmark benchmark_sola_retire --runs 6 --output RETIRE.tsv
py -3 scripts/benchmark_audio_sola.py --compare before after --benchmark benchmark_sola_rates --runs 6 --output RATES.tsv
py -3 scripts/benchmark_audio_sola.py --verify before after --output IDENTITY.json
```

`--verify` checks and executes the same archived binaries that were measured, comparing every capture byte. Archives remain under `target/audio-sola`; committed TSV/JSON evidence is independent of that ignored directory. The runner's newline writes also support the installed Python 3.9.
