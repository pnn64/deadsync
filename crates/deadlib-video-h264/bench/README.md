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

## Serial slice payloads and full-pel B-skip prediction

`1af5170ed` borrows the already-parsed slice bytes during serial decoding. Slices queued for worker threads own their bytes. The allocation probe, which also included a deblock writeback prototype, removed 150 requests on each clip and reduced requested bytes by 2.20 MB on SRPG9 and 1.89 MB on SRPG10. [Allocation probe](results/windows-x86_64-20261126/serial-payload-allocations.csv) · [Isolated paired measurements](results/windows-x86_64-20261126/serial-payload-and-writeback-paired.csv)

`e021087c8` uses SSE2 to load and blend integer-position luma predictions directly into an 8-bit frame for uniform B-skip macroblocks. FFmpeg's unweighted path likewise averages into the destination when applying the second prediction. With the Rust luma change isolated, median paired cycle counts fell 5.85% on SRPG9 and 6.19% on SRPG10. It used 1.21× and 1.27× the cycles of packed FFmpeg. The isolated build matched FFmpeg on both bundled clips and the temporal-direct, constrained-intra and resolution-change fixtures. [Paired samples](results/windows-x86_64-20261126/integer-prediction-paired.csv) · [FFmpeg motion compensation](https://github.com/FFmpeg/FFmpeg/blob/1041abdc962f4cc4f394aa8de9dc5236c0c3b9e7/libavcodec/h264_mb.c)

On the preceding revision, a diagnostic build sampled one in 32 CABAC macroblocks using the timestamp counter. Motion compensation for luma and chroma together accounted for about 36% and 29% of sampled macroblock ticks. These estimates guide optimization. The cycle comparisons use `QueryThreadCycleTime` on uninstrumented builds. [Macroblock profile](results/windows-x86_64-20261126/macroblock-profile.csv)

The full benchmark at `e021087c8` matched FFmpeg on both clips in every output mode. The five-run cycle spreads were 2.7–3.4% for default Deadlib and packed FFmpeg. The two allocation runs agreed for every mode. Default Deadlib used 20.7% and 29.5% more cycles than packed FFmpeg and requested 2.7% and 2.2% more bytes per frame. [Full cycle and allocation samples](results/windows-x86_64-20261126/serial-payload-and-luma)

| Clip | Output mode | Mcycles/frame | Frames/billion cycles | Allocation calls/frame | Requested MiB/frame |
|---|---|---:|---:|---:|---:|
| SRPG9 | Deadlib | 45.02 | 22.21 | 16.73 | 3.34 |
| SRPG9 | Deadlib recycled, SPS depth | 43.20 | 23.15 | 13.98 | 0.43 |
| SRPG9 | FFmpeg native | 34.08 | 29.35 | 17.91 | 0.29 |
| SRPG9 | FFmpeg packed | 37.30 | 26.81 | 18.91 | 3.26 |
| SRPG10 | Deadlib | 34.84 | 28.70 | 14.39 | 3.22 |
| SRPG10 | Deadlib recycled, SPS depth | 32.90 | 30.39 | 12.07 | 0.39 |
| SRPG10 | FFmpeg native | 23.63 | 42.31 | 17.63 | 0.18 |
| SRPG10 | FFmpeg packed | 26.90 | 37.18 | 18.63 | 3.15 |

## Full-pel chroma bi-prediction

`b4110122d` blends the two full-pel chroma predictions directly into the Cb and Cr planes for eligible 8-bit frame macroblocks. The isolated candidate matched FFmpeg on both bundled clips, MBAFF, the resolution change, High 4:2:2 and High 10. [Paired cycles](results/windows-x86_64-20261126/integer-chroma-paired.csv)

Median paired cycles fell 6.46% on SRPG9 and 11.91% on SRPG10 when chroma was added to the full-pel luma build. The candidate used 11.85% and 13.47% more cycles than packed FFmpeg. A reference retirement prototype in the same batch freed DPB storage before copying the current picture. Its median paired cycle changes were +0.38% and −0.39% relative to the pre-luma baseline.

