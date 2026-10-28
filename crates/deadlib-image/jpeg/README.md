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
grid. CMYK/YCCK planes retain their encoded values, including Adobe inversion;
shared RGB conversion for these spaces is still pending. Shared-model output
scales 12-bit samples to the full 16-bit range. ICC application is pending;
metadata bytes are preserved without modification.

Malformed tables, runs, scan layouts, padding and restart sequences are errors.
Limits bound pixels, coefficient/sample storage, metadata (including segment
record overhead), and scan count. These are storage budgets, not a measured
allocator high-water mark; allocation instrumentation remains planned.
Bytes after EOI are left unconsumed. Unknown SOF modes return explicit
unsupported errors. Arithmetic, predictive lossless, hierarchy,
DNL, streaming, and encoding remain required work.

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
