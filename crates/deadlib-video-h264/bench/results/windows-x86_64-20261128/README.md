# H.264 parsing and CAVLC revision comparison (2026-11-28)

Host: Intel Xeon E5-2696 v4, Windows x64, logical CPU 42; rustc 1.100.0. Both revisions use the same direct release rustc settings (opt-level=3, 16 codegen units, default target CPU, std and surface enabled, no LTO). FFmpeg 7.1 (Gyan full build) provided input preparation and independent output hashes. The benchmark uses QueryThreadCycleTime on one pinned decoding thread; cycles and allocation traffic are measured in separate executables. One invocation starts with fresh decoder and lazy tables. Allocation calls include alloc, alloc_zeroed and realloc; bytes are requested traffic rather than peak memory.

The two 1080p samples are the first 45 coded pictures of the bundled SRPG9 and SRPG10 game videos. The CAVLC sample is a 90-picture 640x360 baseline-profile transcode of the real SRPG10 video. Annex B and four-byte AVCC inputs contain the same NALs. The pipeline includes framing, RBSP extraction, SPS parsing, picture decoding, display ordering, output consumption, draining, and destruction. The packed output uses the default reorder depth, the native surface output uses the signaled SPS depth. Parsing repeats 64 times per sample, normalized to one parse; the peek microbenchmark cycles through CAVLC lookup widths on the real compressed bytes. That microbenchmark is a representative operation mix, not an instrumented replay of the exact decoder call trace.

Stage 0 = original source at af1cf3ff8; stage 1 = b03dbf04e; stage 2 = b409c4d65; stage 3 = 0ce490b4e.

Each comparison alternates the order of the two binaries across rounds. Absolute cycle and throughput figures are individual medians; percentage changes are medians of paired ratios, so they need not equal the ratio of individual medians. The full paired range is provided to show variance. Allocation counts and requested bytes were unchanged (0% change) across two fresh-process runs in every comparison. The three changes introduce no unsafe code or new dependencies. High-profile controls use CABAC; the third change primarily benefits CAVLC. These measurements establish the decoder pipeline behavior for these inputs/build settings, not the complete DeadSync render loop or every possible stream/platform. Other builds were active on the host; large outliers and small or contradictory cycle changes are inconclusive.

## Input identity

| File | Bytes | SHA-256 |
|---|---:|---|
| cavlc.avcc | 162593 | `68ea7bc1e55a39512493261594262a266bfd5f005486891e21faaa5003b2fd64` |
| cavlc.h264 | 162591 | `1a5cee1db2ef746fa4b9ee304cd6760a202a859b5f911f056f493ae44c449da0` |
| srpg10.avcc | 608716 | `0d6b46fbe33e50f2e90e00b8e6731243a05c480689aeef5f828e6fe139667811` |
| srpg10.h264 | 608670 | `6deb4073b70632e32f4237f39b0b535982a49f2afaddef8b49b0b55a78b69e83` |
| srpg9.avcc | 762600 | `eb96cc1664b14e180ed0c7d79933d506aec68b7aea3cbc02292acdd41a669d99` |
| srpg9.h264 | 762599 | `da749bf97d34eca0a907433ba55bc19e55ca6a2ee72603636402fe65ed659a4e` |

## stage0-stage1-parse-annex-surface-annex-packed-annex

[Raw paired samples](stage0-stage1-parse-annex-surface-annex-packed-annex.csv)

Throughput units: bytes/billion cycles for parser rows, peeks/billion cycles for peek rows, frames/billion cycles for decoding rows.