The full benchmark of `b4110122d` matched FFmpeg on every output mode and reproduced the preceding allocation counts and byte totals. Median default cycles were 14.0% and 12.9% above the packed FFmpeg medians. Across the five runs, cycle spreads ranged from 3.4% to 6.7% for these modes. [Full cycle and allocation samples](results/windows-x86_64-20261126/full-pel-chroma)

| Clip | Output mode | Mcycles/frame | Frames/billion cycles | Allocation calls/frame | Requested MiB/frame |
|---|---|---:|---:|---:|---:|
| SRPG9 | Deadlib | 42.28 | 23.65 | 16.73 | 3.34 |
| SRPG9 | Deadlib recycled, SPS depth | 40.98 | 24.40 | 13.98 | 0.43 |
| SRPG9 | FFmpeg native | 34.06 | 29.36 | 17.91 | 0.29 |
| SRPG9 | FFmpeg packed | 37.08 | 26.97 | 18.91 | 3.26 |
| SRPG10 | Deadlib | 31.09 | 32.16 | 14.39 | 3.22 |
| SRPG10 | Deadlib recycled, SPS depth | 29.23 | 34.22 | 12.07 | 0.39 |
| SRPG10 | FFmpeg native | 24.33 | 41.10 | 17.63 | 0.18 |
| SRPG10 | FFmpeg packed | 27.54 | 36.31 | 18.63 | 3.15 |

## Borrowed bit reader

`a75c0e03c` reads the existing RBSP slice and supplies virtual zero padding to the bit reader. This removes separate padded copies during header parsing and slice decoding. The committed code matched FFmpeg on both bundled clips, the temporal-direct progressive and MBAFF multislice samples, and constrained-intra CABAC and CAVLC. The default and no-default-features tests passed. [Allocation probe](results/windows-x86_64-20261126/bit-reader-allocations.csv)

The probe reproduced the prior default allocation totals before the change. With the borrowed reader, default allocation calls per frame fell from 16.73 to 11.71 on SRPG9 and from 14.39 to 9.33 on SRPG10. Requested bytes fell from 3.343 to 3.272 MiB per frame and from 3.219 to 3.158. These totals are 0.54% and 0.23% above the packed FFmpeg totals. The recycled decoder with SPS reorder depth requested 0.364 and 0.329 MiB per frame. The comparison with native FFmpeg still shows a gap in requested bytes.

The full benchmark reproduced those allocations in both runs and matched the FFmpeg output on both clips. The five-round paired cycle changes versus `b4110122d` were +2.38% on SRPG9 and −0.46% on SRPG10. An interior fractional chroma blend prototype in the same paired run changed cycles by +0.80% and −0.54%. [Paired cycles](results/windows-x86_64-20261126/bit-reader-paired.csv) · [Full benchmark](results/windows-x86_64-20261126/bit-reader)

| Clip | Output mode | Mcycles/frame | Frames/billion cycles | Allocation calls/frame | Requested MiB/frame |
|---|---|---:|---:|---:|---:|
| SRPG9 | Deadlib | 42.38 | 23.60 | 11.71 | 3.272 |
| SRPG9 | Deadlib recycled, SPS depth | 40.92 | 24.44 | 8.95 | 0.364 |
| SRPG9 | FFmpeg native | 33.80 | 29.58 | 17.91 | 0.289 |
| SRPG9 | FFmpeg packed | 37.44 | 26.71 | 18.91 | 3.255 |
| SRPG10 | Deadlib | 32.52 | 30.75 | 9.33 | 3.158 |
| SRPG10 | Deadlib recycled, SPS depth | 30.48 | 32.80 | 6.99 | 0.329 |
| SRPG10 | FFmpeg native | 24.77 | 40.37 | 17.63 | 0.184 |
| SRPG10 | FFmpeg packed | 28.28 | 35.36 | 18.63 | 3.151 |

