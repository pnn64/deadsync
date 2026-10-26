# deadlib-image-gif

Independent pure-Rust GIF87a/GIF89a codec. GIF parsing, sub-block framing, LZW,
interlacing and indexed encoding are implemented locally in safe Rust. The only
production dependency is `deadlib-image` for explicit shared-image conversion.
The crate is registered in the workspace and is not used by DeadSync loaders.

```rust
use deadlib_image_gif::{decode, encode, EncodeOptions};
# fn example(bytes: &[u8]) -> Result<(), deadlib_image_gif::Error> {
let gif = decode(bytes)?;
for frame in gif.frames() {
    // Row-major original palette indices, including transparent pixels.
    let indices = &frame.indices;
    let palette = frame.palette(&gif.header);
}
let bytes = encode(&gif, &EncodeOptions::default())?;
# Ok(())
# }
```

Build and test with `cargo test -p deadlib-image-gif` or
`cargo test --manifest-path crates/deadlib-image/gif/Cargo.toml`.
`cargo tree -p deadlib-image-gif --edges normal` audits production dependencies.

`Decoder::update` accepts any input fragment and returns its consumed prefix
and at most one event. Submit the unconsumed suffix again after every event.
`Decoder::finish` verifies receipt of the trailer. Frames can be returned through
`recycle_frame` to reuse their allocation. The LZW dictionary references the
already decoded image, avoiding a separate prefix-chain expansion buffer.
`Encoder<W: Write>` writes one block at a time and reuses compression buffers.

Resource limits apply cumulatively to input, metadata (including sub-block
storage overhead), graphics, frame count and block count. Dimensions are checked
before allocating pixels. Erroring streaming decoders and writers stay failed.
No RGBA quantization or implicit lossy conversion is included.

The implementation milestones and remaining validation work are in [PLAN.md](PLAN.md).

`Compositor::new(&gif.header)` uses the specification's logical-screen background
color. `CompositeOptions` can select a transparent canvas or a supplied RGBA
background instead. `render` borrows the reusable RGBA8 canvas until the next
call. Transparent indices leave existing pixels untouched. Background disposal
uses the configured background; previous disposal saves only the affected
rectangle. Unspecified (0) and reserved (4..7) disposal retain existing pixels.
All reserved disposal values are preserved in the indexed model.

`render_block` ignores non-graphic extensions. Plain-text graphics require
`render_text` with a `TextRasterizer`, because GIF does not prescribe a font.
The codec lays out cells, applies transparency, clipping, timing and disposal;
the rasterizer supplies a cell mask. Partial cells are discarded, extra text is
ignored, unused grid pixels use the text background, and characters outside
0x20..=0xf7 are passed to the font as spaces (GIF89a's recommendation).

Graphic controls attach to the next image or plain-text graphic, across comments
and application extensions. Encoding normalizes a control to immediately before
its target. Meaningful extension payloads, sub-block boundaries and their order
relative to graphics survive roundtrips. Byte-for-byte compressed representation,
reserved packed bits and unused descriptor size bits are not preserved.

Loop extensions are retained as ordinary application data. `loop_count` recognizes
NETSCAPE2.0 and ANIMEXTS1.0, using the first recognized instruction; zero means
infinite and other values count repetitions after the initial playback. Absence
means a single playback. Delays are centiseconds with no zero-delay minimum;
playback scheduling and user-input waits are the caller's responsibility.

Default decoding checks reserved fields, version declarations, duplicate/dangling
controls and frame bounds. `strict: false` permits mislabeled GIF87a streams,
reserved packed bits, last-control-wins duplicates and dangling controls.
`check_bounds: false` separately permits oversized rectangles, which rendering
clips. Missing palettes are retained in raw output: supply an external palette
from an earlier stream for rendering, or select a background and use local image
palettes. Missing colors are never fabricated. EOI is mandatory, and output size
must match exactly; bytes after LZW EOI inside image sub-blocks are ignored as a
compatibility policy. Bytes after the GIF trailer are rejected by whole-file
decoding; incremental callers can stop at the End event for embedded streams.