| Input | Mode | Pairs | Cycles before → after | Paired cycles Δ (min…max) | Throughput before → after (paired Δ) | Allocation calls before → after | Requested bytes before → after |
|---|---|---:|---:|---:|---:|---:|---:|
| srpg9 | parse-annex | 7 | 2,967,767 → 1,352,024 | -54.80% (-56.18%…-52.37%) | 256,879,340.0 → 563,864,270.5 (+121.25%) | 6 → 6 | 3,998 → 3,998 |
| srpg9 | surface-annex | 7 | 1,566,908,228 → 1,597,697,480 | +0.23% (-2.47%…+3.38%) | 28.7 → 28.2 (-0.23%) | 404 → 404 | 44,221,088 → 44,221,088 |
| srpg9 | packed-annex | 7 | 1,677,080,527 → 1,686,617,148 | +0.53% (-4.64%…+4.11%) | 26.8 → 26.7 (-0.52%) | 580 → 580 | 185,246,984 → 185,246,984 |
| srpg10 | parse-annex | 7 | 2,564,439 → 1,233,127 | -51.91% (-52.81%…-49.72%) | 237,185,587.3 → 493,256,562.0 (+107.96%) | 9 → 9 | 54,206 → 54,206 |
| srpg10 | surface-annex | 7 | 1,142,471,153 → 1,127,969,234 | +0.09% (-7.90%…+4.54%) | 39.4 → 39.9 (-0.09%) | 304 → 304 | 26,433,606 → 26,433,606 |
| srpg10 | packed-annex | 7 | 1,222,899,467 → 1,285,325,716 | +0.31% (-1.99%…+11.07%) | 36.8 → 35.0 (-0.30%) | 475 → 475 | 167,458,926 → 167,458,926 |
| cavlc | parse-annex | 7 | 661,072 → 294,991 | -55.79% (-57.13%…-54.18%) | 245,248,533.7 → 549,600,138.5 (+126.17%) | 7 → 7 | 8,089 → 8,089 |
| cavlc | surface-annex | 7 | 231,473,723 → 230,104,699 | -0.35% (-1.28%…+1.06%) | 388.8 → 391.1 (+0.35%) | 60,567 → 60,567 | 2,921,969 → 2,921,969 |
| cavlc | packed-annex | 7 | 252,655,478 → 249,911,505 | -0.42% (-1.56%…+1.18%) | 356.2 → 360.1 (+0.42%) | 60,899 → 60,899 | 34,750,585 → 34,750,585 |

## stage0-stage3-parse-annex-parse-avcc-surface-avcc-packed-annex

[Raw paired samples](stage0-stage3-parse-annex-parse-avcc-surface-avcc-packed-annex.csv)

Throughput units: bytes/billion cycles for parser rows, peeks/billion cycles for peek rows, frames/billion cycles for decoding rows.

| Input | Mode | Pairs | Cycles before → after | Paired cycles Δ (min…max) | Throughput before → after (paired Δ) | Allocation calls before → after | Requested bytes before → after |
|---|---|---:|---:|---:|---:|---:|---:|
| srpg9 | parse-annex | 7 | 2,916,709 → 497,099 | -82.93% (-83.07%…-82.88%) | 261,376,085.0 → 1,533,614,366.0 (+485.83%) | 6 → 6 | 3,998 → 3,998 |
| srpg9 | parse-avcc | 7 | 1,031,952 → 252,442 | -75.61% (-78.87%…-60.55%) | 738,753,442.4 → 3,019,931,609.3 (+310.03%) | 6 → 6 | 3,998 → 3,998 |
| srpg9 | surface-avcc | 7 | 1,673,103,181 → 1,667,374,681 | +1.53% (-9.10%…+4.59%) | 26.9 → 27.0 (-1.51%) | 404 → 404 | 44,221,088 → 44,221,088 |
| srpg9 | packed-annex | 7 | 1,938,141,110 → 1,773,212,772 | -8.51% (-37.49%…+1.87%) | 23.2 → 25.4 (+9.30%) | 580 → 580 | 185,246,984 → 185,246,984 |
| srpg10 | parse-annex | 7 | 2,590,745 → 430,899 | -83.36% (-83.87%…-80.96%) | 234,777,275.0 → 1,411,578,791.6 (+501.12%) | 9 → 9 | 54,206 → 54,206 |
| srpg10 | parse-avcc | 7 | 986,738 → 213,052 | -78.46% (-79.00%…-77.30%) | 616,422,855.4 → 2,854,924,923.0 (+364.33%) | 9 → 9 | 54,206 → 54,206 |
| srpg10 | surface-avcc | 7 | 1,162,507,557 → 1,173,459,508 | -0.57% (-27.76%…+52.66%) | 38.7 → 38.3 (+0.58%) | 304 → 304 | 26,433,606 → 26,433,606 |
| srpg10 | packed-annex | 7 | 1,307,536,467 → 1,295,241,181 | -0.39% (-1.12%…+1.07%) | 34.4 → 34.7 (+0.39%) | 475 → 475 | 167,458,926 → 167,458,926 |
| cavlc | parse-annex | 7 | 651,809 → 128,123 | -80.89% (-81.57%…-79.65%) | 248,734,027.5 → 1,265,402,604.9 (+423.39%) | 7 → 7 | 8,089 → 8,089 |
| cavlc | parse-avcc | 7 | 242,866 → 62,770 | -73.19% (-75.82%…-72.43%) | 667,556,130.1 → 2,582,868,197.9 (+273.06%) | 7 → 7 | 8,089 → 8,089 |
| cavlc | surface-avcc | 7 | 236,817,457 → 221,216,929 | -6.36% (-8.21%…-5.96%) | 380.0 → 406.8 (+6.79%) | 60,567 → 60,567 | 2,921,969 → 2,921,969 |
| cavlc | packed-annex | 7 | 252,274,205 → 235,144,863 | -7.20% (-8.35%…-4.68%) | 356.8 → 382.7 (+7.76%) | 60,899 → 60,899 | 34,750,585 → 34,750,585 |

