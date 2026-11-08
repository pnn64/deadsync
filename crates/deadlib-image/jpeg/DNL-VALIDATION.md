# DNL and initially unknown heights

The whole-input decoders support T.81 B.2.5 DNL for all thirteen JPEG-1 frame
processes. `decode_coefficients`, `decode_lossless` and `decode_hierarchical`
resolve the height directly after each frame's first scan, before allocating
coefficient/sample storage. There is no entropy copy or additional scan of the
compressed image. The ordinary path inspects the marker at that boundary and
continues with the same entropy decoders and storage limits.

DNL is mandatory when SOF declares Y=0. It can also replace a nonzero
provisional height. NL must be nonzero; Ld is allowed to span 4..65535 as in
Table B.10, with bytes beyond NL skipped as segment payload. Duplicate,
premature, delayed and later-scan DNL markers are errors. Huffman padding,
restart and excess-entropy checks enforce the declared MCU-row extent.
Arithmetic decoding retains Annex D's marker-supplied zero padding. The
resolved height controls pixel/storage limits; an excessively large
provisional SOF height does not trigger a needless allocation before DNL.
Missing scans remain invalid after allocation was moved to the first scan.

Returned image/frame models contain resolved heights. The public `MarkerReader`
continues to expose original SOF/DHP/DNL payloads and offsets unchanged, for
callers that need the original declarations or extended DNL bytes. Rewriting
defaults to final SOF heights rather than preserving a source DNL declaration.
APPn/COM bytes, ICC handling and global scan positions retain their existing
behavior; DNL is not counted as another scan or as application metadata.

In a hierarchy, DHP may also declare zero height. Per-frame DNL is resolved
before frame-family/reference rules and allocation. At completion, the DHP
height is inferred from components having maximal DHP vertical sampling. Those
components must agree, and normal final-resolution checks then validate every
component and the completed image limits. The last frame can belong to a
smaller component group; its SOF height alone is insufficient. A focused
regression covers this case, inconsistent maximal components and missing groups.
Prepared encoder models still require resolved dimensions.

## Encoding

`CodingOptions::height_declaration` and
`LosslessOptions::height_declaration` select:

- `HeightDeclaration::FrameHeader` (default): final SOF height, no DNL.
- `HeightDeclaration::Dnl { initial_height: 0 }`: zero SOF height and final NL
  immediately after the first completed scan.
- A nonzero `initial_height`: a provisional SOF height redefined by DNL at
  the same boundary.

These controls apply to coefficient rewriting, native/shared raster encoders,
lossless authoring, direct hierarchical frames and source-pyramid levels.
`HierarchicalOptions::omit_dhp_height` and the corresponding `PyramidOptions`
field emit a zero DHP height. DNL is emitted after entropy termination/padding
and before metadata, tables or another scan. The same bounded writer accounts
for its six bytes, including per-frame budgets. No frame JPEG is copied or
concatenated. Normalizing emitted SOF/DHP heights and removing DNL produces
byte-identical ordinary encoder output for the same controls.

## Independent evidence

The independent reference remains Thomas Richter's JPEG XT implementation at
`702114c8130ae818ccd2354cc03e879dac69e579`, MSVC 19.44/v143, Win32 Release.
The earlier three setup/observer patches are described in
HIERARCHICAL-VALIDATION.md. A fourth test-only
`tests/oracle/jpegxt-ordinary-observation.patch` records ordinary DCT blocks
immediately before IDCT during `-U` decoding. It changes no parser, entropy,
predictor, transform or arithmetic decision. Ordinary lossless output is
compared through exact raw PGX samples, without color conversion. Hierarchical
frame data still uses the earlier observer before reconstruction. The new
reference executable SHA-256 is
`a911595c53a6a49f19a4ff57fddc53bb26798ad4d3b46f035e2cf2dcabe64813`;
the prior three-patch executable remains identified in older reports.

`tests/reference_dnl.py` snapshots tools, captures frame expectations before
encoding, checks native coefficients/samples and matching reconstructed planes,
and attempts exact C comparison with the original headers first. It separately
records C errors or incorrect frame/sample results. Only then, when necessary,
does it use a C-input copy with final SOF/DHP heights restored and DNL removed.
The copy changes no entropy bytes. The previously documented stale-EXP defect
also needs EXP00 in some C-input copies. Such comparisons establish entropy
parity; they do not prove that C parsed the original DNL/DHP declarations.

The native encoder matrix covers **1,788 files** from 288 pinned source images:
all thirteen frame processes, DCT P=8/12, predictive P=2..16, zero/unchanged/
redefined SOF heights, fixed/optimized Huffman, arithmetic, progressive
refinement, subsampling, restarts and per-frame DNL. It includes **2,892 DNL
segments** and **348 zero-height DHP files**. All original native files decode
with exact frame values and matching native reference planes. All normalize
byte-for-byte to the corresponding ordinary encoder output.

Of these, **1,163 C comparisons match with original headers**, including 875
initially unknown-height cases and all 288 redundant known-height cases.
Another **625** require explicitly recorded C-input copies: 337 initially
unknown-height cases and all 288 true redefinitions. The checked values cover
**19,471 DCT blocks** and **885,753 predictive sample/residual values**. The
report breaks down original-header matches by process and initial-height mode.
See `tests/reports/dnl-encode-jpegxt.json`.

