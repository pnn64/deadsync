# MPEG-2 decoder benchmark

Run from a Visual Studio x64 developer shell with Rust, FFmpeg and Python on
the path. Point the script at an ITGmania checkout with its bundled x64
libavcodec 62 headers, import libraries and DLLs.

```powershell
.\crates\deadlib-video-mpeg2\bench\measure.ps1 -ItgmaniaRoot ..\itgmania
```

The harness extracts the MPEG-2 elementary stream from each full ITGmania
fixture, `dix.mpg` and `lights.mpg`. A SHA-256 check covers the visible Y, Cb and
Cr bytes of every frame in display order. FFmpeg uses its native MPEG-2 decoder
and the 8-bit Simple IDCT.

Both decoders parse identical in-memory bytes on one pinned Windows thread.
The measured region includes decoder setup, parsing, reconstruction, drain and
destruction of the native output. `QueryThreadCycleTime` supplies the cycle
counts. Decoder order alternates in each round. Throughput is frames per
billion measured cycles. A separate pass counts successful allocation and
reallocation requests and their requested bytes. The FFmpeg allocation counter
calibrates its CRT hooks before taking measurements. [Windows thread cycles](https://learn.microsoft.com/en-us/windows/win32/api/realtimeapiset/nf-realtimeapiset-querythreadcycletime)

## Optimizations

FFmpeg checks motion bounds per block and dispatches to packed half-pixel
kernels. Deadlib uses SSE2 kernels for the four half-pixel positions and extends
the source into a small tile for boundary blocks. [FFmpeg motion code](https://github.com/FFmpeg/FFmpeg/blob/1041abdc962f4cc4f394aa8de9dc5236c0c3b9e7/libavcodec/mpegvideo_motion.c) · [FFmpeg x86 kernel dispatch](https://github.com/FFmpeg/FFmpeg/blob/1041abdc962f4cc4f394aa8de9dc5236c0c3b9e7/libavcodec/x86/hpeldsp_init.c)

FFmpeg recycles its reference-counted plane buffers. Deadlib reuses the three
planes of a retired frame when its decoder owns the last strong reference.
Callers can retain frames independently. [FFmpeg frame pools](https://github.com/FFmpeg/FFmpeg/blob/1041abdc962f4cc4f394aa8de9dc5236c0c3b9e7/libavcodec/get_buffer.c)

FFmpeg combines run and level in its coefficient lookup and reconstructs
8-bit blocks with packed fixed-point arithmetic. Deadlib's short table also
resolves the sign in the same lookup. Its SSE2 transform preserves Simple IDCT
rounding and uses a separate path when only the first and last frequency rows
contain nonzero coefficients. That case covered 82% of coded blocks on
`dix.mpg` and 77% on `lights.mpg`. [FFmpeg coefficient decode](https://github.com/FFmpeg/FFmpeg/blob/1041abdc962f4cc4f394aa8de9dc5236c0c3b9e7/libavcodec/mpeg12dec.c) · [FFmpeg x86 Simple IDCT](https://github.com/FFmpeg/FFmpeg/blob/1041abdc962f4cc4f394aa8de9dc5236c0c3b9e7/libavcodec/x86/simple_idct10.asm)

## Verification

The measurements used an Intel Xeon E5-2696 v4, Rust 1.100.0 and ITGmania FFmpeg
`1041abd` with libavcodec 62.28.103. The original decoder used 7.21 and 7.42
million cycles per frame on `dix.mpg` and `lights.mpg`. The final decoder
matched every visible YCbCr sample across all 865 and 1,553 frames.

| Clip | Decoder | Million cycles/frame | Frames/billion cycles | Heap calls/frame | Requested KiB/frame |
|---|---|---:|---:|---:|---:|
| `dix.mpg` | Deadlib | 1.122 | 891.5 | 1.010 | 1.316 |
| `dix.mpg` | FFmpeg | 1.340 | 746.3 | 18.682 | 6.129 |
| `lights.mpg` | Deadlib | 1.820 | 549.5 | 1.006 | 0.838 |
| `lights.mpg` | FFmpeg | 2.104 | 475.2 | 18.658 | 12.747 |

The table uses the eleven-pair run on logical CPU 4. The run on logical CPU 2
used nine pairs. Across both runs the maximum paired Deadlib/FFmpeg cycle ratio
was 0.866 on `dix.mpg` and 0.893 on `lights.mpg`. All six allocation repetitions
had the same counts and requested bytes on each clip. [CPU 4 measurements](results/windows-x86_64-20261126/cpu4/) · [CPU 2 measurements](results/windows-x86_64-20261126/cpu2/)
