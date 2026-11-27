# deadlib-video-hevc

Standalone H.265/HEVC decoding with zero Cargo dependencies. The library
uses the Rust standard library and defaults to one decode thread.

Use `decode_mp4` for a complete indexed `hvc1` or `hev1` MP4 and
`decode_annex_b` for an elementary stream. Both emit pictures in display
order, as cropped Y, Cb and Cr planes. For packets from another demuxer,
create a `Decoder` with `with_display_order`, feed its `hvcC` record with
`push_decoder_config`, and pass the returned length-field size to
`push_packet`. Drain the last pictures with `flush`.

Use `decode_mp4_pictures`, `decode_annex_b_pictures`, or a decoder's
`push_packet_pictures` and `flush_picture` to share the reference-plane
buffers. A `Picture` can outlive the decoder and its visible rows exclude
stride padding. Use `Picture::to_owned_frame` when contiguous owned
samples are needed.

```sh
cargo run --release --example decode -- bg.mp4 bg.yuv
cargo run --release --example decode -- bg.hevc bg.yuv
cargo test --locked --offline
```

The example writes 8-bit planes as bytes and wider planes as little-endian
16-bit samples. MP4 output contains the video track's pictures in display
order, with container timing and audio handled separately.

## Verification

The ITGmania clip `[recognize m.] happy century/video/bg.mp4` contains
4,198 frames of HEVC Main at 256 × 256 in an `hvc1` track. Its original
MP4 and the corresponding Annex B stream both match FFmpeg 7.1 on every
visible Y, Cb and Cr sample. The raw output SHA-256 is
`c0e8150a08e2941881060fbd2839ecae40f171e5d7ca7acc9b4d96051ef3801d`.

The offline test fixtures contain the same twelve synthetic 82 × 50
frames in indexed MP4 and Annex B. They exercise cropped planes, B-frame
reordering and end-of-stream draining.

```sh
ffmpeg -f lavfi -i testsrc2=size=82x50:rate=12 -frames:v 12 -c:v libx265 -threads 1 -preset fast -tag:v hvc1 -x265-params "pools=none:frame-threads=1:wpp=0:keyint=8:min-keyint=8:bframes=2:b-adapt=0:scenecut=0:log-level=error" -movflags +faststart main.mp4
ffmpeg -i main.mp4 -map 0:v:0 -c:v copy -bsf:v hevc_mp4toannexb -f hevc main.hevc
```

The decoding core is adapted from `rust_h265`. Its MIT and Apache-2.0
license texts are included. The Deadlib port adds packet and configuration
handling, indexed MP4 input and offline display-order tests.
[Upstream revision](https://github.com/roticv/rust_h265/tree/b822f95d23248a1996f65491a0c16d2ba2e18e45)
