# deadlib-video-vp9

Standalone, dependency-free VP9 video decoding in Rust. The crate uses
`no_std` with `alloc`. It exposes cropped planar Y, U and V samples, stored
as bytes for 8-bit video and as 16-bit integers for 10- or 12-bit video.

Use `decode_mp4` for a VP9 track in a complete indexed or fragmented MP4
file. The reader handles 32- and 64-bit chunk offsets and fragments with
explicit or movie-fragment-relative data offsets. It emits decoded pictures
in VP9 display order. For IVF or another container, feed complete packets in
bitstream order to `Decoder::push_packet`. `ivf::IvfReader` exposes packets
from IVF files.

```sh
cargo run --release --example decode -- Countdown.mp4 Countdown.yuv
cargo run --release --example decode -- video.ivf video.yuv
cargo test --locked --offline
```

On x86-64 the decoder selects AVX2 reconstruction when the processor and
operating system support it. Other builds use the portable reconstruction
path. A build targeting AVX2 directly can use `RUSTFLAGS="-C target-feature=+avx2"`.

## Verification

Both VP9 Profile 0 clips from the installed Million Dollar Baby song folder
matched FFmpeg 7.1 on every visible Y, U and V sample when read from the
original fragmented MP4 files.

| Clip | Dimensions | Frames |
| --- | --- | ---: |
| `Countdown.mp4` | 1164 × 720 | 426 |
| `Film.mp4` | 1280 × 720 | 603 |

The offline fixtures were generated from FFmpeg's
`testsrc2=size=82x50:rate=12:duration=0.75` with
`-c:v libvpx-vp9 -threads 1 -g 4 -crf 34 -b:v 0 -lag-in-frames 0`.
The fragmented file uses `-movflags +frag_keyframe+empty_moov+default_base_moof`.
The indexed fixture remuxes the same packets with `-c:v copy -movflags +faststart`.

The VP9 decoding core is adapted from the MIT-licensed `vp9dec`, with a serial
`no_std` library, a packet API, and an MP4 reader. [Upstream revision](https://github.com/ChanyaVRC/vp9dec/tree/abc22e90b00aff2f708c5524e78566ccb74b0203) · [VP9 MP4 binding](https://www.webmproject.org/vp9/mp4/)
