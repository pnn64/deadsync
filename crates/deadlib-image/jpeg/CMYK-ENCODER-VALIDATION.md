# CMYK source conventions and YCCK encoding

`EncodeOptions::cmyk_polarity` selects conventional ink samples or their Adobe
complements. The default is `AdobeInverted`, preserving the original raw-input
API. Conventional samples are complemented during sampling for CMYK output.
`cmyk_to_ycck` applies the JFIF matrix to conventional CMY and retains
complemented K. Conversion and area downsampling operate directly on borrowed
source samples, including precision scaling; no converted source image is
allocated. Already encoded YCCK input retains its original meaning.

YCCK presets keep Y and K at the same resolution. 4:4:4, 4:2:2, 4:2:0, 4:4:0
and 4:1:1 fit the current interleaved MCU limit. The 4:1:0 preset would require
18 blocks per MCU for YCCK and returns an error; multi-scan layouts remain in
the overall encoder plan. `Subsampling::Custom` controls every component and
retains the configured K resolution. K and Y use the luma quantizer.

`CmykImage::encode` and `encode_with_metadata` carry the raster's polarity into
the encoder, including profile transport. The source-polarity option applies
to CMYK input; it does not reinterpret already transformed YCCK or RGB data.

## External gate

```text
cmake --build <oracle-build> --config Release
cargo build -p deadlib-image-jpeg --example cmyk_fixtures
target/debug/examples/cmyk_fixtures.exe crates/deadlib-image/jpeg/experiments/cmyk-encode-gate
python crates/deadlib-image/jpeg/tests/reference_cmyk_encode.py --oracle <jpeg-oracle> --encoder <jpeg-cmyk-encode> --corpus crates/deadlib-image/jpeg/experiments/cmyk-encode-gate
```

The independent C client uses libjpeg-turbo 3.2.1's native CMYK/YCCK input and
float DCT, with identical precision, quantizers, sampling, restarts and quality.
The corpus covers 192 cases: 8/12-bit output, both source polarities, qualities
1/50/90/100, CMYK/YCCK output, 4:4:4/4:2:2/4:2:0 and sequential/progressive
coding. Every native entropy stream is decoded by the C oracle and must match
the native quantizer/coefficient arrays serialized **before** entropy coding.

Executed results:
- 192 exact native-to-C coefficient/table reconstructions, zero C warnings.
- All 96 equivalent-polarity pairs produce byte-identical native JPEG files.
- All 192 C float-encoder comparisons have exact geometry/quantizers; maximum
  observed quantized coefficient difference is 4. The bound is 1 for identical
  4:4:4 CMYK samples and 9 when conversion/filter rounding can differ by one
  sample (the orthonormal 8x8 DCT bound of 8, plus quantization rounding).
- Case hashes, byte sizes and numerical errors are retained in
  tests/reports/cmyk-encode-turbo.json.

Native tests additionally cover 16-bit source scaling to 8/12-bit output,
sampling and ten-block MCU limits, native-raster/profile entry points,
constant-white reconstruction, and preservation of already encoded YCCK.

This gate complements ENCODER-VALIDATION.md and COLOR-VALIDATION.md. Full
independent corpora, the other C oracles, rare modes, streaming, fuzzing and
performance work remain open in COVERAGE.md.
