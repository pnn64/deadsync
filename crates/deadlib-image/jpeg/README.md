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

`decode_lossless(&jpeg, limits)` decodes predictive Huffman SOF3 directly into
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
unsupported errors. Predictive lossless encoding and arithmetic lossless, hierarchy,
DNL, streaming, additional encoder scan strategies, and the broader validation
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
