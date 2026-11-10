# deadlib-image-jpeg

Currently implemented: native marker parsing, canonical Huffman decoding with
a nine-bit lookup, sequential SOF0/SOF1 and progressive SOF2 coefficient
reconstruction, 8/12-bit
integer IDCT, restart intervals, interleaved and separate component scans,
component subsampling, borrowed APPn/COM preservation, native component planes,
grayscale/RGB/YCbCr interpretation and centered chroma interpolation.
Progressive decoding includes spectral selection, DC/AC successive
approximation, EOB runs, restart boundaries, and scan-order validation. Partial
progressions retain the precision available for each band.
Sequential SOF9 and progressive SOF10 arithmetic decoding use a native Annex D
binary coder, Annex F/G coefficient contexts, DAC conditioning and restart
resets. Progressive arithmetic includes spectral bands, DC/AC refinement and
per-block end-of-band history. Arithmetic input can be rewritten as Huffman
JPEG without changing its coefficients when the coefficients fit the target
Huffman process's magnitude categories. Arithmetic decisions use the full
signed 16-bit model at either sample precision.

`encode_lossless(&shared_image, LosslessOptions::default())` preserves shared
gray/RGB integers at their original 2..16-bit precision. The native
`encode_lossless_samples` API also accepts interleaved YCbCr, CMYK and YCCK.
It performs no color transform, inversion, downsampling or precision scaling.
CMYK polarity is explicit; RGB/YCbCr/YCCK use Adobe hints and no JFIF header is
generated. Use `LosslessImage::from_planes(...).encode(...)` to retain arbitrary
component IDs, sampling and up to 255 components. Decoded lossless images can
also be rewritten directly without another sample buffer. Options select all
seven predictors, Pt, fixed/optimized Huffman tables, separate component scans,
whole MCU rows between restarts and borrowed `LosslessScan` scripts. Explicit
scans can vary predictor/Pt/restart rows and visit components in any order;
each individual scan follows frame order. Generated scans pack legal MCUs and
split larger layouts automatically. The metadata variants preserve APPn/COM,
add segments, and preserve/remove/replace ICC bytes through the same writer.

`build_hierarchical_planes` and `build_hierarchical_samples` construct source
pyramids with matching reconstructed references. `PyramidOptions::new(&levels)`
borrows per-frame `PyramidLevel::dct(expansion, quality, coding)` or
`PyramidLevel::lossless(expansion, coding)` descriptors. Supply final source
dimensions; preceding dimensions follow the expansion flags backwards. The
first frame has no expansion. Repeated resolutions and independent horizontal
or vertical expansion are supported. DCT stages may end with a same-resolution
lossless correction; Pt=0 reproduces the sampled source exactly with this
codec's matching IDCT. Pure lossless Pt=0 preserves source integers directly.

`Tables` owns persistent quantization and Huffman destinations for T.81 B.4/B.5
abbreviated streams. Load a table-only stream with `tables.load(bytes, limits)`,
then call its `decode_coefficients`, `decode_lossless` or `decode_hierarchical`
methods for successive images. Definitions in images update the same cache.
`consumed` stops at EOI, allowing concatenated streams. Arithmetic conditioning
and restart intervals reset at every SOI. Table-stream APPn/COM metadata is
returned separately and does not become metadata of later images.

`encode_coefficients(&coefficients, CodingOptions::default())` rewrites a JPEG
without another lossy transform. It preserves quantizer values, visible blocks,
and APPn/COM payloads. Controls select sequential or progressive
output, optimized or fixed Huffman tables, restart intervals, and an output
byte budget. Sequential output supports 1..255 components and progressive
output 1..4. Generated scans pack legal MCU groups and sequential scans can
reuse quantizer destinations after earlier components finish. Metadata keeps
its relative order; segments beyond the new last scan are emitted before EOI.
Progressive output defaults to full-precision spectral scans. Set
`successive_approximation` to an initial Al in 1..=13 to generate DC/AC
refinement passes down to Al=0. `scan_script` borrows caller `ScanSpec`
descriptors for custom component scans, spectral bands and precision order.
The descriptor uses frame component IDs; IDs within one scan follow frame
order, while separate scans may visit components in any order. Explicit
progressive scripts may intentionally transmit partial progressions. These
controls are available through both `CodingOptions<'a>` and the pixel/raster
`EncodeOptions<'a>` entry points.
`CodingOptions::entropy_coding` selects `EntropyCoding::Huffman` (default) or
native `EntropyCoding::Arithmetic`. Arithmetic output uses SOF9/SOF10, explicit
DAC tables, configurable `ArithmeticConditioning` and the same restart,
metadata, precision and sampling controls. `optimize_huffman` applies to
Huffman output. The arithmetic writer does not collect a Huffman histogram or
allocate a separate entropy buffer.