`tests/generate_dnl_reference.py` additionally authors **336 independent inputs**
with the C encoder's `-n` option: gray/RGB, ordinary/hierarchical, sequential/
progressive, both entropy families, all predictive precisions and DCT P=8/12.
All 336 encodes succeed. Only its previously documented empty DAC/EXP00 segments
are removed; DNL, zero SOF heights and every entropy byte remain unchanged.
Predictive restart intervals are explicit whole-row multiples: 85 for ordinary
17-wide images, and 153 for the hierarchy's 9/17-wide frames. DCT uses arbitrary
MCU intervals. Flat/noise source patterns are retained by hash; no source-fidelity
claim is made for the reference's hierarchical encoder.

All **336 independent DNL inputs** decode natively with exact C-checked frame
values: **248** original-input C checks match directly, and **88** use known
header copies. Their 336 native DNL rewrites also pass, with **207** original
header C matches and **129** known-header comparisons, covering another
**2,048 DCT blocks** and **221,760 predictive values**. Native source parsing is
checked against the original C input's resolved dimensions and independent
frame observations, rather than only against the rewritten JPEG.
See `tests/reports/dnl-reference-generation.json` and
`tests/reports/dnl-decode-jpegxt.json`.

The corpus under `tests/fixtures/dnl` retains **25 C-authored inputs** plus one
native baseline file checked directly by C. All thirteen frame processes occur.
It includes both original-header C successes and failures. Original DNL/zero
heights, frame expectations, matching native target planes and tool/file hashes
are retained. These are independently checked regressions, not an independent
standards conformance corpus. Tests exercise all **11,861 truncated prefixes**
and **2,600 bounded mutations** of these files, plus length/placement/height,
MCU-row, metadata and resource-boundary cases. In-memory DNL support does not
provide incremental input or output; streaming remains a separate work item.
Additional source-API tests cover 255 predictive components, 128 generated
scans, first-component-group DNL and zero-height DHP with known SOF heights.
All **129 tests** pass in release and isolated debug builds; rustfmt, Clippy
and rustdoc with warnings denied pass. All **10,612** prior encoder output
hashes remain unchanged, with **964** fresh libjpeg-turbo coefficient decodes.
The compact audit is `tests/reports/dnl-regressions.json`, which distinguishes
fresh C checks from byte-identity reuse of earlier reference evidence.

## Reproduction

Apply the four explicitly recorded JPEG XT test patches and build the pinned
reference as described in LOSSLESS-VALIDATION.md. `-U` is required for the
ordinary coefficient observer. The native tools and all C runs are isolated
from production dependencies.

```text
cargo build -p deadlib-image-jpeg --release --examples --jobs 1 --target-dir crates/deadlib-image/jpeg/experiments/dnl-target
python crates/deadlib-image/jpeg/tests/reference_dnl.py --native crates/deadlib-image/jpeg/experiments/dnl-target/release/examples/dnl.exe --inspect crates/deadlib-image/jpeg/experiments/dnl-target/release/examples/inspect.exe --lossless crates/deadlib-image/jpeg/experiments/dnl-target/release/examples/lossless_inspect.exe --hierarchy crates/deadlib-image/jpeg/experiments/dnl-target/release/examples/hierarchical_inspect.exe --reference crates/deadlib-image/jpeg/experiments/jpegxt-reference/vs15.0/jpeg/Release/jpeg.exe --out crates/deadlib-image/jpeg/experiments/dnl-gate
python crates/deadlib-image/jpeg/tests/generate_dnl_reference.py --reference crates/deadlib-image/jpeg/experiments/jpegxt-reference/vs15.0/jpeg/Release/jpeg.exe --out crates/deadlib-image/jpeg/experiments/dnl-c-authored
python crates/deadlib-image/jpeg/tests/reference_dnl.py --native crates/deadlib-image/jpeg/experiments/dnl-target/release/examples/dnl.exe --inspect crates/deadlib-image/jpeg/experiments/dnl-target/release/examples/inspect.exe --lossless crates/deadlib-image/jpeg/experiments/dnl-target/release/examples/lossless_inspect.exe --hierarchy crates/deadlib-image/jpeg/experiments/dnl-target/release/examples/hierarchical_inspect.exe --reference crates/deadlib-image/jpeg/experiments/jpegxt-reference/vs15.0/jpeg/Release/jpeg.exe --out crates/deadlib-image/jpeg/experiments/dnl-c-final-gate --sources crates/deadlib-image/jpeg/experiments/dnl-c-authored/sources --only-additional --fixtures crates/deadlib-image/jpeg/tests/fixtures/dnl
python crates/deadlib-image/jpeg/tests/pin_dnl_baseline.py --report crates/deadlib-image/jpeg/experiments/dnl-gate/report.json --native crates/deadlib-image/jpeg/experiments/dnl-gate/tools/native.exe --out crates/deadlib-image/jpeg/tests/fixtures/dnl
```

The comparison driver supports verified checkpoint resumption. A running gate
must be observed, not restarted merely because an observation timed out.
Abbreviated table/image streams, incremental I/O, broader independent
conformance, IJG/mozjpeg checks, coverage-guided fuzzing and measured performance
remain required. No complete-conformance or competitive-throughput claim is made.
