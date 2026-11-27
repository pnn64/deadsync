# HEVC decoder benchmark

Run the shared harness from a Visual Studio x64 developer shell with Rust,
FFprobe and Python on the path. The ITGmania checkout supplies native FFmpeg.

```powershell
.\crates\deadlib-video-mpeg2\bench\measure.ps1 -ItgmaniaRoot ..\itgmania -Codec hevc -Cpu 4
```

The harness feeds the same in-memory HEVC configuration record and padded
picture packets to both decoders. The inputs are Happy Century's `bg.mp4`
and the offline `main.mp4` fixture. It checks visible YCbCr SHA-256 hashes,
alternates decoder order on a pinned thread and uses FFmpeg's native
single-threaded HEVC decoder. Each cycle sample for the short fixture
repeats the full decode 256 times. The allocation pass counts heap calls,
including reallocations, and requested bytes. Throughput is frames per
billion measured thread cycles.

## Baseline

The decoder at `40ce030a5` used 14.2% more cycles on Happy Century and
16.2% more on the offline fixture by paired median. On Happy Century it
requested 568 MB and made 474.58 heap calls per frame. FFmpeg requested
5.50 MB and made 18.06 heap calls per frame. The runs used a Xeon E5-2696 v4,
Rust 1.100.0 and ITGmania FFmpeg `1041abd` with libavcodec 62.28.103.
[Baseline samples](results/windows-x86_64-20261127/baseline/)

## Optimized decoder

The default release build at `a1d855295` was compared with the same native
FFmpeg build. The cycle and throughput changes are medians of paired decoder
ratios. Both runs used the same binaries and each allocation pass measured
three complete decodes per clip.

| Input | Logical CPU | Cycle pairs | Thread cycles vs FFmpeg | Throughput vs FFmpeg |
| --- | ---: | ---: | ---: | ---: |
| Happy Century (4,198 frames) | 6 | 9 | +0.40% | −0.40% |
| Happy Century (4,198 frames) | 10 | 7 | −1.13% | +1.15% |
| Main fixture (12 frames) | 6 | 9 | −4.71% | +4.95% |
| Main fixture (12 frames) | 10 | 7 | −4.61% | +4.83% |

| Input | Deadlib heap calls/frame | FFmpeg heap calls/frame | Deadlib requested bytes | FFmpeg requested bytes |
| --- | ---: | ---: | ---: | ---: |
| Happy Century | 11.51 | 18.06 | 4,468,263 | 5,501,078 |
| Main fixture | 13.00 | 34.50 | 177,878 | 243,787 |

Every visible YCbCr sample matched FFmpeg for both benchmark inputs. Happy
Century also matched an independent FFmpeg 7.1 decode through the public MP4
and Annex B APIs. The public MP4 API matched FFmpeg 7.1 on a separate generated
24-frame Main 10 clip.

[CPU 6 samples](results/windows-x86_64-20261127/final-cpu6/)
[CPU 10 samples](results/windows-x86_64-20261127/final-cpu10/)

## Decoder changes

- Share decoded planes with output and reuse picture, coefficient, and NAL
  scratch storage across packets. Keep common slice reference lists inline.
  [FFmpeg picture references](https://github.com/FFmpeg/FFmpeg/blob/1041abdc962f4cc4f394aa8de9dc5236c0c3b9e7/libavcodec/hevc/refs.c)
- Keep merge candidates on the stack and stop once the requested candidate
  is available.
  [FFmpeg motion vectors](https://github.com/FFmpeg/FFmpeg/blob/1041abdc962f4cc4f394aa8de9dc5236c0c3b9e7/libavcodec/hevc/mvs.c)
- Use paired byte and word taps for x86 motion filters, vectorize SAO edge
  classification, and extend motion-reference borders by copying rows.
  [FFmpeg motion filters](https://github.com/FFmpeg/FFmpeg/blob/1041abdc962f4cc4f394aa8de9dc5236c0c3b9e7/libavcodec/x86/hevc/mc.asm)
  [FFmpeg SAO](https://github.com/FFmpeg/FFmpeg/blob/1041abdc962f4cc4f394aa8de9dc5236c0c3b9e7/libavcodec/x86/hevc/sao.asm)
  [FFmpeg edge extension](https://github.com/FFmpeg/FFmpeg/blob/1041abdc962f4cc4f394aa8de9dc5236c0c3b9e7/libavcodec/videodsp_template.c)
- Refill CABAC with a big-endian word, normalize the range through a lookup
  table, and select bypass results without branching on the decoded bit.
  Precompute coefficient significance contexts in coefficient scan order.
  [FFmpeg x86 CABAC](https://github.com/FFmpeg/FFmpeg/blob/1041abdc962f4cc4f394aa8de9dc5236c0c3b9e7/libavcodec/x86/cabac.h)