MOZJPEG-VALIDATION.md records the pinned mozjpeg source/rewrite comparisons
against all three ordinary-process C references, exact coefficient checks,
separate numerical tolerances and explicit reference-producer limitations.
The C tools remain test-only; broader conformance and performance work is open.

T83-VALIDATION.md covers the official ITU T.83 compressed streams and numeric
references, including the 255-component B1/B2 pair, exact lossless samples,
native FDCT/IDCT accuracy and C-checked coefficient output. The corpus and
independent f64 model are retained for offline Cargo tests; full conformance
acceptance audits and the standard's unavailable streams remain explicit gaps.

`encode(&shared_image, EncodeOptions::default())` encodes shared grayscale/RGB
images. Quality, output precision (8/12), subsampling, RGB-to-YCbCr conversion,
quantizer range and entropy coding are configurable. `encode_samples` accepts
native interleaved U8/U16 gray, RGB, YCbCr, CMYK and YCCK sources at 1–16-bit
precision, explicitly scaled to output precision. CMYK source polarity is
explicit (`cmyk_polarity`, default Adobe-complemented); `cmyk_to_ycck` enables
the native transform. Already encoded YCCK values pass through. `quantize` exposes the coefficient result before
entropy coding. Alpha/indexed conversion is explicit; grayscale is always 1x1
sampled. Color presets enlarge the first component and also K for YCCK; `Subsampling::Custom`
specifies every component's factors, including fractional ratios.
Explicit separate-component scripts support layouts whose global sampling
sum exceeds ten blocks, including YCCK 4:1:0. The ten-block restriction applies
to each actual interleaved scan.

```rust
use deadlib_image_jpeg::{decode_coefficients, Limits, Upsampling};
# fn example(jpeg: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
let coefficients = decode_coefficients(jpeg, Limits::default())?;
let native_planes = coefficients.reconstruct()?;
let image = native_planes.to_image(Upsampling::Linear)?;
image.validate()?;
# Ok(())
# }
```

Coefficient blocks and quantization tables use natural frequency order; padded
MCU blocks remain available. Each component snapshots its quantizer at scan
time. Component planes preserve their original integer precision and sampling
grid. CMYK/YCCK planes retain their encoded values. `to_cmyk` exposes native
CMYK with explicit polarity; `convert_polarity` changes convention in place.
Shared RGB conversion uses a documented uncalibrated subtractive approximation
and can write directly without an intermediate CMYK raster. Shared-model output
scales 12-bit samples to the full 16-bit range. ICC profiles can be borrowed,
assembled or replaced; their original bytes are exposed for caller color
management. Metadata bytes are preserved without modification by default.

Malformed tables, runs, scan layouts, padding and restart sequences are errors.
Limits bound pixels, coefficient/sample storage, metadata (including segment
record overhead), and scan count. These are storage budgets, not a measured
allocator high-water mark; allocation instrumentation remains planned.
Bytes after EOI are left unconsumed. Unknown SOF modes return explicit
unsupported errors. Incremental input, single-channel table-first paired output,
additional encoder scan strategies, and the broader validation
and performance work remain required.

Run `cargo test -p deadlib-image-jpeg` and
`cargo clippy -p deadlib-image-jpeg --all-targets -- -D warnings`.
The checked-in sequential and progressive oracle fixtures run without a C compiler or reference
library. To regenerate them, see tests/oracle/README.md.
