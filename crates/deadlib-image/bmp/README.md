# deadlib-image-bmp

A pure Rust BMP decoder and encoder with **zero Cargo dependencies**, including
development dependencies. It uses `std`, forbids unsafe code, and owns its pixel
model so it can be used independently of `deadlib-image` or the application.

```rust
use deadlib_image_bmp::{decode, encode};

# fn example(bytes: &[u8]) -> Result<(), deadlib_image_bmp::Error> {
let bitmap = decode(bytes)?;
let encoded = encode(&bitmap)?;
let rgba = bitmap.into_rgba8()?;
# let _ = (rgba, encoded);
# Ok(())
# }
```

## Supported files

Files must have a `BM` file header. Detection is independent of the filename or
extension. All decoded rows are top-to-bottom, left-to-right.

| DIB header | Supported pixels | Compression |
| --- | --- | --- |
| OS/2 core, 12 bytes | Indexed 1/4/8-bit, BGR24 | Uncompressed |
| Windows info, 40 bytes | Indexed 1/4/8-bit, RGB555, BGR24, BGRA32 | `BI_RGB`; `BI_BITFIELDS` for 16/32-bit with three external RGB masks |
| Windows V2/V3, 52/56 bytes | Same Windows depths | `BI_RGB`; `BI_BITFIELDS` with embedded RGB/RGBA masks |
| Windows V4/V5, 108/124 bytes | Same Windows depths | `BI_RGB`; `BI_BITFIELDS` with embedded RGB/RGBA masks |

Both bottom-up and top-down Windows images are supported, including odd widths,
DWORD row padding, and gaps before the pixel array. RGB masks must be nonzero,
contiguous, disjoint, and contained in the pixel word; alpha is optional and must
obey the same constraints when present. Channel masks can contain one to eight
bits. Expansion repeats the original bits, matching ITGmania's STB loader.

RLE4/RLE8, embedded JPEG/PNG, `BI_ALPHABITFIELDS`, OS/2 v2 headers, standalone DIBs,
and channels wider than eight bits return an unsupported-feature error. These
are outside the normal BMP subset accepted by ITGmania's active STB loader. The
52-byte V2 header is a small additional supported header variant. This codec does
not apply V4/V5 color-space transforms, gamma, or ICC profiles, matching the
application's existing BMP interpretation.

## Pixel and compatibility policies

- Indexed images retain one unpacked byte per index and an opaque RGB palette.
  Palette entries are read as BGR triples for OS/2 or BGR plus an ignored reserved
  byte for Windows. Stored palette capacity comes from the pixel offset, capped
  by the indexed depth; unreliable `biClrUsed` values do not hide stored colors.
  An index beyond the available palette is an error. Unused packed tail bits and
  row padding are ignored as pixels.
- `BI_RGB16` uses RGB555. For V4/V5 headers, an embedded alpha mask is retained,
  including all-zero transparency, matching ITGmania's legacy interpretation.
- `BI_RGB32` uses BGRA. If every pixel's alpha byte is zero, the image is made
  opaque; otherwise every alpha value is preserved. This handles legacy files
  whose fourth byte was unused without discarding actual transparency.
- `BI_BITFIELDS` without an alpha mask is opaque. With an explicit alpha mask,
  all alpha values are preserved, even when the whole image is transparent.
- `bfSize` and `biSizeImage` are advisory. Actual source bounds determine whether
  a file is complete. Every pixel row, including its padding, must be present.
  Trailing bytes are permitted within the input limit.
- Color-keying belongs to texture preparation and is not applied by the codec.

Some STB implementation quirks are intentionally corrected: core palettes start
at byte 26 (valid minimal palettes work); V3 masks are read from their defined
positions inside the header; and `bfOffBits` is applied exactly once. The decoder
rejects missing palette indices, overlapping masks, and truncated input instead
of reproducing undefined pixels or zero-filling missing bytes.

## API and bounds

