# H.264 decoder benchmark

This Windows x64 benchmark compares the crate with ITGmania's software FFmpeg H.264 decoder. The harness and FFmpeg imports live here, outside the decoder library.

Run from a Visual Studio x64 developer shell with Cargo, FFmpeg and Python on the path. Point the script to a checkout with ITGmania's bundled x64 FFmpeg 62 headers, DLLs and import libraries.

```powershell
.\crates\deadlib-video-h264\bench\measure.ps1 -ItgmaniaRoot ..\itgmania
```

Both codecs read the first 150 coded pictures of each bundled SRPG9 and SRPG10 video from identical in-memory Annex B payloads. These inputs are 1920 × 1080, 8-bit 4:2:0 High-profile streams with B-pictures. The benchmark checks plane bytes in display order against FFmpeg before collecting samples. The measured region includes input parsing, context initialization, decode, frame ordering, drain, and output destruction.

The cycle executable runs serially on a pinned logical CPU. It records Windows `QueryThreadCycleTime` over five runs with rotating decoder order, then reports the median, minimum and maximum. The allocation executable runs separately and counts successful heap allocation and reallocation requests and their requested bytes. It calibrates FFmpeg's CRT hooks before decoding. The requested-byte metric is allocation traffic. Cycle-normalized throughput is frames or megapixels per billion CPU cycles.

FFmpeg's native mode consumes its refcounted AVFrame directly. Its packed mode copies the visible planes to one fresh contiguous YUV buffer per picture. The Rust API returns three owned, packed planes. Both FFmpeg modes are reported so the cost of packing is visible.

## Reference implementation

