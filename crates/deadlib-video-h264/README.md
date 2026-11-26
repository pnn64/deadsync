# deadlib-video-h264

Standalone, decode-only H.264/AVC software codec. Builds with Cargo offline and has no Rust dependencies, native library links, or runtime tool requirements.

Use `nal::parse_annex_b` for a raw H.264 bytestream. For a demuxed MP4 track, parse the `avcC` record with `nal::parse_avcc_config`, feed its SPS and PPS NAL units to the decoder, then use `nal::parse_avcc` on each sample with the record's NAL length size. The caller supplies the demuxed bytes and any timestamps. `OrderedDecoder::decode_nal_with_meta` carries those timestamps through frame reordering.

```no_run
use deadlib_video_h264::{nal::parse_annex_b, OrderedDecoder};

let bitstream = std::fs::read("input.h264")?;
let mut decoder = OrderedDecoder::new();
for nal in parse_annex_b(&bitstream) {
    for frame in decoder.decode_nal(&nal)? {
        println!("{} × {}", frame.width, frame.height);
    }
}
for frame in decoder.flush() {
    println!("{} × {}", frame.width, frame.height);
}
# Ok::<(), Box<dyn std::error::Error>>(())
```

`OrderedDecoder` emits planar YUV frames in display order. `Decoder` emits them in decode order. Frames contain cropped, tightly packed Y, U and V planes. The `std` feature enables `threading::ThreadedDecoder`. Disabling default features builds with `no_std` and `alloc`.

`SurfaceDecoder`, available with the `surface` feature, delivers display-ordered pictures to a callback. Borrow the visible rows through `SurfaceFrame::plane` to read the coded buffers directly. The surfaces can be retained after the callback or after the decoder is dropped. Releasing a surface in the callback lets the decoder reuse buffers as soon as its reference pictures release them.

The decoder supports Baseline, Main, High, High 10 and High 4:2:2 streams with CAVLC or CABAC. It handles I, P and B pictures, 8- or 10-bit 4:2:0, 4:2:2 and monochrome, and progressive and interlaced streams. Unsupported tools include 4:4:4 chroma, lossless transform bypass, SP/SI slices and flexible macroblock ordering. Monochrome frames have neutral chroma planes. Ten-bit plane samples are little-endian 16-bit values.

The decoder core comes from [`rust_h264` 0.6.1](https://github.com/roticv/rust_h264/tree/64363352b2796624da4d86b7b4ee3ff2ba9f69e7), copyright roticv, under the included MIT or Apache-2.0 licenses. This crate vendors the decoder sources, removes profiling and nightly-only tooling, and uses local reference clips for integration tests.
