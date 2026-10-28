# Test-only libjpeg oracle

`oracle.c` is an independent API client that serializes quantization tables and
un-padded coefficient arrays, then decompresses raw component planes with the
reference float IDCT. This isolates coefficient correctness from IDCT, color
conversion, and upsampling differences. No reference code is used in the native
implementation or linked into its production crate.

Initial run: Windows x64, MSVC 19.44.35229, libjpeg-turbo **3.2.1**, static,
SIMD disabled (correctness reference only). Source archive:
https://github.com/libjpeg-turbo/libjpeg-turbo/archive/refs/tags/3.2.1.zip
SHA-256: `f18088e5528c2d3a615fa6f50de222fc46c92fb208ed29cd3ecd77be79cab42a`.
Pillow **12.3.0** generates only the CMYK test case. Other fixtures use cjpeg
and deterministic PPM sources. Generated source images and the fixture manifest
are checked in alongside the fixtures. They contain no third-party artwork.

Build the reference outside production dependencies:

```text
cmake -S <libjpeg-turbo-source> -B <turbo-build> -DWITH_SIMD=OFF -DENABLE_SHARED=OFF -DWITH_TURBOJPEG=OFF
cmake --build <turbo-build> --config Release
cmake -S crates/deadlib-image/jpeg/tests/oracle -B <oracle-build> -DJPEG_INCLUDE_DIR=<source>/src -DJPEG_CONFIG_DIR=<turbo-build> -DJPEG_LIBRARY=<turbo-build>/Release/jpeg-static.lib
cmake --build <oracle-build> --config Release
cargo build -p deadlib-image-jpeg --example inspect
python crates/deadlib-image/jpeg/tests/reference.py --cjpeg <cjpeg> --oracle <jpeg-oracle> --native target/debug/examples/inspect.exe --out crates/deadlib-image/jpeg/tests/fixtures/sequential --pillow
```

Use platform-specific library/executable paths on Unix. The oracle currently
requires libjpeg-turbo's 12-bit raw API; adapters for IJG libjpeg and mozjpeg are
still required by the overall plan.

Repeat the Python command with `--progressive` and the output directory
`tests/fixtures/progressive` to regenerate the second set of 25 cases. That set
adds DC/AC successive approximation, reordered spectral bands, and progressive
restart markers. Both sets include 8/12-bit precision and CMYK.

Observed gate: **50/50 exact coefficient/table comparisons** and **maximum
native IDCT error 1** for all samples in these fixtures. Rust integration tests
load these exact outputs. Tests also cover every truncated fixture prefix,
4,000 deterministic bounded mutations, corrupted restarts, fractional sampling
geometry, baseline quantizer precision rules and storage/scan limits.

The `.coef` format starts with `JCO1`, four LE u32 values (precision, width,
height, component count), then for each component seven LE u32 values (id, H,
V, sample width/height, block width/height), 64 LE u16 quantizers, and natural
order LE i16 blocks. `.planes` concatenates each component's unpadded rows as
LE u16 samples. Fixture metadata and command provenance are in manifest.json.

## Native coefficient encoder gate

```text
cargo build -p deadlib-image-jpeg --example rewrite
python crates/deadlib-image/jpeg/tests/reference_rewrite.py --oracle <jpeg-oracle> --native target/debug/examples/rewrite.exe --out crates/deadlib-image/jpeg/experiments/rewrite-gate
```

Executed with the same pinned libjpeg-turbo reference: **600/600 native
rewrites** decode with exact visible coefficients and quantizers and **zero C
decoder warnings**. This covers all 50 sources, sequential/progressive output,
fixed/optimized Huffman policies and restart intervals 0, 1, 7. The retained
case hashes and byte sizes are in `tests/reports/rewrite-turbo.json`.
Separate native tests verify metadata payload/order preservation and bounded
errors for malformed public coefficient models. Broader oracle coverage remains
open.

The pixel encoder gate and its commands are documented in
`../../ENCODER-VALIDATION.md`. The current oracle additionally writes `.color`
files: native precision LE u16 full-resolution grayscale/RGB/CMYK output with
float IDCT and nearest upsampling. Fractional ratios have coefficient/raw-plane
checks and omit `.color` because the reference upsampler cannot handle them.