At ITGmania's FFmpeg revision `1041abd`, the H.264 decoder takes frame planes and picture tables from pools and carries its macroblock work tables across pictures. The intra predictor uses cached modes from the top and left neighbors. On x86, FFmpeg's deblock path transposes vertical edges and dispatches vector filters. The CABAC hot path uses an inlined, mask-based binary decoder and SSE2/AVX reconstruction kernels handle inverse transforms. [Picture allocation](https://ffmpeg.org/doxygen/trunk/h264__slice_8c_source.html) · [Frame pools](https://ffmpeg.org/doxygen/trunk/get__buffer_8c_source.html) · [Intra mode prediction](https://ffmpeg.org/doxygen/trunk/h264__mvpred_8h_source.html) · [x86 deblocking](https://github.com/FFmpeg/FFmpeg/blob/master/libavcodec/x86/h264_deblock.asm) · [CABAC](https://ffmpeg.org/doxygen/trunk/cabac__functions_8h_source.html) · [x86 H.264 DSP dispatch](https://ffmpeg.org/doxygen/trunk/h264dsp__init_8c_source.html)

## Initial measurements

Runs used an Intel Xeon E5-2696 v4, logical CPU 2, Rust 1.100.0, and ITGmania FFmpeg `1041abd` with libavcodec 62.28.103. Each stage's outputs matched FFmpeg on both clips.

| Revision | Clip | Mcycles/frame | Frames/billion cycles | Allocation calls/frame | Requested MiB/frame |
|---|---|---:|---:|---:|---:|
| `4ca0a01ab` | SRPG9 | 76.46 | 13.08 | 328.67 | 14.39 |
| `e5153d888` | SRPG9 | 76.48 | 13.07 | 58.28 | 10.89 |
| `05942d190` | SRPG9 | 70.35 | 14.21 | 38.55 | 5.86 |
| `5b8dec99f` | SRPG9 | 62.48 | 16.01 | 38.55 | 5.86 |
| FFmpeg native, paired with `5b8dec99f` | SRPG9 | 35.05 | 28.53 | 17.91 | 0.29 |
| FFmpeg packed, paired with `5b8dec99f` | SRPG9 | 38.73 | 25.82 | 18.91 | 3.26 |
| `4ca0a01ab` | SRPG10 | 59.93 | 16.69 | 249.41 | 13.84 |
| `e5153d888` | SRPG10 | 58.57 | 17.07 | 48.26 | 10.99 |
| `05942d190` | SRPG10 | 58.02 | 17.24 | 28.81 | 6.07 |
| `5b8dec99f` | SRPG10 | 50.73 | 19.71 | 28.81 | 6.07 |
| FFmpeg native, paired with `5b8dec99f` | SRPG10 | 24.27 | 41.21 | 17.63 | 0.18 |
| FFmpeg packed, paired with `5b8dec99f` | SRPG10 | 27.54 | 36.31 | 18.63 | 3.15 |

A separate instrumented run of `05942d190` attributed 66–67% of cycles to macroblock decode and 19–23% to deblocking. Those figures cover the two progressive clips. The five-run cycle spreads in the paired comparison ranged from 1.5% to 8.3%. A 1% equivalence claim requires repeated paired batches and additional stream types.

[Baseline samples](results/windows-x86_64-20261126/baseline) · [SIMD milestone samples](results/windows-x86_64-20261126/x86-deblock) · [Decode stage profile](results/windows-x86_64-20261126/regions.csv)

## Reference storage and slice context

`f2d3dbb51` reuses the planes and motion vectors of a retired reference after it has become exclusively owned. Requested allocation traffic in the SRPG9 and SRPG10 clips fell from 5.86 and 6.07 MiB per frame to 4.53 and 4.55 MiB. Allocation and reallocation calls fell to 36.02 and 25.91 per frame. [Reference-pool samples](results/windows-x86_64-20261126/reference-pool)

`46aadb9cf` keeps one macroblock context across a slice, as FFmpeg does with its slice context. Separate releases of the reference-pool and slice-context revisions ran inside the same benchmark process with rotating order for five rounds per clip. The median of the paired cycle ratios fell 2.4% on SRPG9 and 2.3% on SRPG10. Both clips and the MBAFF, constrained-intra multislice, and resolution-switch samples matched libavcodec. [Slice-context samples](results/windows-x86_64-20261126/slice-context) · [Paired cycle samples](results/windows-x86_64-20261126/slice-context-paired.csv)

An SSE2 4×4 and AVX2 8×8 inverse-transform prototype matched the scalar kernels and FFmpeg, including a High 10 stream. It used 1.7% more cycles on SRPG9 and 0.1% more on SRPG10 in the paired comparison and was discarded.

## Recycled output frames

`5bc79bbce` lets serial callers return a consumed frame so the decoder can reuse its three owned planes. The `deadlib-recycled` benchmark mode returns each frame after consuming its planes. Both Deadlib modes matched FFmpeg's output hashes with the default reorder depth.

| Clip | Output mode | Mcycles/frame | Frames/billion cycles | Allocation calls/frame | Requested MiB/frame |
|---|---|---:|---:|---:|---:|
| SRPG9 | Deadlib | 62.52 | 16.0 | 36.02 | 4.53 |
| SRPG9 | Deadlib recycled | 62.60 | 16.0 | 33.38 | 1.90 |
| SRPG9 | FFmpeg native | 36.75 | 27.2 | 17.91 | 0.29 |
| SRPG9 | FFmpeg packed | 40.11 | 24.9 | 18.91 | 3.26 |
| SRPG10 | Deadlib | 51.76 | 19.3 | 25.91 | 4.55 |
| SRPG10 | Deadlib recycled | 49.23 | 20.3 | 23.91 | 2.56 |
| SRPG10 | FFmpeg native | 25.49 | 39.2 | 17.63 | 0.18 |
| SRPG10 | FFmpeg packed | 29.20 | 34.2 | 18.63 | 3.15 |

Requested heap traffic fell by 58.0% and 43.7%. The recycled/default cycle ratios ranged from 0.91 to 1.03 on SRPG9 and from 0.88 to 1.10 on SRPG10 across five rounds, so the cycle benefit is uncertain. [Recycled-output samples](results/windows-x86_64-20261126/returned-output)

## Motion compensation at picture edges

`e91adb3ca` uses a block-sized edge extension for 8-bit luma, as FFmpeg does before quarter-pel interpolation. A boundary block in an MBAFF field previously allocated and copied the whole field. The replacement copies the required rows into a stack tile and uses the existing interpolation kernels. [FFmpeg motion compensation](https://github.com/FFmpeg/FFmpeg/blob/1041abdc962f4cc4f394aa8de9dc5236c0c3b9e7/libavcodec/h264_mb.c)

On a 60-frame 320 × 240 MBAFF clip with two slices per picture, the median paired cycle ratio relative to `46aadb9cf` was 0.848 across ten rounds on logical CPU 4. Throughput rose from 241 to 286 frames per billion cycles using the individual decoder medians. Allocation and reallocation calls fell from 90.58 to 84.92 per frame, and requested traffic fell from 0.776 to 0.555 MiB per frame. The MBAFF clip and both bundled progressive clips matched FFmpeg. [Edge-buffer samples](results/windows-x86_64-20261126/luma-edge)

```powershell
ffmpeg -nostdin -f lavfi -i 'testsrc2=size=320x240:rate=60' -vf 'tinterlace=mode=interleave_top' -frames:v 60 -c:v libx264 -threads 1 -flags +ilme+ildct -x264-params 'tff=1:slices=2' -f h264 mbaff-motion.h264
```

## Diagonal interpolation and spatial direct motion

FFmpeg's x86 diagonal quarter-pel filters stage the result of one six-tap pass and use it across the block. `b267ad384` stages the horizontal taps once per source row and reuses them for the vertical pass and the horizontal quarter-pel average. The Rust x86 kernels handle 4-, 8-, and 16-sample block widths and match the scalar filter. [FFmpeg x86 filters](https://github.com/FFmpeg/FFmpeg/blob/1041abdc962f4cc4f394aa8de9dc5236c0c3b9e7/libavcodec/x86/h264_qpel_8bit.asm)

`c3789c998` skips the co-located motion lookup when spatial direct refinement cannot change either motion vector. A uniform macroblock can then skip the per-block motion comparison before prediction. FFmpeg also uses uniform motion fills for spatial direct prediction. [FFmpeg spatial direct prediction](https://github.com/FFmpeg/FFmpeg/blob/1041abdc962f4cc4f394aa8de9dc5236c0c3b9e7/libavcodec/h264_direct.c)

Median paired cycle changes relative to `46aadb9cf` are from five rotating rounds per clip with one candidate change enabled at a time. Both motion changes matched the FFmpeg output hashes on the two bundled clips.

| Candidate | SRPG9 | SRPG10 |
|---|---:|---:|
| Luma edge tile | −0.8% | +0.4% |
| Block diagonal interpolation | −3.4% | −3.4% |
| Spatial direct motion | −4.1% | −3.1% |
| Borrowed slice bytes and cached reference keys prototype | −1.9% | +2.1% |

The slice prototype was left out of the measured combined revision. [Paired progressive cycle samples](results/windows-x86_64-20261126/motion-paired.csv)

## Retired-reference reserve and reorder depth

`64dca4237` retains up to four exclusively owned retired references for reuse. Reference marking and IDR pictures can retire several references together. The reserve lets subsequent pictures reuse the released planes and motion storage.

The `deadlib-recycled-sps` mode also uses `OrderedDecoder::with_max_depth` with the largest reorder depth declared in the input SPS units. It uses the default depth of 16 when an SPS lacks that declaration. Both bundled inputs matched FFmpeg's display-order output in this mode. All Deadlib modes return packed planes.

| Clip | Output mode | Allocation calls/frame | Requested MiB/frame |
|---|---|---:|---:|
| SRPG9 | Deadlib | 33.89 | 3.42 |
| SRPG9 | Deadlib recycled | 31.25 | 0.79 |
| SRPG9 | Deadlib recycled, SPS depth | 31.15 | 0.51 |
| SRPG9 | FFmpeg native | 17.91 | 0.29 |
| SRPG9 | FFmpeg packed | 18.91 | 3.26 |
| SRPG10 | Deadlib | 23.45 | 3.26 |
| SRPG10 | Deadlib recycled | 21.45 | 1.27 |
| SRPG10 | Deadlib recycled, SPS depth | 21.13 | 0.43 |
| SRPG10 | FFmpeg native | 17.63 | 0.18 |
| SRPG10 | FFmpeg packed | 18.63 | 3.15 |

Allocation counts matched in two runs of each mode. Across five cycle runs, the default decoder reached 57.59 and 46.47 Mcycles/frame on SRPG9 and SRPG10. Recycling with the declared reorder depth reached 54.23 and 44.58. Packed FFmpeg measured 39.75 and 28.17, and native FFmpeg measured 35.65 and 24.82. [Complete reserve samples](results/windows-x86_64-20261126/reference-reserve) · [Preceding motion revision samples](results/windows-x86_64-20261126/motion-combined)

## Slice reference lists and weighted prediction

`78b0cdc5d` builds the two B-slice reference lists from a single sorted short-term list for frames. It materializes the complete short-term reference list for list modification only when the slice contains modification commands. Allocation calls with the default output API fell from 33.89 to 23.34 per frame on SRPG9 and from 23.45 to 18.08 on SRPG10. An initial paired batch showed a 2.8% cycle increase on both clips. A repeat showed changes of +0.35% and −0.25%. [Reference-list allocation samples](results/windows-x86_64-20261126/reference-list-allocations.csv) · [Initial paired samples](results/windows-x86_64-20261126/entropy-paired.csv)

`cd0fb3edc` stores implicit prediction weights in contiguous tables and uses bitmasks for long-term reference flags. In isolation, this cut default allocation calls by 5.61 and 2.69 per frame. FFmpeg keeps its implicit weights in an inline slice-context table. [Compact-weight allocation samples](results/windows-x86_64-20261126/compact-weights-allocations.csv) · [FFmpeg weight-table initialization](https://github.com/FFmpeg/FFmpeg/blob/1041abdc962f4cc4f394aa8de9dc5236c0c3b9e7/libavcodec/h264_slice.c)

`37e4e52ab` uses SSE2 to weight 8-bit uni-prediction and implicit bi-prediction. Identity weights and equal averaging use simpler paths. FFmpeg also uses packed SIMD for weighted prediction. Median paired cycle ratios for the SIMD change alone were 0.921 and 0.897 relative to `64dca4237`. With both slice allocation changes included, the ratios were 0.919 and 0.907. All three candidates matched FFmpeg on the bundled clips, MBAFF and constrained-intra CABAC samples. The combined revision also matched on the temporal-direct multislice and High 10 samples. [Weighted prediction cycle samples](results/windows-x86_64-20261126/weights-paired.csv) · [FFmpeg x86 weighted prediction](https://github.com/FFmpeg/FFmpeg/blob/1041abdc962f4cc4f394aa8de9dc5236c0c3b9e7/libavcodec/x86/h264_weight.asm)

The paired CABAC refill and sparse 8×8 inverse-quantization prototypes showed no reproducible cycle reduction and were left out of this revision.

The combined revision completed five cycle runs and two matching allocation runs. Its default output requested fewer allocation calls per frame than both FFmpeg modes on the two clips. Requested bytes and cycles still exceeded the comparable FFmpeg modes. [Complete weighted-prediction benchmark](results/windows-x86_64-20261126/weighted-prediction)

| Clip | Output mode | Mcycles/frame | Frames/billion cycles | Allocation calls/frame | Requested MiB/frame |
|---|---|---:|---:|---:|---:|
| SRPG9 | Deadlib | 50.15 | 19.9 | 17.73 | 3.42 |
| SRPG9 | Deadlib recycled, SPS depth | 48.41 | 20.7 | 14.99 | 0.51 |
| SRPG9 | FFmpeg native | 34.17 | 29.3 | 17.91 | 0.29 |
| SRPG9 | FFmpeg packed | 37.17 | 26.9 | 18.91 | 3.26 |
| SRPG10 | Deadlib | 42.04 | 23.8 | 15.39 | 3.26 |
| SRPG10 | Deadlib recycled, SPS depth | 40.94 | 24.4 | 13.07 | 0.43 |
| SRPG10 | FFmpeg native | 24.70 | 40.5 | 17.63 | 0.18 |
| SRPG10 | FFmpeg packed | 27.84 | 35.9 | 18.63 | 3.15 |

## Temporal reference keys and deblock edges

`e1b2c2329` stores reference-picture IDs per slice and resolves them by macroblock slice and reference index when temporal direct prediction needs them. This replaces two full-picture arrays filled with per-block POCs. FFmpeg also stores reference identities by list index for temporal direct prediction. [FFmpeg temporal direct prediction](https://github.com/FFmpeg/FFmpeg/blob/1041abdc962f4cc4f394aa8de9dc5236c0c3b9e7/libavcodec/h264_direct.c)

`7f01bb1f9` skips interior deblock edges of inter macroblocks with uniform motion and zero luma coefficients. It also shares the motion check across edge segments when both sides have uniform motion. FFmpeg similarly skips interior edges for unpartitioned inter macroblocks with no luma residual. [FFmpeg loop filter](https://github.com/FFmpeg/FFmpeg/blob/1041abdc962f4cc4f394aa8de9dc5236c0c3b9e7/libavcodec/h264_loopfilter.c)

Median per-round cycle ratios versus `37e4e52ab` were 0.966 and 0.946 with both changes applied. The combined revision matched FFmpeg on both bundled clips, temporal-direct progressive and MBAFF multislice clips, constrained-intra CAVLC and the resolution change. [Paired samples](results/windows-x86_64-20261126/reference-keys-and-deblock-paired.csv)

The default output requested 3.36 and 3.23 MiB/frame, down from 3.42 and 3.26. The recycled output with SPS depth requested 0.45 and 0.40 MiB/frame. The separate full SRPG10 batch had wide sample spreads, including 73% for packed FFmpeg. The paired batch provides the cycle comparisons between revisions. [Full cycle and allocation samples](results/windows-x86_64-20261126/reference-keys-and-deblock)