A diagnostic run sampled one in 32 CABAC macroblocks and instrumented the full deblock and macroblock metadata passes. Deblocking accounted for about 17.5% and 15.2% of the instrumented decode ticks. The metadata pass accounted for another 4.2% and 5.1%. [Macroblock profile after luma and chroma blending](results/windows-x86_64-20261126/post-fusion-profile.csv)

## Uniform deblock motion and reference progress

`a019348c9` writes one motion entry for an inter macroblock whose 16 blocks share both reference indices and vectors. The boundary-strength calculation reuses that entry on edges touching a partitioned macroblock. The decoder also resets only the filter metadata it needs before decoding the next picture. FFmpeg skips motion comparisons on eligible edges using its macroblock partition type. [FFmpeg boundary-strength calculation](https://github.com/FFmpeg/FFmpeg/blob/1041abdc962f4cc4f394aa8de9dc5236c0c3b9e7/libavcodec/h264_loopfilter.c)

`2ae41f6d4` checks the reference's published rows before calculating a row to wait for. Complete references and worker references with enough published rows can take the fast path. FFmpeg only computes reference waits when frame threading is active. [FFmpeg motion compensation and reference waits](https://github.com/FFmpeg/FFmpeg/blob/1041abdc962f4cc4f394aa8de9dc5236c0c3b9e7/libavcodec/h264_mc_template.c)

| Candidate versus `a75c0e03c` | SRPG9 median paired cycle change | SRPG10 median paired cycle change |
|---|---:|---:|
| Uniform deblock motion | −5.33% | −4.93% |
| Published-reference fast path | −3.61% | −2.09% |
| Direct strided luma for unweighted skip partitions | −2.06% | −0.68% |

The two committed changes matched FFmpeg in isolation on the bundled clips and temporal-direct progressive and MBAFF multislice clips. The uniform deblock motion build also matched the constrained-intra CAVLC and resolution-change samples. The strided luma path remained a prototype. [Paired samples](results/windows-x86_64-20261126/decode-paths-paired.csv)

The full run of `2ae41f6d4` reproduced the borrowed reader's allocation totals in both passes. The cycle medians for the default decoder were 9.4% and 7.6% above packed FFmpeg. Both bundled clips matched in all five output modes. The same revision also matched FFmpeg on the High 10 and High 4:2:2 fixtures, and with three frame workers on the two bundled clips, the temporal-direct multislice fixtures and the resolution change. [Full run](results/windows-x86_64-20261126/decode-paths)

| Clip | Output mode | Mcycles/frame | Frames/billion cycles | Allocation calls/frame | Requested MiB/frame |
|---|---|---:|---:|---:|---:|
| SRPG9 | Deadlib | 40.72 | 24.56 | 11.71 | 3.272 |
| SRPG9 | Deadlib recycled, SPS depth | 39.07 | 25.59 | 8.95 | 0.364 |
| SRPG9 | FFmpeg native | 34.05 | 29.37 | 17.91 | 0.289 |
| SRPG9 | FFmpeg packed | 37.22 | 26.86 | 18.91 | 3.255 |
| SRPG10 | Deadlib | 30.42 | 32.87 | 9.33 | 3.158 |
| SRPG10 | Deadlib recycled, SPS depth | 28.23 | 35.42 | 6.99 | 0.329 |
| SRPG10 | FFmpeg native | 24.43 | 40.93 | 17.63 | 0.184 |
| SRPG10 | FFmpeg packed | 28.27 | 35.38 | 18.63 | 3.151 |

## Eight-lane chroma deblocking

`d27eb742b` uses SSE2 to load and filter eight 8-bit chroma samples directly at a horizontal or vertical edge. The vertical path transposes the four input columns and stores only the two columns that can change. An edge whose alpha or beta threshold is zero returns before loading pixels. FFmpeg likewise tests the thresholds before dispatching to its x86 chroma filters. [FFmpeg edge dispatch](https://github.com/FFmpeg/FFmpeg/blob/1041abdc962f4cc4f394aa8de9dc5236c0c3b9e7/libavcodec/h264_loopfilter.c) · [FFmpeg x86 chroma filters](https://github.com/FFmpeg/FFmpeg/blob/1041abdc962f4cc4f394aa8de9dc5236c0c3b9e7/libavcodec/x86/h264_deblock.asm)

| Candidate versus `2ae41f6d4` | SRPG9 median paired cycle change | SRPG10 median paired cycle change |
|---|---:|---:|
| Eight-lane chroma deblocking and zero thresholds | −1.98% | −3.34% |
| Per-segment threshold expansion | +1.28% | −0.35% |
| Chroma geometry fast path | +0.43% | +0.31% |

The chroma filter build used 5.22% and 4.96% more cycles than packed FFmpeg in the same batch. Its decoded frames matched FFmpeg on the bundled clips, temporal-direct progressive and MBAFF multislice, constrained-intra CAVLC and the resolution change. The other candidates remained prototypes. [Paired samples](results/windows-x86_64-20261126/chroma-edge-paired.csv)

The full benchmark reproduced the preceding allocation totals. The default cycle medians were 4.2% and 6.0% above packed FFmpeg, with sample spreads of 2.1–4.8% across those modes. All five benchmark output modes matched FFmpeg. Separate checks also matched on the High 10 and High 4:2:2 fixtures and with three frame workers on the two bundled clips, temporal-direct multislice progressive and MBAFF, and the resolution change. [Full run](results/windows-x86_64-20261126/chroma-edge)

| Clip | Output mode | Mcycles/frame | Frames/billion cycles | Allocation calls/frame | Requested MiB/frame |
|---|---|---:|---:|---:|---:|
| SRPG9 | Deadlib | 37.40 | 26.74 | 11.71 | 3.272 |
| SRPG9 | Deadlib recycled, SPS depth | 35.72 | 27.99 | 8.95 | 0.364 |
| SRPG9 | FFmpeg native | 32.60 | 30.68 | 17.91 | 0.289 |
| SRPG9 | FFmpeg packed | 35.88 | 27.87 | 18.91 | 3.255 |
| SRPG10 | Deadlib | 29.56 | 33.83 | 9.33 | 3.158 |
| SRPG10 | Deadlib recycled, SPS depth | 27.45 | 36.43 | 6.99 | 0.329 |
| SRPG10 | FFmpeg native | 24.61 | 40.63 | 17.63 | 0.184 |
| SRPG10 | FFmpeg packed | 27.90 | 35.84 | 18.63 | 3.151 |

## Slice layout specialization

`9f6f62795` selects a slice implementation by sample depth and MBAFF layout before the macroblock loop. This lets the compiler remove layout checks inside the selected path. FFmpeg similarly dispatches macroblock reconstruction to implementations for common layouts, higher bit depths and complex cases. [FFmpeg macroblock dispatch](https://github.com/FFmpeg/FFmpeg/blob/1041abdc962f4cc4f394aa8de9dc5236c0c3b9e7/libavcodec/h264_mb.c)

The isolated specialization saved 4.41% and 1.48% of median paired cycles relative to `2ae41f6d4`. With the chroma filter included, the linked reference-check executable grew by 355 KiB. The combined build matched FFmpeg on the bundled clips, temporal-direct progressive and MBAFF multislice, constrained-intra CAVLC, resolution changes, High 10 and High 4:2:2. [Slice and storage comparisons](results/windows-x86_64-20261126/slice-storage-paired.csv)

Earlier DPB retirement with inline sliding-window counts lowered requested bytes in the allocation probe by about 3.14 MB per 150-frame clip. Its paired cycle changes were +1.65% and −0.12%, so it remains a prototype. A separate batch against `d27eb742b` measured −0.29% and +0.02% for a direct horizontal luma edge kernel, and +3.87% and +2.36% for compilation with the host CPU's target features. [Luma and code-generation comparisons](results/windows-x86_64-20261126/luma-codegen-paired.csv)

The full run of `9f6f62795` matched FFmpeg on both clips and reproduced the prior allocation totals. The default medians were 3.04% and 0.51% above packed FFmpeg and 12.54% and 16.26% above native FFmpeg. The five default runs on SRPG9 spanned 6.4% of their median. [Full run](results/windows-x86_64-20261126/slice-specialization)

| Clip | Output mode | Mcycles/frame | Frames/billion cycles | Allocation calls/frame | Requested MiB/frame |
|---|---|---:|---:|---:|---:|
| SRPG9 | Deadlib | 37.27 | 26.83 | 11.71 | 3.272 |
| SRPG9 | Deadlib recycled, SPS depth | 35.34 | 28.30 | 8.95 | 0.364 |
| SRPG9 | FFmpeg native | 33.12 | 30.19 | 17.91 | 0.289 |
| SRPG9 | FFmpeg packed | 36.17 | 27.64 | 18.91 | 3.255 |
| SRPG10 | Deadlib | 27.14 | 36.84 | 9.33 | 3.158 |
| SRPG10 | Deadlib recycled, SPS depth | 25.23 | 39.64 | 6.99 | 0.329 |
| SRPG10 | FFmpeg native | 23.35 | 42.83 | 17.63 | 0.184 |
| SRPG10 | FFmpeg packed | 27.00 | 37.03 | 18.63 | 3.151 |

Adding a direct vertical luma kernel to the horizontal prototype changed median paired cycles by +2.12% on SRPG9 and −0.46% on SRPG10 relative to `d27eb742b`. The combined kernel stayed out of the decoder. [Direct luma edge comparisons](results/windows-x86_64-20261126/direct-luma-edges-paired.csv)

## Cropped output

`083cb9f9a` compacts visible rows in the coded buffers before truncating the planes. Returned frames retain coded capacity for the next picture. In the 150-frame 720p-input crop fixture, default allocation calls fell from 1,868 to 1,418 and requested bytes from 421.8 MB to 219.8 MB. With recycling and SPS reorder depth, requested bytes fell from 421.8 MB to 23.5 MB. Median paired cycles for the crop-only build fell 1.72% on the cropped clip. The cropped 4:2:0, High 10, and High 4:2:2 fixtures matched FFmpeg. [Allocation probe](results/windows-x86_64-20261126/crop-allocations.csv) · [Paired cycles](results/windows-x86_64-20261126/crop-bskip-paired.csv)

```powershell
ffmpeg -nostdin -v error -f lavfi -i 'testsrc2=size=1280x720:rate=24' -frames:v 150 -c:v libx264 -threads 2 -preset fast -bf 3 -g 72 -bsf:v 'h264_metadata=crop_left=4:crop_top=4:crop_right=8:crop_bottom=8' -f h264 cropped-720.h264
ffmpeg -nostdin -v error -i cropped-720.h264 -fps_mode passthrough -pix_fmt yuv420p -f rawvideo cropped-720.ffmpeg.yuv
```

## Shared surface experiments

FFmpeg hands the caller a reference to the decoded picture and returns the buffers to its pool when the last owner releases them. The Rust prototype shares completed planes between the reference buffer and display queue. Its pooled variant lets the display caller return consumed surfaces. It matched decoded bytes on the two bundled clips, temporal-direct multislice and MBAFF, constrained CAVLC and the resolution change. [FFmpeg frame output](https://github.com/FFmpeg/FFmpeg/blob/1041abdc962f4cc4f394aa8de9dc5236c0c3b9e7/libavcodec/h264dec.c) · [FFmpeg frame buffer pool](https://github.com/FFmpeg/FFmpeg/blob/1041abdc962f4cc4f394aa8de9dc5236c0c3b9e7/libavcodec/get_buffer.c) · [Cycle and allocation samples](results/windows-x86_64-20261126/surface-return-prototype)

| Clip | Output mode | Mcycles/frame | Frames/billion cycles | Allocation calls/frame | Requested MiB/frame |
|---|---|---:|---:|---:|---:|
| SRPG9 | Prototype, surfaces returned | 37.22 | 26.87 | 8.91 | 0.304 |
| SRPG9 | FFmpeg native | 35.89 | 27.86 | 17.91 | 0.289 |
| SRPG10 | Prototype, surfaces returned | 25.56 | 39.12 | 7.09 | 0.246 |
| SRPG10 | FFmpeg native | 24.71 | 40.47 | 17.63 | 0.184 |

Median per-round cycle gaps for the returned surface were 5.03% and 4.25%. Dropping prototype surfaces without returning them increased requested traffic to 267.2 MB and 229.8 MB per clip. The earlier deferred packing prototype increased allocation traffic in recycled mode. Emitting packed output directly into the result vector cut about one allocation call per frame and raised median paired cycles 0.73% and 1.41% on the bundled clips. [Owned-output candidate comparisons](results/windows-x86_64-20261126/owned-output-paired.csv)

The diagnostic profile after slice specialization attributed about 7.6% and 11.5% of measured ticks to skip macroblock overhead outside the motion and prediction kernels. [Specialized decoder profile](results/windows-x86_64-20261126/specialized-decoder-profile.csv)

The automatic-release prototype puts the buffers back in the decoder's pool when the last reference or display owner drops them. The two release modes produced identical allocation totals, and the prototype matched FFmpeg on the two bundled clips, temporal-direct multislice and MBAFF, constrained CAVLC and the resolution change. Across 150 frames it requested 47.8 MB and 32.4 MB, 5.3% and 11.9% above native FFmpeg. The dropped-surface mode used 36.63 and 25.65 Mcycles/frame, or 27.30 and 38.99 frames per billion cycles. Its median paired cycle gaps were 5.48% and 3.55%. [Automatic surface pool samples](results/windows-x86_64-20261126/surface-drop-prototype)

## Full-pel B-skip motion setup

`c270f4779` resolves motion and published rows once per reference for eligible full-pel 4:2:0 B-skip macroblocks. It applies the same implicit blend to each plane. FFmpeg similarly prepares a motion-compensation direction once and then predicts all three planes. The Rust change saved 0.66% and 1.24% of median paired cycles on the bundled clips. The cropped 720p clip saw a 0.58% increase in the same comparison. [FFmpeg motion compensation](https://github.com/FFmpeg/FFmpeg/blob/1041abdc962f4cc4f394aa8de9dc5236c0c3b9e7/libavcodec/h264_mb.c) · [Paired samples](results/windows-x86_64-20261126/crop-bskip-paired.csv)

The full run including the crop and B-skip changes matched FFmpeg in all five output modes and reproduced the previous allocation totals. The default cycle medians were 0.94% and 1.01% above packed FFmpeg. The five-run spreads were 2.2–4.7% across those modes. A larger paired run is needed to verify the cycle gap at the 1% threshold. [Full run](results/windows-x86_64-20261126/crop-and-bskip)

| Clip | Output mode | Mcycles/frame | Frames/billion cycles | Allocation calls/frame | Requested MiB/frame |
|---|---|---:|---:|---:|---:|
| SRPG9 | Deadlib | 36.64 | 27.30 | 11.71 | 3.272 |
| SRPG9 | FFmpeg native | 33.27 | 30.05 | 17.91 | 0.289 |
| SRPG9 | FFmpeg packed | 36.29 | 27.55 | 18.91 | 3.255 |
| SRPG10 | Deadlib | 27.95 | 35.78 | 9.33 | 3.158 |
| SRPG10 | FFmpeg native | 23.90 | 41.85 | 17.63 | 0.184 |
| SRPG10 | FFmpeg packed | 27.67 | 36.14 | 18.63 | 3.151 |

The early-delivery surface prototype emits a ready surface before reserving space for the next picture. A consumer can drop the surface during the callback so its buffer is available for the next decode. This reclaimed one coded picture on each clip, and emitted planes still matched FFmpeg. Two allocation runs requested 44.62 MB on SRPG9 and 29.24 MB on SRPG10, 1.75% below and 0.84% above native FFmpeg. [Early-delivery allocation samples](results/windows-x86_64-20261126/surface-early-allocations.csv)

In five rotating cycle rounds, early delivery used 35.19 and 24.68 Mcycles/frame, equivalent to 28.41 and 40.52 frames per billion cycles. Native FFmpeg used 33.85 and 24.13 Mcycles/frame. The median of the per-round cycle gaps was 4.82% on SRPG9 and 1.97% on SRPG10. [Early-delivery cycle samples](results/windows-x86_64-20261126/surface-early-cycles.csv)

With the full-pel B-skip change applied to early delivery, the median paired gaps were +4.06% and +0.91% against native FFmpeg over eight rounds. The prototype delivered 28.34 and 39.85 frames per billion cycles. Native FFmpeg delivered 29.33 and 39.94. Both runs matched the reference pixel hashes. [Early delivery with full-pel B-skip](results/windows-x86_64-20261126/surface-early-bskip-cycles.csv)

A separate surface decoder with its own reorder queue produced 38.93 and 24.15 Mcycles/frame, or 25.69 and 41.41 frames per billion cycles. Its median paired cycle gaps against native FFmpeg were +6.17% and ?0.47%, with a wide range on SRPG9. The prototype requested 44.62 MB and 29.24 MB across the 150 frames, using 1,188 and 913 allocation or reallocation calls. Native FFmpeg used 45.41 MB and 29.00 MB with 2,687 and 2,644 calls. Pixel hashes matched across the two bundled clips, temporal-direct progressive and MBAFF multislice, constrained CAVLC and the resolution change. [Separate surface cycle samples](results/windows-x86_64-20261126/surface-isolated-cycles.csv) ? [Allocation samples](results/windows-x86_64-20261126/surface-isolated-allocations.csv)

A picture-owned surface prototype retained the decoded picture directly and reclaimed its motion and plane buffers on last release. It reduced median paired cycles 2.01% and 3.61% from the separate-plane prototype. Its median gaps against native FFmpeg were +3.12% and ?2.28%. The prototype requested 44.62 MB and 26.60 MB, with 1,109 and 815 allocation or reallocation calls. The native FFmpeg reference requested 45.41 MB and 29.00 MB with 2,687 and 2,644 calls. Both native prototypes also matched FFmpeg on the High 10, weighted High 4:2:2 and cropped fixtures. [Picture-owned cycle samples](results/windows-x86_64-20261126/picture-owner-surface-cycles.csv) ? [Allocation samples](results/windows-x86_64-20261126/picture-owner-surface-allocations.csv)

## Horizontal quarter-pel interpolation

An SSSE3 prototype used overlapping row loads and aligned byte shifts to filter a horizontal quarter-pel block. FFmpeg also uses aligned byte shifts to reuse the input window for its horizontal six-tap filter. The prototype matched FFmpeg on both bundled clips, temporal-direct progressive and MBAFF multislice, constrained CAVLC and the resolution change. [FFmpeg horizontal quarter-pel filter](https://github.com/FFmpeg/FFmpeg/blob/1041abdc962f4cc4f394aa8de9dc5236c0c3b9e7/libavcodec/x86/h264_qpel_8bit.asm)

Across 12 rotating paired rounds its median cycle changes relative to `c270f4779` were +0.08% and ?0.78%, with mixed results across runs. The existing decoder's median gaps against packed FFmpeg were +0.58% and +0.08%. Distribution-free median intervals with 96.1% coverage span ?1.51% to +1.93% and ?2.84% to +3.29%, respectively. The intervals assume independent rounds. The prototype stays out of the decoder. [Paired cycle samples](results/windows-x86_64-20261126/horizontal-qpel-paired.csv)

## Deblocking metadata and skip motion

The compact metadata candidate stored block-zero motion alongside the deblocking flags and appended per-block detail for nonuniform macroblocks to reusable storage. The skip-motion candidate reused the result of the B-skip uniform-motion check during deblocking setup. Both candidates matched the reference on the bundled clips, temporal-direct progressive and MBAFF multislice, constrained CAVLC and the resolution change. The compact metadata candidate also matched the High 10, High 4:2:2 and cropped fixtures.

| Candidate versus `c270f4779` | SRPG9 median paired cycle change | SRPG10 median paired cycle change |
|---|---:|---:|
| Compact deblocking metadata | +1.66% | +1.56% |
| Reused B-skip uniform-motion check | +1.23% | ?0.41% |

The eight-round cycle comparison showed a regression on SRPG9 for both candidates. [Paired cycle samples](results/windows-x86_64-20261126/deblock-and-skip-paired.csv)

A third candidate used the representatives of the four 8x8 groups to check uniform B-skip motion where inference gives all four blocks in a group the same result. Median paired changes were ?0.20% on SRPG9 and +0.65% on SRPG10. It matched the six reference fixtures and stayed out of the decoder. [Grouped B-skip samples](results/windows-x86_64-20261126/grouped-b-skip-paired.csv)

## Deblocking schedule and threshold trials

FFmpeg filters completed macroblock rows and uses a quantization-parameter bound to skip filtering when all relevant alpha or beta thresholds are zero. The row-local Rust prototype filtered the first slice as each successor row completed. The threshold prototype checked the current, left and top macroblocks before deriving strengths. Both matched the bundled clips and the additional progressive, MBAFF, constrained CAVLC, High 10 and High 4:2:2 fixtures. [FFmpeg slice filtering and thresholds](https://github.com/FFmpeg/FFmpeg/blob/1041abdc962f4cc4f394aa8de9dc5236c0c3b9e7/libavcodec/h264_slice.c)

| Candidate versus `c270f4779` | SRPG9 median paired cycle change | SRPG10 median paired cycle change |
|---|---:|---:|
| Row-local filtering, logical CPU 2 | +1.00% | +0.15% |
| Low-QP filter skip, logical CPU 14 | +0.55% | ?0.59% |
| Packed output from the separate-surface build, logical CPU 2 | ?0.38% | +0.40% |

The low-QP prototype also matched a 48-frame fixture with explicit deblocking and chroma QP offsets. Its median paired cycle change on that fixture was +1.59%. Each comparison used eight rotating rounds. [Row and packed-output samples](results/windows-x86_64-20261126/row-local-deblock-paired.csv) ? [Low-QP samples](results/windows-x86_64-20261126/low-qp-deblock-paired.csv)

## Full-pel P-skip prediction

`37a17faed` copies in-bounds, unweighted full-pel 4:2:0 predictions directly into the current picture. It checks the reference position and published rows once for all three planes. Explicit P-slice weights qualify when they leave every plane unchanged. The decoder matched FFmpeg on both bundled clips, temporal-direct multislice and MBAFF, constrained CAVLC, the resolution change and the low-QP fixture.

| Candidate versus `c270f4779` | SRPG9 median paired cycle change | SRPG10 median paired cycle change |
|---|---:|---:|
| Direct full-pel P-skip | ?2.50% | ?4.87% |
| Row-bounded filter schedule | +1.73% | ?0.34% |
| Row-based filter coordinates | +0.08% | +0.13% |

The P-skip build used 36.68 and 27.28 Mcycles/frame, yielding 27.26 and 36.66 frames per billion cycles. Packed FFmpeg used 36.69 and 28.71 Mcycles/frame. Median paired gaps were ?0.24% and ?4.48% across 10 rotating rounds on logical CPU 14. The SRPG10 samples varied substantially across rounds. [Paired cycle samples](results/windows-x86_64-20261126/p-skip-and-row-paired.csv)
