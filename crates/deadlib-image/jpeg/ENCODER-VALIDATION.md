# Native DCT encoder validation

The encoder is implemented in Rust with no production JPEG dependencies.
This is correctness evidence for the current implementation, not a completed
standards/performance audit. Full scope and gaps remain in PLAN.md/COVERAGE.md.

## Reproducible external gates

The pinned libjpeg-turbo 3.2.1 build and C client are described in
tests/oracle/README.md. The C client reads quantized coefficients and quantizers
directly, independent of its IDCT. It also emits raw planes with float IDCT and
full-resolution nearest-upsampled color output at native precision as LE u16.
Fractional sampling is checked through coefficients/raw planes because the
reference color upsampler does not implement those ratios.

```text
cargo build -p deadlib-image-jpeg --examples
target/debug/examples/encode_fixtures.exe crates/deadlib-image/jpeg/experiments/pixel-gate
python crates/deadlib-image/jpeg/tests/reference_pixels.py --oracle <jpeg-oracle> --cjpeg <cjpeg> --corpus crates/deadlib-image/jpeg/experiments/pixel-gate
```

The native generator constructs deterministic 31x25 images, quantizes them,
serializes coefficients **before entropy encoding**, and writes JPEGs. The C
decoder must recover those exact coefficients/tables without warnings. This
does not rely on a native encoder/decoder pair agreeing with each other.

Executed result: **172/172 exact coefficient/table matches, zero C decoder
warnings**. Cases include both precisions, quality 1/50/90/100, six common
sampling layouts, fractional horizontal/vertical ratios, RGB/YCbCr, grayscale,
raw encoded CMYK/YCCK, sequential/progressive scans, restart intervals, and
extended quantization tables. Hashes, byte sizes and per-case measurements are
retained in tests/reports/pixel-turbo.json.

## Transform and color tolerances

The forward transform is an orthonormal separable Q20 integer matrix. It
retains four fractional bits between passes and combines the final DCT scaling
and quantization division. Constant blocks use an exact DC shortcut. Scalar
mathematical DCT tests cover random 8/12-bit blocks with quantizer 1; integer
quantized coefficients differ by at most one level from the float definition.

The external generator also compresses the same grayscale and RGB PNM sources
with libjpeg-turbo's float DCT, matching quantizers and geometry. **132 cases**
pass this comparison. Maximum observed coefficient difference is **1** for
grayscale and retained RGB, and **2** for RGB converted to YCbCr. The latter
includes differences in chroma rounding and box-filter rounding.

The regression bound is 1 for identical gray/RGB source blocks. For YCbCr it
is 9: one source-sample level of rounding difference maps to at most 8 DCT
coefficient levels by the orthonormal 8x8 transform, plus one level for
transform/quantization rounding. The measured difference of 2 is substantially
smaller. Quantizers and block geometry must match exactly in every comparison.
This bound applies to these matched integer box filters and source precision;
it is not a tolerance for arbitrary preprocessing or color-managed output.

Hand-computed tests separately check fractional 3:2 area footprints, edge
replication, two-dimensional averaging, JFIF primary colors, and neutral
chroma. Constant grayscale extrema reconstruct exactly at both precisions
with quality 100. Lossy quality tests verify that raising quality from 10 to
95 improves squared error on a deterministic source.

## Size and reconstruction quality

The retained report includes compressed sizes, size ratios, squared error and
PSNR for native and reference JPEGs, decoded by the same C implementation.
The 132 reference-compressible cases have median native/reference size ratios
of approximately 0.957 for grayscale, 0.966 for retained RGB, and 0.894 for
YCbCr. These tiny synthetic images emphasize marker/table overhead. They do
not establish compression superiority on photos or other real-world content.

PSNR compares grayscale/RGB sources with reconstructed samples. CMYK numeric
component error is recorded separately; YCCK source components are already
encoded values, so they are not compared directly with converted CMYK output.
Exact reconstruction has MSE zero and a null PSNR field. Color management,
visual/photo corpus assessment, mozjpeg comparisons, complete codec throughput,
cycles, allocations and peak memory benchmarks remain required work.

The current progressive writer uses full-precision DC and AC spectral scans.
It does not yet generate successive-approximation scans or aggregate EOB runs.
Fixed fallback tables cover all 12-bit magnitudes but are deliberately general;
optimized Huffman tables are the default. Those remaining improvements will be
measured as part of the full encoding/performance milestones.
