# SOLA audio kernels, 2026-10-26

Three changes in `deadlib-audio` reduce surround unpacking work, crossfade cost, and mono/dual-mono/surround correlation cost. All measurements below use the production source, including the original source in the baseline, compiled with the same benchmark harness.

1. Surround input now extends each contiguous planar channel from bounded strided source blocks. This removes the per-frame traversal of channel states and per-sample `Vec::push` checks. Reserving the whole incoming packet preserves startup allocation behavior. Blocks target 4096 i16 samples (8 KiB), rounded to complete frames with at least one frame, keeping repeated source passes local even for wide packets; the initial unblocked version lost throughput at 16 channels.
2. Crossfades use the same `mul_add` expression in a runtime-selected FMA function on supported x86 CPUs. The ordinary implementation handles other CPUs, and identical source cursors retain their existing copy path. The substantial measured gain justifies one guarded unsafe call. The specialized function contains only safe slice/Vec operations; its sole safety precondition is CPU/OS support, established by Rust's feature detector. Both branches passed Miri with tree borrows.
3. Mono correlation groups eight absolute differences while the accumulated score is at most 496. Source buffers contain only i16 samples divided by 32768, so each difference is an exact multiple of 2^-15 below 2. Every intermediate sum through 512 is exactly representable in f32. An eight-sample block adds less than 16, making the grouped result bit-identical in this range. Larger prefixes retain the original scalar addition order. Rejection still occurs at the same eight-sample boundaries, preserving ties and chosen offsets. No floating-point fast-math flag is used.

The production diff adds 51 lines and removes 17, excluding the test-module include. The removed execution work is the nested per-sample push machinery and the serial dependency between eight exact correlation additions. Two small crossfade helpers allow one expression to compile for both CPU feature sets. Buffer ownership, channel storage, and startup allocations remain the same.

**Measurement protocol.** Intel Xeon E5-2696 v4, Windows x86_64; rustc 1.99.1 / LLVM 23.1.1. Flags: optimization level 3, fat LTO, one codegen unit, static CRT, generic CPU target. Each process pins its test thread to logical CPU 2. One warmup and eleven measured batches produce a median using Windows `QueryThreadCycleTime`; eight fresh processes per stage rotate stage order. Fixture construction is outside the cycle batches, and stream buffers are warmed before measurement. This counts scheduled thread cycles, not PMU instructions. The 19 workloads also record wall time and scoped Rust allocator activity. All 608 retained measurements report zero warmed allocations, reallocations, or allocated bytes. These are kernel and decoder-fixture results.

**Consecutive changes.** `before` is the original implementation; `unpack` adds bounded channel unpacking; `crossfade` adds FMA dispatch; `retained` adds exact grouped correlation. Ratios are median-before / median-after; paired ranges include every process pair.

| Change and workload | Before cycles/op | After cycles/op | CPU throughput | Paired range |
| --- | ---: | ---: | ---: | ---: |
| `before -> unpack`, `sola/append-6ch` | 238,541.62 | 205,569.45 | 1.160x | 1.105-1.270x |
| `before -> unpack`, `sola/append-8ch` | 308,331.47 | 277,217.36 | 1.112x | 1.065-1.255x |
| `before -> unpack`, `sola/append-16ch` | 601,506.31 | 573,522.62 | 1.049x | 0.987-1.152x |
| `unpack -> crossfade`, `sola/crossfade-256` | 877.25 | 71.06 | 12.345x | 11.566-13.152x |
| `unpack -> crossfade`, `sola/crossfade-1440` | 5,075.08 | 379.83 | 13.361x | 12.395-16.342x |
| `crossfade -> retained`, `sola/mono/360` | 297,100.11 | 114,359.50 | 2.598x | 2.140-2.773x |
| `crossfade -> retained`, `sola/mono/720` | 1,207,387.16 | 461,536.47 | 2.616x | 2.251-2.981x |

**Original versus retained, all workloads.** Append operations unpack 12,001 frames. Search operations examine 361 or 721 candidate offsets with 360 or 720 samples per candidate. Complete stream operations feed 12,001 source frames in 997-frame packets, pull up to 256 frames, and include output fingerprinting. Crossfade operations process the number of samples in their label.

