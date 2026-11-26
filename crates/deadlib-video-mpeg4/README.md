# deadlib-video-mpeg4

Standalone, dependency-free MPEG-4 Part 2 video decoding in Rust. The library
uses `no_std` with `alloc` and exposes decoded 8-bit Y, Cb and Cr planes.
[MPEG-4 Visual standard](https://www.iso.org/standard/39259.html)

The decoder supports progressive rectangular 4:2:0 I and P pictures using
H.263 quantization. It handles AC prediction, video packets, and one or four
half-pixel motion vectors per macroblock.

Use `decode_elementary_stream` for raw `.m4v` bytes, or `decode_avi` for MPEG-4
Visual in RIFF AVI. For incremental decoding, feed complete start-code units
to `Decoder::push`. OpenITG's `logo.mpg` contains Xvid video in an AVI container.

```sh
cargo run --release --example decode -- avi logo.mpg logo.yuv
cargo run --release --example decode -- es video.m4v video.yuv
cargo test --offline
```

The AVI file and its extracted elementary stream matched FFmpeg 7.1 with
`-idct simple` on all 900 frames of `logo.mpg`. The offline test fixture uses
FFmpeg's `testsrc2=size=160x90:rate=25:duration=0.4` with
`-c:v mpeg4 -threads 1 -flags +mv4 -ps 160 -g 5 -bf 0 -q:v 5 -f m4v`.
