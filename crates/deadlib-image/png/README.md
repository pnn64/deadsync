# deadlib-image-png

Independent pure-Rust PNG implementation. The only production dependency is
`deadlib-image` for native sample storage. PNG framing, CRC-32, Adler-32,
zlib/DEFLATE, filters and Adam7 are implemented locally in Rust with checked
buffer accesses. On x86-64, runtime feature checks select BMI2 inflation,
PCLMUL CRC folding, SSSE3 Adler sums and SSE4.1 Average/Paeth reconstruction
for three-, four-, six- and eight-byte strides. Their dispatch modules use `unsafe` for CPU
feature contracts; byte reads and writes use checked slices and fixed-size arrays.

```rust
use deadlib_image_png::decode;
# fn example(bytes: &[u8]) -> Result<(), deadlib_image_png::Error> {
let png = decode(bytes)?;
// png.image.samples retains U8 or U16 precision, original channel count,
// unpacked sub-byte values, and original palette indices.
# Ok(())
# }
```

Build/test independently using `cargo test -p deadlib-image-png` or
`cargo test --manifest-path crates/deadlib-image/png/Cargo.toml`. Development
dependencies supply independent Rust png and flate2 reference implementations.
`cargo tree -p deadlib-image-png --edges normal` verifies the production graph.

The separate [benchmark workspace](bench/README.md) measures decoding and encoding
CPU cycles, throughput and allocation requests against Rust png and image.
Decoder measurements check native samples and RGBA16 pixels first; encoder
measurements verify opaque RGBA8 pixels and report the encoded file size.

`encode` returns an owned PNG byte vector. `encode_writer` writes PNG/APNG to
`std::io::Write` using bounded row, compression and chunk buffers. Select the
optional zero-run compressor explicitly:

```rust
use deadlib_image_png::{EncodeOptions, Filter, Png, encode_writer};
use deadlib_image_png::zlib::Compression;

fn write_png(
    png: &Png,
    destination: impl std::io::Write,
) -> Result<(), deadlib_image_png::Error> {
    let options = EncodeOptions {
        compression: Compression::Rle,
        filter: Filter::Adaptive,
        ..Default::default()
    };
    encode_writer(png, &options, destination)?;
    Ok(())
}
```

`Compression::Rle` encodes zero runs in filtered bytes with distance-one DEFLATE
matches and an analytic Huffman tree. It keeps no match dictionary and selects
stored blocks when smaller. The same options work with `encode`.
`EncodeOptions::default()` remains `Compression::Fast`, `Filter::Adaptive`,
noninterlaced output and a 64 KiB IDAT chunk size. Fast/Adaptive uses zero-run
coding with a stored fallback for PNG and APNG samples up to 8 bits. At 16
bits it keeps fixed-Huffman LZ77 and tries only the most recent match. Fast
with an explicit fixed filter uses the chained LZ77 search. All compression
modes preserve
the supplied samples; their encoded sizes and CPU costs depend on the input.

For reference color samples with 1–16 significant bits per channel, use
`Png::from_reference_samples(width, height, color_type, channel_depths, samples)`.
For example, RGB565 channels use `[5, 6, 5]`; RGBA1010102 uses `[10, 10, 10, 2]`.
The constructor selects the smallest legal PNG depth, applies exact integer
linear scaling, and records sBIT when needed. Shifting decoded samples right
by `PNG depth - original channel depth` recovers every original value. Alpha
zero/maximum remain exact. Both U8/U16 input containers are accepted; matching
containers are reused in place. Gray, gray/alpha, RGB and RGBA channel order is
preserved. Palette indices retain the native `Png::new` contract and are never
scaled as color values. The existing native encode/decode paths keep their
stored-sample contract.

`decode_with_options` exposes explicit input/output/metadata/chunk limits and
`DecodeOptions::metadata_policy`. The default `MetadataPolicy::Recover` discards
optional metadata with an invalid CRC, malformed payload, invalid placement or
duplicate singleton/name. Singleton metadata keeps the first accepted value;
the first CRC-valid iCCP in an accepted position reserves the profile slot
even when its payload is malformed. A later valid profile cannot replace it.
Exif after IDAT is accepted and retains its original `ChunkPosition::AfterData`.
These rules cover the known Exif/ICC compatibility cases for arbitrary input
files; they do not claim identical handling of every malformed PNG by other
decoders.

