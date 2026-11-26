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
