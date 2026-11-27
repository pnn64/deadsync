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