Set `DecodeOptions::metadata_policy` to `MetadataPolicy::Strict` to reject
metadata CRC, ordering, duplicate and parsing errors. Both policies still check
critical chunk CRCs, zlib checksums and image framing. Transparency (`tRNS`) and
APNG validation keep their existing handling; `AnimationPolicy` independently
controls animation recovery. Resource limits remain fatal under either metadata
policy. Discarded metadata is omitted from the raw and typed metadata results.

Out-of-range palette indices are retained, and
`to_rgba16` / APNG rendering display them as opaque black (PNG Third Edition
13.1). Set `DecodeOptions::palette_policy` to `PalettePolicy::Strict` to reject
them while decoding. Both encoders always reject out-of-range indices. Other
invalid critical data is an error, unknown critical
chunks are rejected, and unknown ancillary chunks are retained with placement.

`Limits::max_input_bytes` bounds PNG input bytes; `max_chunks` counts every chunk,
including IHDR, IEND and empty data chunks. `max_output_bytes` bounds cumulative
native sample storage across the default image and separately stored animation
frames, and each image's filtered inflation. A default image reused as frame zero
is counted once. Filter bytes can make the inflation bound larger than the final
sample vector. The metadata budget counts raw ancillary payloads, including
discarded metadata, plus expanded compressed payloads. Expansion produced before
a metadata parsing or checksum failure also counts. PLTE/tRNS and animation
controls have their separate format bounds. Frame limits apply to declared and
observed animation frames. Resource errors remain fatal under metadata and
animation recovery. These caps describe data
budgets, not a process RSS limit. `tests/decoder_boundaries.rs` checks exact and
adjacent boundaries through buffered, collecting-reader and push APIs.

Both `encode` and `encode_writer` enforce strict metadata validation and ordering.
To reencode a decoded PNG with late Exif, explicitly move its retained eXIf chunk
from `ChunkPosition::AfterData` to `ChunkPosition::BeforeData`. Decoding preserves
the original placement instead of silently rewriting the file's metadata.
On reencoding, unrecognized ancillary chunks with the unsafe-to-copy flag are
omitted because critical image chunks are rewritten (PNG Third Edition 14.2).
PNG Third Edition's recommended tolerance for trailing bytes inside final IDAT
is implemented. Bytes after IEND are rejected by this whole-datastream API.

Typed metadata is available through `Png::metadata`; `Metadata::to_chunk` encodes
values. ICC container headers, tag bounds and PNG color-space compatibility,
and Exif TIFF headers / first-directory framing are checked. Profile transforms,
Exif tag meanings and thumbnails remain opaque. Unsupported containers decode as
unknown raw ancillary metadata under both metadata policies; this is distinct
from malformed chunk framing or compression. Encoders reject unsupported
containers. No color management is
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
before it are provisional. `Done` completes PNG validation under the selected
metadata and animation policies. Push parsing
leaves bytes after IEND untouched, while `decode_reader(reader, &options)` collects
final images and rejects trailing bytes like `decode`. The row pipeline keeps a
bounded number of rows and a 32 KiB history, without complete compressed or
filtered-image buffers. See `tests/png_stream.rs` for fragmented IO and events.

`zlib::Deflater` is the matching bounded encoder: pass input/output fragments to
`process`, mark final input with `finish=true`, and drain to `Done`. It retains
a 32 KiB input block; `Fast` also retains cross-block LZ77 history and a match
dictionary. `Stored` and `Rle` need neither. `encode_writer(&png, &options,
writer)` filters native rows and emits PNG/APNG chunks directly to `Write`,
handling partial and Interrupted writes. Filter scratch depends on row width;
compression and chunk buffers are bounded independently of image height.
Compressed chunks are capped at 64 KiB while honoring smaller
`idat_chunk_size` values. `EncodeStats::peak_image_buffer_bytes` reports image
pipeline storage, excluding the caller's images, metadata validation temporaries
and the destination's buffers. A destination `Vec` accumulates the complete
PNG. The caller controls flushing.
