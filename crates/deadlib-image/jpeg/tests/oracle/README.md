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

## Arithmetic reference gate

`arithmetic_transcode.c` copies coefficients through the public C API and
exposes conditioning L/U/Kx, four table destinations, restart intervals and
separate component scans. Its CLI is `input.jpg output.jpg restart L U Kx
table-layout scan-layout`. Table layout 0 shares destination 0; layouts 1-3
rotate component DC/AC destinations. Scan layout 0 is interleaved sequential,
1 uses the reference progressive scan script, 2 uses separate sequential
component scans, and 3 refines every bit from Al=13 (12-bit) or Al=10 (8-bit)
with reversed component/band scan order. APP markers are not copied by this coefficient-only client.
See ../../ARITHMETIC-VALIDATION.md for reproduction, scope and retained evidence.
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
See ../../LOSSLESS-VALIDATION.md for format, commands, scope and the reference
decoder's sampling/component-count limitations.

SOF11 uses the independently built JPEG XT reference at
`702114c8130ae818ccd2354cc03e879dac69e579`, with a documented predictive
table-selection setup patch. `reference_lossless_arithmetic.py` compares native
planes against raw PGX output (`-c -U`). It records direct streams separately
from Pt header normalization, which avoids an observed reference output bug
while leaving entropy unchanged. See ../../LOSSLESS-VALIDATION.md for the
normalization proof, source/build pinning, setup patch and two retained
reference limitation fixtures. No JPEG XT code is linked into the Rust crate.

The hierarchical decoder gate additionally applies the one-line DHP precision
parser correction and environment-controlled raw frame observer in the two
`jpegxt-hierarchy-precision.patch`/`jpegxt-frame-observation.patch` files.
The observer records ordered visible DCT coefficients and lossless sample or
residual rows before the C reconstruction pipeline. Those frame values must
match exactly. An independent integer row-expansion implementation checks pure
lossless composition; final C raster differences are recorded separately.
See ../../HIERARCHICAL-VALIDATION.md for syntax normalization, oracle output
limitations, fixture meanings and full reproduction instructions.

`reference_hierarchical_encode.py` uses the same frame observer for direct native
frame encoding. It checks source files and 49 rewritten configurations per
source, independently comparing ordered C coefficients/sample differences with
pre-entropy expectations and checking native lossless composition separately.
`examples/hierarchy_fixtures.rs` authors the additional native corpus without
decoding its encoded bytes; explicit signed-Pt and target-plane expectations
are retained alongside each JPEG. Reports distinguish repeated configurations
from distinct output hashes. `--expected-hashes <earlier-report>` verifies
byte identity and explicitly reuses the earlier C decoding evidence.

`reference_pyramid.py` checks automatic native/shared source-pyramid encoding.
It compares pre-entropy frame data with this C observer and independently
computes rational source sampling, the chosen K.5 reduction, signed residual Pt
and a floating-point FDCT. Original-header comparisons are separated from
C-input copies that clear defective base Pt prediction or insert EXP00 for
stale reference expansion state. These copies change no differential entropy.
Final mixed DCT/lossless source fidelity uses matching native IDCT references;
no C raster tolerance substitutes for exact frame data. The report, twelve
pinned source encodes and full limitations are documented in
../../HIERARCHICAL-VALIDATION.md. No production code links a reference codec.

`jpegxt-ordinary-observation.patch` adds the same optional coefficient log to
ordinary DCT's unsampled reconstruction path, used with `-U`. It observes blocks
before IDCT and changes no decoder behavior. DNL-VALIDATION.md identifies the
new four-patch executable, independent `-n` input generation, original-header
comparisons and explicitly separate known-header C-input workarounds. The
structural Python helper now resolves DNL/DHP and accepts ordinary frames;
coefficient/sample comparisons retain their existing exact semantics.

`jpeg-abbreviated` uses public Turbo APIs with one persistent codec object:
`jpeg_write_tables` plus `jpeg_start_compress(FALSE)` for real table/image
streams and `jpeg_read_header(FALSE)` for table-only headers. It accepts
separate stream files, concatenated streams and a newline path manifest.
`reference_abbreviated.py` compares exact native coefficients/quantizers or
lossless sample integers in both forms, including SOI DAC/DRI reset cases.
For lossless YCbCr/YCCK sources, generic component input and explicit Adobe
markers avoid Turbo's reset/default Huffman selector mismatch; Tq is set to
zero after the start reset and before SOF emission. No sample algorithm or
reference source is patched. The rare-process JPEG XT comparisons reconstruct
an equivalent interchange stream from an independent raw table bank; they are
not genuine C B.5 parser tests. See ../../ABBREVIATED-VALIDATION.md for provenance,
limitations, pinned session compaction and reproduction commands.

## Generic components and IJG libjpeg

`jpeg-components` authors generic-channel sequential/progressive streams through
the pinned Turbo public API. Its CLI is `OUTPUT N P layout arithmetic
restart_rows quality progressive reverse`. Sequential counts cover 1..10,
progressive 1..4; layouts select 1x1, mixed 2x2 or uniform 4x4 sampling.
Explicit legal scan groups and custom quantizers are checked independently.

`python tests/build_ijg.py --work experiments/ijg10` downloads the pinned official
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

See ../../COMPONENT-VALIDATION.md for the full-range native/C matrices, archive
hash, actual per-case executable identities, genuine IJG B.5 checks, Turbo SOS
selector limitation, JPEG XT fractional-sampling limitation and explicit
EXP00 C-input copies. Reports and retained inputs are under `tests/reports`
and `tests/fixtures/components`. No C implementation enters production code.

## mozjpeg reference gate

`python tests/build_mozjpeg.py --work experiments/mozjpeg-reference` verifies
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
