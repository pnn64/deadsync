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

Use platform-specific library/executable paths on Unix. This raw-plane oracle
requires libjpeg-turbo's 12-bit API; separate clients below cover IJG libjpeg
and mozjpeg.

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
python crates/deadlib-image/jpeg/tests/reference_rewrite.py --oracle <jpeg-oracle> --native target/debug/examples/rewrite.exe --out target/jpeg-reference/rewrite-gate
```

The oracle additionally writes `.color`
files: native precision LE u16 full-resolution grayscale/RGB/CMYK output with
float IDCT and nearest upsampling. Fractional ratios have coefficient/raw-plane
checks and omit `.color` because the reference upsampler cannot handle them.

## Arithmetic reference gate

`arithmetic_transcode.c` copies coefficients through the public C API and
exposes conditioning L/U/Kx, four table destinations, restart intervals and
separate component scans. Its CLI is `input.jpg output.jpg restart L U Kx
table-layout scan-layout`. Table layout 0 shares destination 0; layouts 1-3
rotate component DC/AC destinations. Scan layout 0 is interleaved sequential,
1 uses the reference progressive scan script, 2 uses separate sequential
component scans, and 3 refines every bit from Al=13 (12-bit) or Al=10 (8-bit)
with reversed component/band scan order. APP markers are not copied by this coefficient-only client.
The native arithmetic encoder gate uses `reference_arithmetic_encode.py` with
this same C coefficient decoder. It snapshots executables automatically and
supports verified checkpoint resumption; native encoder bytes are never taken
from the reference library.

## Predictive lossless reference gate

`lossless.c` uses public `jpeg[12/16]_write_scanlines` and
`jpeg[12/16]_read_scanlines` APIs at precision 2..16. Encoding accepts
`encode input.raw output.jpg width height precision components predictor Pt
restart-rows layout`; the raw source is interleaved LE u16 at every precision.
Layout 0 interleaves, layout 1 separates components, and layout 2 separates
components with predictor/Pt varying by component. Decoding accepts
`decode input.jpg output.samples` and emits exact native `JLP1` planes.

SOF11 uses the independently built JPEG XT reference at
`702114c8130ae818ccd2354cc03e879dac69e579`, with a documented predictive
table-selection setup patch. `reference_lossless_arithmetic.py` compares native
planes against raw PGX output (`-c -U`). It records direct streams separately
from Pt header normalization, which avoids an observed reference output bug
while leaving entropy unchanged. No JPEG XT code is linked into the Rust crate.

`reference_hierarchical_encode.py` uses the same frame observer for direct native
frame encoding. It checks source files and 49 rewritten configurations per
source, independently comparing ordered C coefficients/sample differences with
pre-entropy expectations and checking native lossless composition separately.
`examples/hierarchy_fixtures.rs` authors the additional native corpus without
decoding its encoded bytes; explicit signed-Pt and target-plane expectations
are retained alongside each JPEG. Reports distinguish repeated configurations
from distinct output hashes. `--expected-hashes <earlier-report>` verifies
byte identity and explicitly reuses the earlier C decoding evidence.

## Generic components and IJG libjpeg

`jpeg-components` authors generic-channel sequential/progressive streams through
the pinned Turbo public API. Its CLI is `OUTPUT N P layout arithmetic
restart_rows quality progressive reverse`. Sequential counts cover 1..10,
progressive 1..4; layouts select 1x1, mixed 2x2 or uniform 4x4 sampling.
Explicit legal scan groups and custom quantizers are checked independently.

`python crates/deadlib-image/jpeg/tests/build_ijg.py --work target/jpeg-reference/ijg10` downloads the pinned official
IJG libjpeg 10 archive and builds `ijg-coefficients`. Only the documented
`MAX_COMPONENTS` configuration is raised to 255; codec algorithms are unchanged.
The client accepts `INPUT OUTPUT.coef`, or `TABLE_PACKET IMAGE OUTPUT.coef`
using `jpeg_read_header(FALSE)` on the same object. It emits the existing JCO1
visible coefficient/quantizer format at 8/12-bit coded precision. This gate
does not exercise pixel output or rescaling.

The JPEG XT build adds `jpegxt-many-component-observation.patch` and
`jpegxt-many-component-cleanup.patch` after the four earlier patches. The first
reads decoded block/line storage above four components without the bitmap/color
client; the second widens a teardown loop index that otherwise wraps above 31
components. They change no entropy, transform or prediction algorithms. The
six-patch chain reproduces all thirteen affected source files, and 198 earlier
frame logs match the four-patch build exactly.

## mozjpeg reference gate

`python crates/deadlib-image/jpeg/tests/build_mozjpeg.py --work target/jpeg-reference/mozjpeg-reference` verifies
the pinned official 4.1.1 archive and builds unmodified upstream 8/12-bit static
libraries plus producers and separate `classic-oracle` clients. The portable
client uses each build's JSAMPLE layout through public APIs and emits the same
JCO1/LE-u16 plane/color formats. Upstream's 12-bit configuration disables
arithmetic. SIMD is disabled; these are correctness builds.

`reference_mozjpeg.py` checks native/mozjpeg/Turbo/IJG coefficients exactly,
with separate float-IDCT and nearest-color limits, native writer inputs and
explicit source/model exclusions. `reference_mozjpeg_limitation.py` separately
records the scan-optimization/restart producer defect and empty DHT segments
during explicit DC refinement. No runtime C warning is accepted by the main
gate. See ../../MOZJPEG-VALIDATION.md for original/modified input distinctions,
the unmodified source hash, numerical scope and reproduction commands.

## Official ITU T.83 data

`tests/fetch_t83.py` pins unchanged streams and numeric references from the
official public archive. `reference_t83.py` checks original DCT inputs through
the pinned IJG/Turbo/mozjpeg clients and original lossless PGX output through
JPEG XT against exact official SRC values. IJG consumes the real B1/B2 packet
pair, including 255 components. `reference_t83_encode.py` checks 186 native
coefficient rewrites against supported C clients, with a separately labeled
equivalent-interchange B2 source for the standalone rewrite CLI.

`tests/support/t83_transform.rs` uses the native transforms and an independent
f64 cosine model for the official EREF/DREF arrays. DREF is a reconstructed
sample/FDCT reference, not an entropy coefficient dump; its edge treatments
are explicit. T83-VALIDATION.md records the numerical gates, original copyright,
unavailable streams and remaining formal acceptance audit. No reference source
or standard text is copied into production code.
