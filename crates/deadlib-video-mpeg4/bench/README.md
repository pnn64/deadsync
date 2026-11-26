# MPEG-4 decoder benchmark

Run the shared harness from a Visual Studio x64 developer shell with Rust,
FFmpeg and Python on the path. The ITGmania checkout supplies FFmpeg's native
x64 libraries, headers and DLLs.

```powershell
.\crates\deadlib-video-mpeg2\bench\measure.ps1 -ItgmaniaRoot ..\itgmania -Codec mpeg4
```

The benchmark uses the full 900-frame Xvid clip extracted from OpenITG's
`logo.mpg` and the ten-frame `progressive.m4v` test with video packets and
four-vector motion. Both decoders receive identical in-memory elementary
streams. The harness checks the SHA-256 of the visible YCbCr pixels before each
cycle batch. FFmpeg uses its native MPEG-4 Visual decoder and the Simple IDCT.

Each cycle batch alternates decoder order on one pinned Windows thread.
Measurements include decoder setup, parsing, reconstruction, drain and release
of the native frames. Throughput is frames per billion measured thread cycles.
A separate pass counts successful allocations and reallocations and their
requested bytes. [Windows cycle counter](https://learn.microsoft.com/en-us/windows/win32/api/realtimeapiset/nf-realtimeapiset-querythreadcycletime)

## Optimizations

FFmpeg extends the picture edge into a small buffer and uses specialized
half-pixel kernels for rounded and truncated interpolation. Deadlib likewise
checks the source rectangle once and uses SSE2 to predict a row at a time.
[FFmpeg motion code](https://github.com/FFmpeg/FFmpeg/blob/1041abdc962f4cc4f394aa8de9dc5236c0c3b9e7/libavcodec/mpegvideo_motion.c) · [FFmpeg x86 half-pixel dispatch](https://github.com/FFmpeg/FFmpeg/blob/1041abdc962f4cc4f394aa8de9dc5236c0c3b9e7/libavcodec/x86/hpeldsp_init.c)

FFmpeg uses packed fixed-point inverse transforms. Deadlib vectorizes the same
two-stage arithmetic and evaluates only the contributing frequencies when the
middle rows are zero. When only the first frequency row is populated, the
vertical pass produces eight identical output rows. [FFmpeg x86 Simple IDCT](https://github.com/FFmpeg/FFmpeg/blob/1041abdc962f4cc4f394aa8de9dc5236c0c3b9e7/libavcodec/x86/simple_idct10.asm)

FFmpeg obtains planes from a reference-counted buffer pool and keeps predictor
storage across pictures. Deadlib reuses predictor buffers and recycles planes
when the decoder owns the last strong reference to a retired frame.
[FFmpeg frame pool](https://github.com/FFmpeg/FFmpeg/blob/1041abdc962f4cc4f394aa8de9dc5236c0c3b9e7/libavcodec/get_buffer.c) · [FFmpeg predictor storage](https://github.com/FFmpeg/FFmpeg/blob/1041abdc962f4cc4f394aa8de9dc5236c0c3b9e7/libavcodec/mpegvideo.c)

FFmpeg's inter coefficient lookup incorporates inverse quantization. Deadlib
resolves the sign and event in one lookup for short codes, then writes scaled
inter coefficients directly into the inverse transform's input.
[FFmpeg coefficient decoding](https://github.com/FFmpeg/FFmpeg/blob/1041abdc962f4cc4f394aa8de9dc5236c0c3b9e7/libavcodec/mpeg4videodec.c)

## Verification

The runs used an Intel Xeon E5-2696 v4, Rust 1.100.0 and ITGmania FFmpeg
`1041abd` with libavcodec 62.28.103. The initial decoder (`6493a6bae`) had median
paired cycle ratios of 2.574 on `logo.mpg` and 1.338 on the packetized fixture.
Frame and workspace reuse brought `logo.mpg` allocation traffic from 234.539
to 0.549 KiB per frame. Both final batches used decoder revision `150b679f1`.

| Clip | Decoder | Million cycles/frame | Frames/billion cycles | Heap calls/frame | Requested KiB/frame |
|---|---|---:|---:|---:|---:|
| `logo.mpg` | Deadlib | 0.858 | 1166.1 | 1.011 | 0.549 |
| `logo.mpg` | FFmpeg | 0.940 | 1064.1 | 10.110 | 7.112 |
| `progressive.m4v` | Deadlib | 0.199 | 5024.6 | 2.000 | 7.102 |
| `progressive.m4v` | FFmpeg | 0.249 | 4012.2 | 19.700 | 23.300 |

The table uses the eleven-pair run on logical CPU 4. The nine-pair run on
logical CPU 2 used the same decoder. All pairs cleared the cycle and throughput
targets, and all five allocation repetitions agreed on both clips. The largest
paired cycle ratios were 0.9432 on `logo.mpg` and 0.9915 on `progressive.m4v`.
The final decoder also matched FFmpeg on all 900 frames when decoding the
original AVI file directly.
[Baseline](results/windows-x86_64-20261126/baseline/) · [CPU 4 samples](results/windows-x86_64-20261126/cpu4/) · [CPU 2 samples](results/windows-x86_64-20261126/cpu2/)

## Microsoft MPEG-4 v2

```powershell
.\crates\deadlib-video-mpeg2\bench\measure.ps1 -ItgmaniaRoot ..\itgmania -Codec mp42 -SongsRoot C:/Games/ITGmania/Songs
```

The MP42 run uses `A-Short_People_Got_No_Reason_To_Live.avi` from UPS 3,
`banner.avi` from My Penny, and the offline `mp42.avi` fixture. Both decoders
receive the same in-memory packets extracted before measurement. The harness
passes FFmpeg refcounted input backed by those packets so it can retain data
without copying. It checks the SHA-256 of the visible planes, then measures
cycles and allocation traffic in separate passes.

The initial MP42 decoder (`93765622b`) used 22.9% more cycles than FFmpeg on
`banner.avi`. FFmpeg's x86 reconstruction uses saturated packed addition and
converts eight pixels at a time to unsigned bytes. Applying those operations
to Deadlib's output stores reduced the median banner cycle ratio to 1.0219
(`7bdf54103`). [FFmpeg x86 reconstruction](https://github.com/FFmpeg/FFmpeg/blob/1041abdc962f4cc4f394aa8de9dc5236c0c3b9e7/libavcodec/x86/simple_idct10.asm)

FFmpeg's MP42 DC tables return the signed coefficient difference in one lookup.
Deadlib now does the same for short codes with a 10-bit table and uses a
constant fill for intra blocks with neither coded nor predicted AC
(`61e621169`). [FFmpeg DC table construction](https://github.com/FFmpeg/FFmpeg/blob/1041abdc962f4cc4f394aa8de9dc5236c0c3b9e7/libavcodec/msmpeg4.c) · [FFmpeg MP42 decoding](https://github.com/FFmpeg/FFmpeg/blob/1041abdc962f4cc4f394aa8de9dc5236c0c3b9e7/libavcodec/msmpeg4dec.c)

| Clip | Decoder | Million cycles/frame | Frames/billion cycles | Heap calls/frame | Requested KiB/frame |
|---|---|---:|---:|---:|---:|
| Testament Of Axis | Deadlib | 0.237 | 4222.0 | 1.005 | 0.318 |
| Testament Of Axis | FFmpeg | 0.312 | 3203.6 | 8.038 | 0.524 |
| My Penny banner | Deadlib | 0.711 | 1406.9 | 1.145 | 15.268 |
| My Penny banner | FFmpeg | 0.825 | 1211.6 | 9.217 | 22.931 |
| `mp42.avi` | Deadlib | 0.141 | 7097.4 | 2.000 | 6.281 |
| `mp42.avi` | FFmpeg | 0.214 | 4668.8 | 16.400 | 18.756 |

These medians use the fifteen-pair run on logical CPU 4 of the Intel Xeon
E5-2696 v4, Rust 1.100.0, and ITGmania FFmpeg `1041abd`. FFmpeg uses one
thread and the Simple IDCT. Both binaries returned the same visible YCbCr
SHA-256 for each clip, covering 2,285 frames in total.

All 26 cycle pairs per clip across logical CPUs 4 and 2 cleared the 1%
cycle and throughput targets. The highest Deadlib-to-FFmpeg cycle ratio
was 0.9199 on `banner.avi`. The five allocation runs on each CPU agreed
and showed fewer heap calls and requested bytes on all three clips.

[Baseline](results/mp42-windows-x86_64-20261127/baseline/) · [SSE2 pixel stores](results/mp42-windows-x86_64-20261127/packed-store/) · [CPU 4 samples](results/mp42-windows-x86_64-20261127/verify-cpu4/) · [CPU 2 samples](results/mp42-windows-x86_64-20261127/verify-cpu2/)
