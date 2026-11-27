# VP9 decoder benchmark

Run the shared harness from a Visual Studio x64 developer shell with Rust,
FFprobe and Python on the path. Use the ITGmania checkout with its native
x64 FFmpeg headers, libraries and DLLs.

```powershell
.\crates\deadlib-video-mpeg2\bench\measure.ps1 -ItgmaniaRoot ..\itgmania -Codec vp9 -Cpu 4 -Results target/vp9-cpu4
.\crates\deadlib-video-mpeg2\bench\measure.ps1 -ItgmaniaRoot ..\itgmania -Codec vp9 -Cpu 6 -CycleRuns 7 -Results target/vp9-cpu6
```

Both decoders receive identical in-memory VP9 packets from `Countdown.mp4`
and `Film.mp4` in Cringle Culmination's Million Dollar Baby, and from the
offline `indexed.mp4` fixture. FFmpeg uses its native VP9 decoder with one
thread. Deadlib uses `Decoder::push_picture` to return shared pictures, and
the default build selects AVX2 at runtime on the benchmark machine.

The harness checks the SHA-256 of the visible YCbCr planes before measuring
and alternates decoder order on a pinned thread. Cycle samples include
decoder creation, full decode and release. Each `indexed.mp4` cycle sample
includes 256 independent decodes and the summary reports cycles per decode.
Throughput is frames per billion thread cycles. An independent pass counts
allocations plus reallocations and the total bytes requested from the heap.
Packet extraction and pixel hashing run outside both measurements.
[Windows thread cycle counter](https://learn.microsoft.com/en-us/windows/win32/api/realtimeapiset/nf-realtimeapiset-querythreadcycletime)

## Optimizations

FFmpeg shares reference-counted frames and pools their 8-bit pixel buffers.
Deadlib shares displayed pictures with its reference slots, recycles retired
planes and reconstructs 8-bit samples in byte planes. Compact motion-vector
metadata and reusable contexts further reduce heap traffic.
[FFmpeg frame pool](https://github.com/FFmpeg/FFmpeg/blob/1041abdc962f4cc4f394aa8de9dc5236c0c3b9e7/libavcodec/get_buffer.c)
· [FFmpeg reference metadata](https://github.com/FFmpeg/FFmpeg/blob/1041abdc962f4cc4f394aa8de9dc5236c0c3b9e7/libavcodec/vp9shared.h)

Sampling the short fixture identified coefficient decoding as its largest
cost. FFmpeg uses direct branches for the fixed coefficient token tree and
specialized reconstruction for 4×4 blocks. Deadlib uses the existing Pareto
probabilities with direct branches and keeps both small transform passes in
registers until reconstruction.
[FFmpeg coefficient decoding](https://github.com/FFmpeg/FFmpeg/blob/1041abdc962f4cc4f394aa8de9dc5236c0c3b9e7/libavcodec/vp9block.c)
· [FFmpeg x86 transforms](https://github.com/FFmpeg/FFmpeg/blob/1041abdc962f4cc4f394aa8de9dc5236c0c3b9e7/libavcodec/x86/vp9itxfm.asm)

FFmpeg replicates out-of-bounds samples in a small buffer for motion
compensation at the picture edge. Deadlib copies the visible portion of each
source row in bulk and fills the edge segments with their nearest sample.
Integer motion can copy these bytes straight into the reconstruction plane.
[FFmpeg motion compensation](https://github.com/FFmpeg/FFmpeg/blob/1041abdc962f4cc4f394aa8de9dc5236c0c3b9e7/libavcodec/vp9recon.c)

## Verification

The runs used an Intel Xeon E5-2696 v4, Rust 1.100.0, and ITGmania's FFmpeg
`1041abd` with libavcodec 62.28.103. The original decoder `51b59872c` used
5.36× and 6.47× FFmpeg's cycles on the two song clips in its scalar build.
With AVX2 forced it used about 24% fewer cycles, and requested 597 MB and
913 MB of heap memory while returning owned pictures. The byte-plane
milestone `c1a686d6e` reduced that traffic to 7.34 MB and 7.98 MB.
[Scalar baseline](results/windows-x86_64-20261127/baseline-portable/)
· [Forced AVX2 baseline](results/windows-x86_64-20261127/baseline-avx2/)
· [Byte-plane milestone](results/windows-x86_64-20261127/native-planes-cpu4/)

The final decoder `bbd6d91fb` matched all 1,038 frames. This table gives
per-decoder medians from nine pairs on logical CPU 4. Heap counts and
requested bytes agreed across all three allocation repetitions.

| Clip | Decoder | Million cycles/frame | Frames/billion cycles | Heap calls/frame | Requested KiB/frame |
|---|---|---:|---:|---:|---:|
| `Countdown.mp4` | Deadlib | 15.657 | 63.87 | 1.246 | 16.832 |
| `Countdown.mp4` | FFmpeg | 22.390 | 44.66 | 12.711 | 16.947 |
| `Film.mp4` | Deadlib | 10.136 | 98.66 | 1.189 | 12.923 |
| `Film.mp4` | FFmpeg | 13.733 | 72.82 | 12.735 | 13.193 |
| `indexed.mp4` | Deadlib | 0.2347 | 4,260.03 | 4.222 | 6.307 |
| `indexed.mp4` | FFmpeg | 0.2358 | 4,240.26 | 19.889 | 16.835 |

The seven-pair repeat on logical CPU 6 also met the 1% target. Median
paired Deadlib/FFmpeg cycle ratios were 0.7005, 0.7399 and 0.9961 on CPU 4
and 0.7091, 0.7681 and 0.9775 on CPU 6, in table order. The highest
individual `indexed.mp4` cycle ratio was 1.0212. Per-clip ratios and raw
measurements for both runs are in the archived CSV files.
[CPU 4 samples](results/windows-x86_64-20261127/cpu4/)
· [CPU 6 samples](results/windows-x86_64-20261127/cpu6/)

An additional 10-bit MP4 and the IVF form of the short fixture matched
FFmpeg's output byte for byte. The performance results cover 8-bit VP9 on
this single-threaded AVX2 setup.
