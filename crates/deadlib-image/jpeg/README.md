# deadlib-image-jpeg

Independent, pure-Rust JPEG-1 codec under active development. The sole
production dependency is the shared `deadlib-image` crate. No DeadSync loader
uses this package. The full requested decoder/encoder scope remains in PLAN.md;
COVERAGE.md records the outstanding standards and performance work.

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

`decode_lossless(&jpeg, limits)` decodes predictive Huffman SOF3 or arithmetic SOF11 into
native unsigned sample planes at every precision from 2 through 16 bits.
All seven predictors, per-scan point transforms, whole-row restart intervals,
interleaved/separate scans, subsampling and up to 255 frame components are
supported. At Pt=0 the samples are exact source integers; Pt>0 reconstructs
`(source >> Pt) << Pt`. The `coding` records retain each component's predictor
and Pt. APPn/COM and ICC views use the same borrowed metadata representation.
`LosslessImage::into_planar` moves its buffers into the common color interface
without copying samples. Shared output keeps 8-bit values and scales other
precisions to 16-bit storage; use the native planes for exact original values.
See LOSSLESS-VALIDATION.md for reference gates and remaining lossless work.

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

`LosslessOptions::entropy_coding = EntropyCoding::Arithmetic` writes SOF11
using the native Annex H two-dimensional model. DAC L/U bounds use
`arithmetic_conditioning`; the AC split is unused. Four decoder destinations,
158 bins per table and left/above difference classes are supported. The class
history is bounded to one row per vertical sampling phase. Arithmetic scans
share the same precision, sampling, scripts, metadata and restart controls.
The JPEG XT reference gate and its documented setup/output limitations are in
LOSSLESS-VALIDATION.md.

`decode_hierarchical(&jpeg, limits)` parses DHP/EXP and all six differential
JPEG-1 frame processes. It preserves every original frame's coefficients,
quantizers or native sample differences and global metadata/scan positions.
`reconstruct_reference()` composes raw modulo-65536 component planes with the
ordered Annex J expansion filter; `reconstruct()` provides the shared planar
color interface. Initial frames may use any supported ordinary process.
`HierarchicalImage::from_frames` constructs a validated native hierarchy without
decoding a JPEG. `encode(HierarchicalOptions::default())` writes its frames into
one bounded output buffer, preserving native values and each frame's entropy
family and sequential/progressive choice. Per-frame `HierarchicalFrameOptions`
select Huffman/arithmetic, fixed/optimized tables, generated/custom scans,
refinement, signed residual Pt and restarts. Metadata/ICC controls use
`encode_with_metadata`. DCT frames currently accept 1..4 components; lossless
frames support 255.
Exact independent frame-data gates and the reference's raster limitations are
documented in HIERARCHICAL-VALIDATION.md.

`build_hierarchical_planes` and `build_hierarchical_samples` construct source
pyramids with matching reconstructed references. `PyramidOptions::new(&levels)`
borrows per-frame `PyramidLevel::dct(expansion, quality, coding)` or
`PyramidLevel::lossless(expansion, coding)` descriptors. Supply final source
dimensions; preceding dimensions follow the expansion flags backwards. The
first frame has no expansion. Repeated resolutions and independent horizontal
or vertical expansion are supported. DCT stages may end with a same-resolution
lossless correction; Pt=0 reproduces the sampled source exactly with this
codec's matching IDCT. Pure lossless Pt=0 preserves source integers directly.

The `encode_hierarchical_planes`, `encode_hierarchical_samples` and shared
`encode_hierarchical_image` conveniences use the same bounded frame writer;
their metadata variants add APPn/COM and preserve/remove/replace ICC profiles.
`PyramidRasterOptions::default()` keeps source precision, color space and
component integers with 4:4:4 sampling. Precision scaling, RGB-to-YCbCr,
CMYK-to-YCCK and subsampling require explicit selection. Unchanged CMYK values
retain the declared polarity. No hierarchical output is labeled as JFIF.
See HIERARCHICAL-VALIDATION.md for the chosen reduction filter, progressive/Pt
reference handling, memory behavior and independent source-encoding evidence.

All frame decoders support first-scan DNL, including initially zero or
provisional SOF heights. Models return resolved dimensions; `MarkerReader`
exposes the original declarations unchanged. Hierarchies can also omit the
completed DHP height, which is inferred from full-resolution components.
Set `height_declaration: HeightDeclaration::Dnl { initial_height: 0 }` in
`CodingOptions` or `LosslessOptions` to emit DNL. Per-frame hierarchy and pyramid
controls share these options; `omit_dhp_height` selects zero-height DHP.
Defaults retain ordinary SOF/DHP output. DNL-VALIDATION.md records all-process
checks, independent C-authored inputs and explicit reference limitations.

`encode_coefficients(&coefficients, CodingOptions::default())` rewrites a JPEG
without another lossy transform. It preserves quantizer values, visible blocks,
and APPn/COM payloads. Controls select sequential or progressive
output, optimized or fixed Huffman tables, restart intervals, and an output
byte budget. The initial writer accepts one to four components. Metadata keeps
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

Typed `jfif`, `adobe`, `exif`, `icc_profile` and segment-level `jfxx` views are
available. The `encode_*_with_metadata` functions accept borrowed APPn/COM
additions and ICC preserve/remove/replace controls. Profiles up to 255 chunks
are supported. See METADATA-VALIDATION.md for APIs, bounds and reference gates.

Malformed tables, runs, scan layouts, padding and restart sequences are errors.
Limits bound pixels, coefficient/sample storage, metadata (including segment
record overhead), and scan count. These are storage budgets, not a measured
allocator high-water mark; allocation instrumentation remains planned.
Bytes after EOI are left unconsumed. Unknown SOF modes return explicit
unsupported errors. Full DCT component-count coverage,
Abbreviated streams, incremental I/O, additional encoder scan strategies, and the broader validation
and performance work remain required.

Run `cargo test -p deadlib-image-jpeg` and
`cargo clippy -p deadlib-image-jpeg --all-targets -- -D warnings`.
The checked-in sequential and progressive oracle fixtures run without a C compiler or reference
library. To regenerate them, see tests/oracle/README.md.

The current correctness-first IDCT uses separable Q20 cosine factors with i64
accumulators, four fractional bits between passes, explicit sample clipping,
and a DC-only shortcut. The pinned corpus differs by at most one native sample
level from libjpeg-turbo's float IDCT across 50 fixtures; this observed bound is
not yet a general
conformance bound for all legal quantizers/blocks. Pixel/color conversion,
broader conformance, libjpeg/mozjpeg parity and performance characterization
remain open. No throughput claim is made yet.
See ENCODER-VALIDATION.md for external encoder checks and measured size/quality
results, including their numerical bounds and limits.
COLOR-VALIDATION.md describes CMYK/YCCK conversion, exact rational chroma
siting, profile/polarity policies and the 486-case color reference gate.
CMYK-ENCODER-VALIDATION.md covers source-polarity and CMYK-to-YCCK encoding.
ARITHMETIC-VALIDATION.md covers sequential/progressive arithmetic decoding and reference gates.
PROGRESSIVE-ENCODER-VALIDATION.md covers refinement/custom scan generation and
the external scan-script matrix.
