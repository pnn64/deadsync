# VP9 decoder benchmark

Run the shared harness from a Visual Studio x64 developer shell with Rust,
FFprobe and Python on the path. Use the ITGmania checkout with its native
x64 FFmpeg headers, libraries and DLLs.

```powershell
.\crates\deadlib-video-mpeg2\bench\measure.ps1 -ItgmaniaRoot ..\itgmania -Codec vp9 -Cpu 4
.\crates\deadlib-video-mpeg2\bench\measure.ps1 -ItgmaniaRoot ..\itgmania -Codec vp9 -Cpu 4 -Avx2 -Results target/vp9-avx2
```

Both decoders receive identical in-memory picture packets from
`Countdown.mp4`, `Film.mp4`, and the offline `indexed.mp4` fixture. The
harness checks each clip's visible YCbCr SHA-256 before timing and alternates
decoder order on a pinned thread. It measures CPU cycles and frames per
billion cycles, with a separate pass for allocation calls and requested bytes.
The sample extraction and pixel hashing run outside the measurements.
