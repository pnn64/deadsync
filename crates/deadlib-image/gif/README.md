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
