# deadlib-image-png

Independent pure-Rust PNG implementation. The only production dependency is
`deadlib-image` for native sample storage. PNG framing, CRC-32, Adler-32,
zlib/DEFLATE, filters and Adam7 are implemented locally in safe Rust.

```rust
use deadlib_image_png::{decode, encode, EncodeOptions};
# fn example(bytes: &[u8]) -> Result<(), deadlib_image_png::Error> {
let png = decode(bytes)?;
// png.image.samples retains U8 or U16 precision, original channel count,
// unpacked sub-byte values, and original palette indices.
let encoded = encode(&png, &EncodeOptions::default())?;
# Ok(())
# }
```

Build/test independently using `cargo test -p deadlib-image-png` or
`cargo test --manifest-path crates/deadlib-image/png/Cargo.toml`. Development
dependencies supply independent Rust png and flate2 reference implementations.
`cargo tree -p deadlib-image-png --edges normal` verifies the production graph.

`decode_with_options` exposes explicit input/output/metadata/chunk limits. CRC
verification is mandatory. Invalid critical data is an error, unknown critical
chunks are rejected, and unknown ancillary chunks are retained with placement.
On reencoding, unrecognized ancillary chunks with the unsafe-to-copy flag are
omitted because critical image chunks are rewritten (PNG Third Edition 14.2).
PNG Third Edition's recommended tolerance for trailing bytes inside final IDAT
is implemented. Bytes after IEND are rejected by this whole-datastream API.

Typed metadata is available through `Png::metadata`; `Metadata::to_chunk` encodes
values. ICC and Exif payloads remain opaque to this codec. No color management is
applied implicitly. Color-description precedence is exposed separately.

See [COVERAGE.md](COVERAGE.md) for evidence and current limitations. In particular,
APNG, streaming and performance validation remain under development.
This crate is registered in the workspace but is not used by DeadSync loaders.

APNG frame images retain their original formats. `Animation::default_is_frame`
distinguishes the fallback image from animation content; `Frame::image == None`
references the default allocation for the first animation frame. Optional
`Png::compositor` and `Png::to_rgba16` explicitly produce RGBA16 without automatic
gamma/ICC/HDR conversion. The compositor supports SOURCE/OVER and all disposal
operations. Animation timing and repetition are controlled by the caller.

The [external libpng oracle](tests/oracle/README.md) independently validates
native samples and stays outside the production dependency graph.
