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
verification is mandatory. Out-of-range palette indices are retained, and
`to_rgba16` / APNG rendering display them as opaque black (PNG Third Edition
13.1). Set `DecodeOptions::palette_policy` to `PalettePolicy::Strict` to reject
them while decoding. Both encoders always reject out-of-range indices. Other
invalid critical data is an error, unknown critical
chunks are rejected, and unknown ancillary chunks are retained with placement.
On reencoding, unrecognized ancillary chunks with the unsafe-to-copy flag are
omitted because critical image chunks are rewritten (PNG Third Edition 14.2).
PNG Third Edition's recommended tolerance for trailing bytes inside final IDAT
is implemented. Bytes after IEND are rejected by this whole-datastream API.

Typed metadata is available through `Png::metadata`; `Metadata::to_chunk` encodes
values. ICC container headers, tag bounds and PNG color-space compatibility,
and Exif TIFF headers / first-directory framing are checked. Profile transforms,
Exif tag meanings and thumbnails remain opaque. Unsupported containers decode as
unknown raw ancillary metadata; encoders reject them. No color management is
applied implicitly. `color_description` skips unsupported ICC containers; the
fallible `color_description_with_limits` preserves resource errors instead of
silently selecting a lower-precedence description.
Typed iTXt follows WHATWG UTF-8 decoding, consuming one leading BOM and replacing
malformed sequences; raw chunk bytes remain unchanged. Encoders require valid
UTF-8 and method 0 even for uncompressed iTXt. BCP47 syntax, duplicate variants /
extensions, IANA subtag registration and extlang prefixes are checked. Encoder
registration uses a bundled offline snapshot (`LANGUAGE_SUBTAG_REGISTRY_DATE`).
Well-formed tags unknown to that snapshot decode as `Metadata::Unknown`, keeping
their raw chunks for future registry versions. Refreshing the tables requires
an explicit development script; decoding and encoding perform no network access.
Encoding zeroes unused tRNS/bKGD sample bits and rejects hIST zero frequencies
for palette indices used by the default image or animation frames.

See [COVERAGE.md](COVERAGE.md) for evidence and current limitations. Broader
conformance and competitive performance validation remain under development.
The [clause audit](CONFORMANCE.md) records supported behavior and remaining gates.
This crate is registered in the workspace but is not used by DeadSync loaders.

APNG frame images retain their original formats. `Animation::default_is_frame`
distinguishes the fallback image from animation content; `Frame::image == None`
references the default allocation for the first animation frame. Optional
`Png::compositor` and `Png::to_rgba16` explicitly produce RGBA16 without automatic
gamma/ICC/HDR conversion. The compositor supports SOURCE/OVER and all disposal
operations. Animation timing and repetition are controlled by the caller.

The [external libpng oracle](tests/oracle/README.md) independently validates
native samples and stays outside the production dependency graph.

`zlib::Inflater` accepts arbitrary input/output slices through `process`, returns
exact consumed/written counts and `NeedInput`/`NeedOutput`/`Done`, and owns a
bounded 32 KiB history ring. Only `Done` proves a complete checksum-verified zlib
stream. `finish` reports truncated input; errors remain terminal. The caller
retains any bytes after the completed stream. See `tests/inflate_stream.rs` for
driving examples and zero-capacity boundary cases.

`StreamDecoder::process` emits borrowed metadata, palette/transparency, APNG
controls and native sample rows. Plain rows use pass 0; Adam7 rows use passes 1–7
and expose their x/y/step coordinates within the current image or frame rectangle.
Only `ImageEnd` verifies the current zlib stream and its data-chunk CRCs; rows
before it are provisional. `Done` verifies the complete PNG grammar. Push parsing
leaves bytes after IEND untouched, while `decode_reader(reader, &options)` collects
final images and rejects trailing bytes like `decode`. The row pipeline keeps a
bounded number of rows and a 32 KiB history, without complete compressed or
filtered-image buffers. See `tests/png_stream.rs` for fragmented IO and events.

`zlib::Deflater` is the matching bounded encoder: pass input/output fragments to
`process`, mark final input with `finish=true`, and drain to `Done`. It retains
32 KiB input blocks and cross-block LZ77 history. `encode_writer(&png, &options,
writer)` filters native rows and emits PNG/APNG chunks directly to `Write`,
handling partial and Interrupted writes. It returns byte and image-buffer
statistics; the caller controls flushing. Compressed chunks are capped at
64 KiB while honoring smaller `idat_chunk_size` values. Encoding retains the
caller's images and metadata without whole filtered/compressed output buffers.

`scripts/verify_independent.ps1` copies only the two packages into a separate
temporary workspace and runs offline package tests plus the production dependency
tree. `independent-build.log` records the latest verified run.