## stage1-stage2-parse-avcc-surface-avcc-packed-avcc

[Raw paired samples](stage1-stage2-parse-avcc-surface-avcc-packed-avcc.csv)

Throughput units: bytes/billion cycles for parser rows, peeks/billion cycles for peek rows, frames/billion cycles for decoding rows.

| Input | Mode | Pairs | Cycles before → after | Paired cycles Δ (min…max) | Throughput before → after (paired Δ) | Allocation calls before → after | Requested bytes before → after |
|---|---|---:|---:|---:|---:|---:|---:|
| srpg9 | parse-avcc | 7 | 1,046,446 → 257,487 | -75.39% (-75.92%…-71.24%) | 728,521,337.0 → 2,960,760,732.0 (+306.41%) | 6 → 6 | 3,998 → 3,998 |
| srpg9 | surface-avcc | 7 | 1,624,693,568 → 1,601,310,889 | +2.58% (-1.62%…+4.00%) | 27.7 → 28.1 (-2.52%) | 404 → 404 | 44,221,088 → 44,221,088 |
| srpg9 | packed-avcc | 7 | 1,772,917,965 → 1,753,701,829 | -1.56% (-26.18%…+3.78%) | 25.4 → 25.7 (+1.58%) | 580 → 580 | 185,246,984 → 185,246,984 |
| srpg10 | parse-avcc | 7 | 995,717 → 218,423 | -78.28% (-78.54%…-76.37%) | 610,864,236.1 → 2,784,724,055.8 (+360.31%) | 9 → 9 | 54,206 → 54,206 |
| srpg10 | surface-avcc | 7 | 1,269,819,566 → 1,203,186,397 | -6.37% (-8.71%…+50.57%) | 35.4 → 37.4 (+6.80%) | 304 → 304 | 26,433,606 → 26,433,606 |
| srpg10 | packed-avcc | 7 | 1,352,011,360 → 1,320,611,418 | +1.41% (-5.33%…+9.49%) | 33.3 → 34.1 (-1.39%) | 475 → 475 | 167,458,926 → 167,458,926 |
| cavlc | parse-avcc | 7 | 231,998 → 62,743 | -73.54% (-75.83%…-68.82%) | 698,829,959.2 → 2,583,997,047.5 (+277.91%) | 7 → 7 | 8,089 → 8,089 |
| cavlc | surface-avcc | 7 | 230,744,980 → 230,260,988 | +1.01% (-2.78%…+2.31%) | 390.0 → 390.9 (-1.00%) | 60,567 → 60,567 | 2,921,969 → 2,921,969 |
| cavlc | packed-avcc | 7 | 245,003,923 → 244,654,916 | -0.46% (-2.66%…+1.25%) | 367.3 → 367.9 (+0.46%) | 60,899 → 60,899 | 34,750,585 → 34,750,585 |

## stage1-stage2-surface-avcc-surface-annex

[Raw paired samples](stage1-stage2-surface-avcc-surface-annex.csv)