`decode` / `decode_with_limits` return `Bitmap { width, height, pixels }`.
`Pixels` is `Indexed`, `Rgb`, or `Rgba`; 16/32-bit files without an alpha mask
produce RGB. `Bitmap::validate` checks caller-created images before conversion
or encoding. `to_rgba8` / `to_rgba8_with_limits` expand palettes or add opaque
alpha as needed.

When the bitmap is no longer needed, `into_rgba8` / `into_rgba8_with_limits`
consume it and reuse an existing RGBA sample buffer without allocating or
copying. RGB and indexed input use the same expansion as the borrowed methods.
Both variants validate public models and apply the same pixel/output limits;
consuming RGBA preserves the buffer's existing spare capacity.

`decode_rgba8` / `decode_rgba8_with_limits` return the source `Info` and owned,
packed RGBA samples in top-down order. RGB24 goes directly from BGR to RGBA
in one traversal. The decoded byte limit applies to the RGBA raster.

`probe` / `probe_with_limits` inspect dimensions, source depth, and orientation
without allocating or requiring palette/pixel bytes. They require a complete
DIB and any external masks, validate header structure, and enforce the same
estimated decoded-storage limits. Successful probing does not certify that the
pixel data is present or valid.

Default decode limits are 256 MiB of input, 67,108,864 pixels (`64 * 1024 * 1024`),
and 256 MiB of decoded storage. Indexed storage includes
indices plus three bytes per retained palette entry. RGBA conversion checks the
RGBA buffer length; the source bitmap is already owned.
Limits are configurable. Dimension, pitch, and allocation arithmetic is checked,
and output buffers use fallible reservation. These are per-operation output
bounds, not a process-wide memory budget.

`encode` / `encode_with_options` write deterministic BMP bytes:

- RGB and indexed input becomes a 24-bit, 40-byte-header BMP. Indexed colors are
  expanded directly into the output without a temporary full-color image.
- RGBA becomes a 32-bit V4 BMP with explicit RGBA masks and an sRGB color-space
  marker, preserving even entirely transparent pixels.
- `EncodeOptions` selects bottom-up (default) or top-down row order and a maximum
  encoded size (256 MiB by default). Padding is zeroed and dimensions must fit
  the signed Windows header fields.

The crate does not depend on the application's image loader. Connecting it to
runtime loading and removing the existing BMP fallback are separate migration
steps.

## Performance measurements

The isolated [benchmark workspace](bench/README.md) compares exact output with
`image` and measures thread cycles, throughput, and allocations without adding
dependencies to this crate. The [recorded optimization results](bench/results/windows-x86_64-20261120/README.md)
include per-change baselines, complete final comparisons, observed regressions,
and behavior-preservation checks.

## Validation and references

The integration tests construct independent BMP byte streams and expected pixels
for the supported layouts, orientation, padding, palettes, masks, alpha behavior,
invalid/truncated input, exact limit boundaries, and encoder headers/pixel bytes.
They do not download assets or pull in reference codecs.

The compatibility baseline is ITGmania's `RageSurface_Load_BMP.cpp`,
`RageSurface_Load_STB.cpp`, and `extern/stb/stb_image.h`, together with DeadSync's
captured BMP fixtures. Reference comparisons can run outside this crate so its
dependency graph remains empty.

- [Microsoft BITMAPFILEHEADER](https://learn.microsoft.com/en-us/windows/win32/api/wingdi/ns-wingdi-bitmapfileheader)
- [Microsoft BITMAPINFOHEADER](https://learn.microsoft.com/en-us/windows/win32/api/wingdi/ns-wingdi-bitmapinfoheader)
- [Microsoft BITMAPV4HEADER](https://learn.microsoft.com/en-us/windows/win32/api/wingdi/ns-wingdi-bitmapv4header)
- [ITGmania BMP loader](https://github.com/itgmania/itgmania/blob/release/src/RageSurface_Load_BMP.cpp)
- [ITGmania STB implementation](https://github.com/itgmania/itgmania/blob/release/extern/stb/stb_image.h)
