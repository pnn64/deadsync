# Native color conversion and upsampling

The codec reconstructs native component planes and converts grayscale, RGB,
YCbCr, CMYK and YCCK to shared grayscale/RGB storage. Native 12-bit values
remain available; shared 12-bit RGB/gray output is explicitly scaled to 16 bits.
ICC profiles remain accessible through the original coefficient image.

## Four-channel data and polarity

`PlanarImage::to_cmyk` returns native-precision full-resolution CMYK with an
explicit `CmykPolarity`. An Adobe transform 0/2 indicates complemented
components; untagged CMYK defaults to conventional ink values. Callers can
override an untagged CMYK plane's polarity when its producer used another
convention. `CmykImage::convert_polarity` complements all four channels in place
while preserving their interpretation and precision.

YCCK converts its first three components with the JFIF YCbCr matrix, clamps
them, and complements the result to obtain Adobe CMYK; K is retained. This
matches libjpeg's CMYK output. Four-channel data is never relabeled RGBA.

The RGB convenience output uses an uncalibrated subtractive approximation.
For conventional ink C/M/Y/K in 0..M, R=(M-C)(M-K)/M, with analogous G/B.
For complemented components it is R=C*K/M. Division rounds to nearest.
Color-managed clients can use native CMYK, explicit polarity, and the original
ICC profile. `PlanarImage::to_image` performs its RGB conversion directly and
allocates only the requested RGB buffer. A regression test permits that path
under a budget too small for a full intermediate CMYK raster.

The encoder currently accepts already encoded CMYK/YCCK component values.
Additional encoder source-polarity controls and CMYK-to-YCCK transformation
remain in the plan; this milestone covers reconstruction/output conversion.

## Chroma sample positions

Nearest and bilinear interpolation use JFIF's centered positions. For an output
coordinate x, component sampling H and maximum sampling Hmax, its component
coordinate is `(x + 0.5) * H / Hmax - 0.5`. Edges replicate the nearest sample.
The linear path uses exact rational weights with small integer denominators,
including fractional ratios such as 3:2. This replaces the earlier 8-bit
fraction approximation, which could lose several levels on 12-bit gradients.
The 12-bit fractional regression checks 4095*5/6 rounding to 3413.

YCbCr conversion uses Q16 JFIF constants. Negative green contributions use a
single signed rounded sum, including the half-way cases. Public mutable
coefficient/plane/raster models are checked for valid dimensions, precision,
grid/sample lengths, and sample ranges before reconstruction or conversion.

## Reproducible reference gate

Build the pinned libjpeg-turbo 3.2.1 C oracle as described in
tests/oracle/README.md. Generate the base encoder corpus from
ENCODER-VALIDATION.md, then run:

```text
cargo build -p deadlib-image-jpeg --example colors --example color_fixtures
target/debug/examples/color_fixtures.exe crates/deadlib-image/jpeg/experiments/color-inputs
python crates/deadlib-image/jpeg/tests/reference_colors.py --oracle <jpeg-oracle> --native target/debug/examples/colors.exe --pixel-corpus crates/deadlib-image/jpeg/experiments/pixel-gate --extra-corpus crates/deadlib-image/jpeg/experiments/color-inputs --out crates/deadlib-image/jpeg/experiments/color-gate
```

The corpus is deduplicated by JPEG hash: **243 unique images**, **486 color
comparisons**, both precisions and both interpolation policies. It includes
the pinned sequential/progressive decoder fixtures, the native encoder corpus,
and 24 extra four-channel files with YCCK 4:2:0/4:2:2 and full-resolution K,
unusual CMYK component sampling, and tagged/untagged polarity cases.

| Comparison | Cases | Maximum observed error, native sample levels |
| --- | ---: | ---: |
| Nearest vs libjpeg full color output | 235 | 3 |
| Nearest fractional vs independent filter of C planes | 8 | 3 |
| Linear vs libjpeg triangle/full-resolution output | 195 | 3 |
| Linear wider/fractional ratios vs independent filter of C planes | 48 | 3 |

Native CMYK output differs by at most 2 levels for nearest and 3 for linear.
Pillow RGB output also agrees within 3 levels on all 26 comparable tagged
8-bit four-channel cases with the same interpolation policy. Pillow's inferred
polarity for untagged CMYK differs from the explicit conventional-ink fallback;
those diagnostic differences are recorded without treating them as equal
policies. Nearest/fancy differences are similarly kept separate.

The reference uses float IDCT. Retained RGB/gray bounds are 1. YCbCr nearest
has a bound of 4, accounting for a one-level native component-IDCT difference
and matrix rounding. The triangle path can add a one-level interpolation
rounding difference; its bound is 6. YCCK-to-CMYK has bounds 4/6; applying the
additional K multiplication gives conservative RGB bounds 6/8. The maximum
observed errors are smaller, as recorded in tests/reports/colors-turbo.json.
These are numerical bounds for the stated algorithms/corpus, not tolerances
for different color-management transforms or arbitrary filters.

The C library's fancy mode uses triangle interpolation for 2:1 horizontal and
vertical ratios; generic wider ratios use its box policy. To verify native
linear interpolation for the wider/fractional ratios, the Python reference
uses exact `Fraction` arithmetic on the C decoder's raw float-IDCT planes,
then an independent real-valued JFIF matrix. The C raw planes provide an
independent entropy/IDCT source while the filter remains tied to JFIF siting.

Full photographic/conformance corpora, other C implementations, coverage-guided
fuzzing and throughput/allocation/cycle measurements remain open in COVERAGE.md.