Throughput units: bytes/billion cycles for parser rows, peeks/billion cycles for peek rows, frames/billion cycles for decoding rows.

| Input | Mode | Pairs | Cycles before → after | Paired cycles Δ (min…max) | Throughput before → after (paired Δ) | Allocation calls before → after | Requested bytes before → after |
|---|---|---:|---:|---:|---:|---:|---:|
| srpg9 | surface-avcc | 11 | 1,707,432,105 → 1,693,125,095 | -0.67% (-31.22%…+15.52%) | 26.4 → 26.6 (+0.67%) | 404 → 404 | 44,221,088 → 44,221,088 |
| srpg9 | surface-annex | 11 | 1,640,589,522 → 1,651,957,888 | +1.82% (-2.78%…+25.78%) | 27.4 → 27.2 (-1.79%) | 404 → 404 | 44,221,088 → 44,221,088 |

## stage2-stage3-peek-surface-avcc-packed-avcc

[Raw paired samples](stage2-stage3-peek-surface-avcc-packed-avcc.csv)

Throughput units: bytes/billion cycles for parser rows, peeks/billion cycles for peek rows, frames/billion cycles for decoding rows.

| Input | Mode | Pairs | Cycles before → after | Paired cycles Δ (min…max) | Throughput before → after (paired Δ) | Allocation calls before → after | Requested bytes before → after |
|---|---|---:|---:|---:|---:|---:|---:|
| srpg9 | surface-avcc | 7 | 1,674,093,797 → 1,602,532,176 | -2.88% (-6.35%…+27.88%) | 26.9 → 28.1 (+2.96%) | 404 → 404 | 44,221,088 → 44,221,088 |
| srpg9 | packed-avcc | 7 | 1,781,674,710 → 1,777,257,699 | -0.25% (-3.02%…+10.36%) | 25.3 → 25.3 (+0.25%) | 580 → 580 | 185,246,984 → 185,246,984 |
| srpg10 | surface-avcc | 7 | 1,172,580,412 → 1,218,752,911 | -1.45% (-31.05%…+55.36%) | 38.4 → 36.9 (+1.47%) | 304 → 304 | 26,433,606 → 26,433,606 |
| srpg10 | packed-avcc | 7 | 1,348,391,133 → 1,335,298,841 | -4.81% (-23.17%…+2.82%) | 33.4 → 33.7 (+5.06%) | 475 → 475 | 167,458,926 → 167,458,926 |
| cavlc | peek | 7 | 31,961,833 → 3,346,936 | -89.52% (-89.62%…-89.39%) | 13,565,304.6 → 129,542,961.1 (+854.39%) | 0 → 0 | 0 → 0 |
| cavlc | surface-avcc | 7 | 226,251,383 → 213,271,906 | -5.86% (-9.23%…-3.44%) | 397.8 → 422.0 (+6.22%) | 60,567 → 60,567 | 2,921,969 → 2,921,969 |
| cavlc | packed-avcc | 7 | 244,358,373 → 233,795,378 | -4.60% (-42.64%…+7.57%) | 368.3 → 385.0 (+4.82%) | 60,899 → 60,899 | 34,750,585 → 34,750,585 |

## Output identity

Each stage compared the appropriate packed and surface output against the previous stage and FFmpeg SHA-256 of visible YUV420p planes in display order:

| Clip | Frames | SHA-256 |
|---|---:|---|
| SRPG9 | 45 | `f9301a596ac487ba19393bc4f0fe4848d4a59fbd877a100063952633a88fed90` |
| SRPG10 | 45 | `fb59c6f413e15e17cccff1e43125a6758799cafd205d8d3b6a91154f15c8703e` |
| CAVLC | 90 | `2ae5a960e68994f390d01884047a3f62743c6e9e8ccf287b397b7cac9f69e374` |

## Validation

Cargo all-features tests: 96 unit, 4 integration and 7 documentation tests passed. The no-default-features check and cargo fmt check passed.

The start-code change skips nonzero words while preserving the original prefix search order. The EPB change removes the second prefix scan and byte-at-a-time copy; unchanged spans are copied directly. The bit peek change eliminates per-bit extraction in the common case while preserving the old zero-padding fallback. Boundary and exhaustive short-pattern tests cover the new paths.
