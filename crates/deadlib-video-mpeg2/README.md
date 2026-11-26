# deadlib-video-mpeg2

A standalone, dependency-free MPEG-2 Video (H.262) decoder for Rust. The library
uses `no_std` with `alloc`.

The decoder reads frame-predicted, progressive 4:2:0 I, P and B pictures and
returns visible 8-bit Y, Cb and Cr rows in display order. It accepts MPEG-2 video
elementary streams and extracts the first video stream from MPEG Program Streams.
Audio, container timestamps, field coding, other chroma formats and encoding are
outside the current scope.

```rust
use deadlib_video_mpeg2::decode_program_stream;

let input = std::fs::read("background.mpg")?;
decode_program_stream(&input, |frame| {
    let y = &frame.planes()[0];
    println!("{}×{} luma plane", y.width(), y.height());
})?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

Use `decode_elementary_stream` for raw `.m2v` files. For incremental decoding,
feed complete start-code units to `Decoder::push` and drain with `Decoder::finish`.

```sh
cargo run --release --example decode -- ps background.mpg background.yuv
cargo test --offline
```

The integer inverse DCT uses the 8-bit Simple IDCT rounding. H.262 allows
different inverse DCT implementations within its accuracy limits, so use
`-idct simple` when comparing bytes against FFmpeg. [ITU-T H.262](https://www.itu.int/rec/T-REC-H.262/en)

With FFmpeg 7.1, all visible YCbCr samples match for the offline test fixture and
both ITGmania fixtures (`dix.mpg` and `lights.mpg`). The offline test fixture was
generated with FFmpeg using
`testsrc2=size=48x34:rate=25:duration=0.32` and
`-c:v mpeg2video -g 12 -bf 2 -q:v 4 -non_linear_quant 1 -qmax 28`.