| Workload | Before cycles/op | Retained cycles/op | CPU throughput | Before -> retained ns/op |
| --- | ---: | ---: | ---: | ---: |
| `sola/append-16ch` | 601,506.31 | 570,795.17 | 1.054x | 274,524.61 -> 260,564.45 |
| `sola/append-6ch` | 238,541.62 | 201,414.40 | 1.184x | 108,928.91 -> 91,996.49 |
| `sola/append-8ch` | 308,331.47 | 273,754.21 | 1.126x | 140,723.83 -> 124,927.74 |
| `sola/crossfade-1440` | 5,414.51 | 377.80 | 14.332x | 2,470.70 -> 171.33 |
| `sola/crossfade-256` | 983.46 | 97.75 | 10.061x | 447.27 -> 43.80 |
| `sola/left-silent/360` | 310,150.06 | 307,275.98 | 1.009x | 141,476.57 -> 140,145.32 |
| `sola/left-silent/720` | 1,244,784.50 | 1,235,400.89 | 1.008x | 567,835.94 -> 563,537.50 |
| `sola/mono/360` | 282,091.80 | 114,359.50 | 2.467x | 128,695.32 -> 52,148.44 |
| `sola/mono/720` | 1,191,075.62 | 461,536.47 | 2.581x | 543,421.88 -> 210,435.94 |
| `sola/right-silent/360` | 288,793.42 | 292,559.20 | 0.987x | 131,714.06 -> 133,496.88 |
| `sola/right-silent/720` | 1,239,389.62 | 1,245,467.03 | 0.995x | 565,412.50 -> 568,167.19 |
| `sola/stereo/360` | 312,252.45 | 319,276.48 | 0.978x | 142,451.57 -> 145,693.75 |
| `sola/stereo/720` | 1,340,606.53 | 1,337,245.45 | 1.003x | 611,754.69 -> 613,037.50 |
| `sola/stream-1ch-noise-1.2` | 1,301,635.25 | 586,339.50 | 2.220x | 592,975.00 -> 266,700.00 |
| `sola/stream-2ch-left-silent-1.2` | 1,732,404.00 | 1,781,023.25 | 0.973x | 789,900.00 -> 812,025.00 |
| `sola/stream-2ch-noise-0.8` | 2,889,059.00 | 2,977,243.00 | 0.970x | 1,319,150.00 -> 1,358,400.00 |
| `sola/stream-2ch-silence-1.2` | 143,662.75 | 98,720.25 | 1.455x | 65,050.00 -> 44,575.00 |
| `sola/stream-2ch-tone-1.2` | 379,186.50 | 220,158.25 | 1.722x | 172,375.00 -> 99,875.00 |
| `sola/stream-6ch-noise-1.2` | 7,958,632.00 | 3,467,771.25 | 2.295x | 3,631,700.00 -> 1,581,075.00 |

Independent stereo correlation remains a control: the retained whole-stream stereo noise and left-silent fixtures measure 0.970x and 0.973x baseline CPU throughput. These tradeoffs and the variability in unchanged search kernels are included rather than claiming a speedup for every fixture. Full [raw measurements](2026-10-26-audio-sola-retained.tsv) and [compiler/source/binary manifests](2026-10-26-audio-sola-retained.manifest.json) accompany this report.

**Correctness and soundness.** All four retained stages produced identical 6,714,176-byte captures, compared byte for byte. Captures include every f32 sample bit and each pull boundary across 1/2/6/8 channels, 8/44.1/48/96 kHz, packet sizes 137/997/4096, pull caps 7/256/997, live rate changes 1.2/0.8/2.0/0.5, EOF drains, and loop resets. SHA-256: `60b5056470be9ea45ebddce67fcda5a7fe492a2e17fd3c19a78d4c1cacc044b4`. [Identity evidence](../testing/2026-10-26-audio-sola-identity.json).

The suite also checks every i16 value through each unpacked channel at widths 1/2/3/4/6/8/16; correlation choices against a scalar oracle; exact scores at rejection and rounding boundaries, including full-scale opposing samples; and dispatched crossfade bits against scalar fused arithmetic. The audio crate passes 44 unit tests plus the playback integration test, with four benchmark/capture tests intentionally ignored by the default suite and the new benchmark/capture tests run separately. Clippy performance checks and rustfmt pass. Metadata compilation passes for aarch64 Linux and i686 Windows; those targets were not executed. [Check logs](../testing/2026-10-26-audio-sola-checks.json). Miri passes with both `+fma` and `-fma`, explicitly confirming the selected branch. [Miri logs](../testing/2026-10-26-audio-sola-miri.json).

**Rejected experiments.** Plain scalar chunks did not improve throughput consistently ([data](2026-10-26-audio-sola-scalar.tsv)). Searching only the unmatched stereo channel produced negligible gains ([data](2026-10-26-audio-sola-quiet.tsv)). Eager stereo scoring regressed searches about 22-32% ([data](2026-10-26-audio-sola-stereo.tsv)); chunked stereo scoring gave no meaningful gain ([data](2026-10-26-audio-sola-stereo-chunks.tsv)). All four were reverted. Initial [unpacking](2026-10-26-audio-sola-append.tsv), [FMA](2026-10-26-audio-sola-fma.tsv), [exact sums](2026-10-26-audio-sola-sums.tsv), and [bounded-block refinement](2026-10-26-audio-sola-blocked.tsv) runs are retained as exploratory evidence; final claims use the eight-process consecutive comparison above.

**Reproduction.** Run `python scripts/benchmark_audio_sola.py --build current` to compile and check the current production kernels. The runner accepts `--source PATH` to archive an earlier source exported from Git; it supplies the same test include when absent. `--compare NAME1 NAME2 --runs 8 --output PATH.tsv` rotates archived executables, verifies PCM fingerprints, records allocator probes, and writes manifests. `--verify NAME1 NAME2 --output PATH.json` rebuilds archived sources with the current oracle/capture harness and compares complete output bytes. Archives live under `target/audio-sola`. Cargo checks use `cargo test -p deadlib-audio --lib --test playback -- --test-threads=1` and `cargo clippy -p deadlib-audio --all-targets --no-deps -- -D clippy::perf`. The existing unused `is_direct` method warning in the old processing test baseline remains.
