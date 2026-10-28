# Hierarchical JPEG frame coding

`decode_hierarchical(input, limits)` implements T.81 Annex J, including DHP,
EXP and differential SOF5/6/7/13/14/15. The first frame for a component uses
SOF0/1/2/3/9/10/11. The decoder retains each frame's header, expansion flags,
scan positions, original quantizers and coefficients, or native lossless
samples/differences. APPn/COM borrow the original bytes and retain global scan
positions; ICC and color interpretation use the completed DHP components.
Tables and restart settings persist across frames without concatenating or
rewriting frame byte streams.

First-scan DNL resolves initially zero/provisional SOF heights before allocation.
Frame models contain resolved heights; `MarkerReader` retains original bytes.
Zero-height DHP is inferred from the completed full-resolution components.
Encoder options also emit DNL and optionally zero DHP height. See
DNL-VALIDATION.md for these rules, separate C-header/entropy evidence and tests.

`HierarchicalImage::reconstruct_reference()` composes native planes with
modulo-65536 addition. Residual planes contain signed differences represented
as two's-complement `u16`; differential DCT has neither a level shift nor
unsigned sample clipping. Non-differential DCT restores and clips its level
shift as required by A.3.1/F.2.4.3. DC is decoded directly in differential
frames, while arithmetic conditioning retains the previous DC class. Huffman
AC categories include the additional Annex J bit at both sample precisions.
Differential lossless requires PSV=0 and restores the declared Pt without an
unsigned P-bit range check on the differences.

EXP uses the J.1.1.2 filter: replicate the right/bottom edge, preserve the
left/top samples, truncate horizontal interpolation, then truncate vertical
interpolation. Component grids can crop expanded edges to the visible frame
dimensions, including fractional sampling. `reconstruct()` supplies the common
planar/color interface and clips completed DCT excursions to unsigned P-bit
output. The raw reference method preserves the modulo values before display
clipping. Pure lossless Pt=0 is exact when the hierarchy follows Annex J's
integer reconstruction; mixed DCT/lossless hierarchies also depend on the
encoder's reconstructed DCT values.

Header rules reject missing/duplicate/late DHP, absent reference components,
invalid EXP placement/parameters, incomplete final resolution, inconsistent
precision/entropy families and illegal frames after a final lossless correction
to DCT components. The baseline/arithmetic exception is supported. Frame and
scan counts, component storage, retained tables, metadata and reconstruction
peaks are bounded. Validation state advances once per frame, without replaying
all preceding frames. Public model mutations are revalidated before rendering.
Visible non-differential lossless samples must fit their declared precision.
Partial-MCU completion samples are consumed to preserve entropy/prediction
state and discarded per A.2.4; the decoder tolerates completion values outside
the visible sample range. A literal test covers both this behavior and rejection
of an out-of-range visible sample. The reference produces such discarded values
in some small odd-width frames even for constant source planes.

## Independent evidence

The reference is Thomas Richter's [JPEG XT implementation](https://github.com/thorfdbg/libjpeg),
commit `702114c8130ae818ccd2354cc03e879dac69e579`, built with MSVC 19.44/v143,
Win32 Release. No reference code is linked into the native codec. Three
test-only patches under `tests/oracle` are explicit:

- `jpegxt-lossless-table-selection.patch`: the earlier two-line predictive
  encoder setup fix, selecting entropy tables without a DCT quantizer lookup.
- `jpegxt-hierarchy-precision.patch`: treat DHP (`Dimensions`) as permitting
  P=2..16 in the reference parser, per B.3.2. Without this one-line case label,
  it rejects its own lossless hierarchy outside P=8/12 before entropy decoding.
  Its citation comment was corrected to B.3.2 after the recorded run; executable
  code is unchanged.
- `jpegxt-frame-observation.patch`: an optional environment-controlled observer
  writes entropy-decoded DCT blocks and native sample/residual rows immediately
  before reconstruction. It changes no coder, predictor, transform or filter.
  Logs retain frame process/dimensions, component IDs and within-component
  order. Only bottom padding beyond visible lossless rows is excluded.

`tests/reference_hierarchical.py` snapshots both executables and compares every
visible coefficient and lossless sample/difference exactly, separately from
the final C raster. It checks frame/scan structure and native component sizes.
An independent Python implementation expands complete rows horizontally and
then vertically and composes all pure-lossless hierarchies modulo 65536. Both
raw native reference planes and final clipped output must agree exactly with
that implementation, using the independently C-checked frame values.
The matrix spans six coding families, P=8/12 for DCT, P=2..16 for pure lossless,
one through four resolutions, gray/RGB with 4:4:4 or 4:2:0 sampling, odd/even
dimensions, and constant/ramp/noise sources. The report records actual counts,
reference encoder failures, patch/executable hashes and all input hashes.

The retained run attempts **2,340 inputs**. All **1,798 decodable reference
files** pass exact frame-data comparisons: 720 DCT hierarchies, 268 pure
Huffman-lossless hierarchies and 810 pure arithmetic-lossless hierarchies.
The comparison covers **18,768 DCT blocks** and **2,226,363 visible lossless
samples/residuals**, with zero runtime reference warnings. Another **542**
inputs fail in the reference Huffman encoder before a usable stream exists.
All **1,078 pure-lossless compositions** match the independent integer model
exactly. The syntax repair removes 1,716 empty DAC and 144 EXP00 segments.
There are 42 pinned hierarchical JPEGs and frame-data snapshots. All 104 crate
tests pass in release and the isolated debug package build; rustfmt, Clippy
and rustdoc with warnings denied pass.

On this matrix the observed final C DCT-raster difference reaches 13 sample
units; pure-lossless C raster distance reaches 32768 modulo 65536. These are
documented reference reconstruction differences, **not accepted IDCT error
bounds**. The reference's flat-source failure affects 115,456 samples in each
decoder's output. Twelve cross-entropy source pairs have different C-generated
frame data, which each decoder nevertheless recovers exactly. These findings
are recorded rather than assuming that distinct C encoding paths are identical.

The selected fixtures pin exact C-checked frame data. Their final `.samples`
files are **native reconstruction regression snapshots**, not exact C raster
oracles. Literal independently assembled entropy/expected samples establish
the Annex J integer rules. Tests include ordered EXP truncation, edge padding,
modulo wrap at P=16, negative differences and Pt, no DC prediction/level shift,
progressive DC refinement, additional AC magnitude categories at P=8/12,
separate component bases, fractional component cropping, the baseline entropy
exception, all 255 components and twelve successive Huffman lossless frames.
A separate signed floating-point DCT equation checks 100 dense differential
blocks, with maximum rounded-sample difference one on that defined matrix.
All 49,300 truncated JPEG prefixes and 4,200 bounded mutations are exercised
in Cargo.

## Reference reconstruction limitations

The C reference's raster is not an exact lossless-hierarchy oracle:

- Its internal fractional planes defer integer truncation across EXP passes.
  For the literal base `[0,1;1,2]` with zero residuals, the doubly interpolated
  center becomes one instead of Annex J's zero. Input, independently specified
  expected samples and observed C output are retained and hashed under
  `tests/fixtures/jpegxt-limitations/hierarchical-truncation.*`.
- It retains fractional, unclipped DCT intermediates between frames. A.3.1's
  initial inverse level shift includes unsigned clipping. Consequently a
  mixed DCT/lossless final image can differ by more than a usual IDCT rounding
  unit even when **all original frame coefficients and residuals match exactly**.
  Final raster differences are reported as observations; no general numerical
  conformance tolerance is inferred from them.
- Its final output clips instead of applying Annex J's required modulo-65536
  reconstruction. The retained one-pixel literal `hierarchical-modulo.jpg`
  has a zero base and a signed -1 differential: the expected output is 65535,
  while C outputs zero. Full-range 16-bit C raster differences are therefore
  not bounded by the one-unit EXP truncation discrepancy.
- It emits redundant EXP00 segments and empty DAC segments in some streams.
  B.3.3 requires EXP only for an actual expansion; B.2.4.3 requires at least one
  DAC entry. The driver removes only those empty/no-op segments in native
  input copies. Original/normalized hashes and removal counts are retained;
  entropy bytes and table contents are unchanged. The native parser continues
  to reject these invalid segments.
- Its optimized Huffman encoder cannot represent some of its own deep
  lossless differential frames. Those explicit reference encoder failures
  are counted, with their diagnostics, rather than reported as passing cases.
- Some deep pure-lossless encodes fail to retain their source even when both
  decoders agree exactly. The retained P=2 four-level constant example starts
  from nine ones, adds two all-zero residual frames, then adds an all-minus-one
  frame. Both decoders and independent integer composition produce zero,
  although the original source was one. Source, frame-derived reconstruction
  and observed C output are retained as `hierarchical-flat-encoder.*`; these
  decoder cases do not establish that the reference encoder is lossless.

These observations do not establish correctness for every hierarchical stream.
Independent conformance corpora, broader DCT numerical checks,
abbreviated streams, incremental I/O, coverage-guided fuzzing and
all measured performance work remain required.

## Native frame encoding

`HierarchicalImage::from_frames(header, frames, metadata, limits)` constructs a
native hierarchy without another decoder. It assigns each frame's `first_scan`
from the supplied nominal source scan counts; metadata uses those positions.
`image.encode(HierarchicalOptions::default())` writes DHP, EXP and all six
differential frame families directly. Frame headers, sampling, visible DCT
coefficients/quantizers and sample differences are read from borrowed views of
the owned model. No frame is cloned or compressed into a second JPEG buffer.
The shared ordinary writers emit each frame into the single bounded output.

Defaults retain each source frame's entropy family and sequential/progressive
choice, with full-precision generated scans and lossless Pt=0. Thus rewritten
frame values remain exact even if the original file used a nonzero Pt. A
`HierarchicalFrameOptions` descriptor per frame can select DCT `CodingOptions`
or `LosslessOptions`. DCT controls include fixed/optimized Huffman or arithmetic,
spectral/custom scans, successive approximation through Al=13 and MCU restarts.
Predictive controls include component scripts, row restarts and Pt. Generated
differential predictive scans force PSV=0; explicit scripts must select zero.
Signed residual Pt uses integer division toward zero before rescaling, so -3
at Pt=1 becomes -2 rather than -4. Lossless point transforms or partial DCT
scripts intentionally change frame values and therefore subsequent references.

Output processes are checked against the cross-frame Annex J rules before
writing. Every frame clears or replaces inherited DRI, including an explicit
zero when restarts are disabled. Differential DCT resets only the numerical DC
predictor for each block; arithmetic DC conditioning retains its prior class.
Fixed Huffman tables cover the additional AC category. For 12-bit progressive
differential frames the complete 256-symbol fixed table uses 255 nine-bit codes
and one ten-bit code, avoiding an impossible 256-code count in one DHT byte or
an all-ones code. Tests exercise the ten-bit run/magnitude symbol itself.

`encode_with_metadata` preserves APPn/COM order and frame-relative scan positions
where possible, moving positions beyond a shortened frame to its end. Additional
segments and replacement ICC chunks precede DHP. Removal/replacement uses the
shared metadata writer. Global budgets charge retained source storage, additional
metadata, output capacity, Huffman symbol storage and the largest arithmetic
history. Nested lossless limits further constrain the hierarchy. Each nested
output byte budget bounds its frame writer, and the global byte budget also
covers DHP/EXP, leading metadata and relocated frame-end metadata. Allocator
profiling remains open.

DCT frame encoding currently supports one through four components; predictive
frames support up to 255 with generated legal component scans. This API accepts
prepared coefficients and signed difference planes. Automatic construction of
resolution pyramids, source sampling and differential FDCT from shared/native
raster inputs is the next hierarchy encoder stage, and remains required.
Large DCT sampling layouts can require explicit component scripts to satisfy
the ten-block interleaved MCU limit, as with ordinary coefficient encoding.

### Encoder evidence

Two pinned executable-snapshot matrices perform **5,200 exact C/native
frame-data comparisons**, with zero C warnings:

- 42 existing reference-derived hierarchies, each checked directly and rewritten
  in 49 configurations: **2,100 comparisons**, 19,600 DCT blocks and 965,550
  sample/residual values. See `tests/reports/hierarchical-encode-rewrite-jpegxt.json`.
- 62 newly authored native hierarchies with expectations constructed **before
  entropy encoding**, never recovered from JPEG bytes: **3,100 comparisons**,
  1,200 DCT blocks and 1,962,000 sample/residual values. Thirty precisions/sampling
  combinations use Pt=0 and thirty use differential Pt=1; two DCT cases exercise
  both precision/category boundaries. See
  `tests/reports/hierarchical-encode-native-jpegxt.json`.

The 49 rewritten configurations span both entropy coders, fixed/optimized tables,
sequential/progressive DCT, Al=0/3/13 and restart intervals 0/1/3. Controls irrelevant
to a frame family can produce identical files: the reports explicitly count
distinct output hashes (762 and 642 respectively). The totals include 104 direct
source checks and 5,096 native rewrites; 62 source files are also native encodes.
Pure-lossless composition matches the independent integer model in **3,900**
comparisons. All 3,000 native-authored lossless comparisons additionally match
prescribed target samples: original integers at Pt=0, and explicit signed
division/rescaling targets at Pt=1. The authored source files use arithmetic
coding, so the 30 original differential-Pt=1 streams are checked directly. Their
restored residual values are then rewritten through both entropy coders at
Pt=0. Huffman differential Pt=1 additionally has an independently specified
entropy-bit golden. The ordinary lossless reference's initial-prediction
workaround is not used for these frames.

`examples/hierarchy_fixtures.rs` constructs gray and 4:2:0 sample hierarchies at
every precision P=2..16 and DCT category-boundary cases at P=8/12. The 62 JPEGs,
124 pre-entropy frame dumps and 60 exact target planes are retained under
`tests/fixtures/hierarchical-native`, with hashes and provenance. These are
independently C-checked native encoder regressions, not an independent standards
conformance corpus. Existing literal Annex J tests remain separate.

Common-writer refactoring leaves all **600 prior DCT** and **4,448 Huffman
lossless** output hashes unchanged. The DCT matrix was also decoded again by
libjpeg-turbo with exact coefficients and no warnings. Reports are
`rewrite-hierarchical-refactor-turbo.json` and
`lossless-hierarchical-refactor-huffman.json` under `tests/reports`.
Native tests cover authored frames, signed/Pt/wrap goldens, full fixed tables,
explicit/reversed scripts, mixed component Pt, partial DCT precision, inherited
restart clearing, metadata/ICC, budget errors and 255-component authoring.
All **112 tests** pass in release and isolated debug builds; rustfmt, Clippy and
rustdoc with warnings denied pass. The source-pyramid milestone below extends
that work; broader rare scan/Pt combinations and independent
conformance/performance work remain open.

```text
cargo build -p deadlib-image-jpeg --release --examples --jobs 1 --target-dir crates/deadlib-image/jpeg/experiments/arithmetic-encode-target
crates/deadlib-image/jpeg/experiments/arithmetic-encode-target/release/examples/hierarchy_fixtures.exe crates/deadlib-image/jpeg/tests/fixtures/hierarchical-native
python crates/deadlib-image/jpeg/tests/reference_hierarchical_encode.py --native crates/deadlib-image/jpeg/experiments/arithmetic-encode-target/release/examples/rewrite_hierarchy.exe --inspect crates/deadlib-image/jpeg/experiments/arithmetic-encode-target/release/examples/hierarchical_inspect.exe --reference crates/deadlib-image/jpeg/experiments/jpegxt-reference/vs15.0/jpeg/Release/jpeg.exe --sources crates/deadlib-image/jpeg/tests/fixtures/hierarchical-native --out crates/deadlib-image/jpeg/experiments/hierarchy-encode-native-gate
```

Omit `--sources` to check the 42 reference-derived fixtures. `--resume` verifies
the saved executable and source/frame-data hashes. Decoder and encoder matrices
share `tests/hierarchy_support.py`. The DNL milestone extends its structural
parser for ordinary frames and resolved DNL/DHP heights; sample, observation
and independent lossless-composition helpers retain their behavior.

## Reproduction

Use the pinned checkout/build instructions in LOSSLESS-VALIDATION.md, then apply
the hierarchy precision and frame observation patches before rebuilding.

```text
git -C crates/deadlib-image/jpeg/experiments/jpegxt-reference apply ../../tests/oracle/jpegxt-hierarchy-precision.patch ../../tests/oracle/jpegxt-frame-observation.patch
cargo build -p deadlib-image-jpeg --release --example hierarchical_inspect --jobs 1 --target-dir crates/deadlib-image/jpeg/experiments/arithmetic-encode-target
python crates/deadlib-image/jpeg/tests/reference_hierarchical.py --native crates/deadlib-image/jpeg/experiments/arithmetic-encode-target/release/examples/hierarchical_inspect.exe --reference crates/deadlib-image/jpeg/experiments/jpegxt-reference/vs15.0/jpeg/Release/jpeg.exe --out crates/deadlib-image/jpeg/experiments/hierarchy-verified-gate --fixtures crates/deadlib-image/jpeg/tests/fixtures/hierarchical
```

An interrupted gate resumes with `--resume` using verified executable snapshots
and source hashes. A process still running must be observed rather than
restarted. The checked-in report is `tests/reports/hierarchical-jpegxt.json`.

## Automatic source pyramids

`build_hierarchical_planes` accepts a final DHP header and borrowed native
component planes, including fractional grids and up to 255 predictive
components. `build_hierarchical_samples` accepts interleaved U8/U16 samples in
all five native color spaces. `encode_hierarchical_planes`,
`encode_hierarchical_samples`, and `encode_hierarchical_image` prepare and write
the hierarchy; each has a metadata variant. DCT stages currently require
P=8/12 and 1..4 components. Predictive stages permit P=2..16 and 255 components.
Raster defaults retain original precision, color space and CMYK integers with
4:4:4 sampling. Explicit options select precision scaling, rational component
sampling, RGB-to-YCbCr or CMYK-to-YCCK. Color matrix intermediates use i64 so
explicit 16-bit transforms cannot overflow. Adobe hints express the chosen
interpretation; no JFIF marker is generated for hierarchical output.

`PyramidOptions` borrows a sequence of `PyramidLevel` descriptors. The first
has no expansion. Later horizontal/vertical flags determine earlier global
dimensions backwards by ceil division from the source dimensions. Per-component
grids follow the frame's sampling factors independently, with edge replication.
Unscaled refinements and repeated singleton resolutions are supported. Each
level selects DCT quality/quantizer width or predictive options plus entropy,
scripts, restarts and progressive precision through the existing controls.
The Annex J frame-family rules still apply; for example, entropy families
cannot be mixed arbitrarily and a spatial correction ends a DCT component's
hierarchy.

The chosen source reduction is the K.5 1-2-1 neighborhood in each expanded
axis. Product weights are accumulated in integer arithmetic and rounded once
after the two-dimensional sum; a single-axis reduction rounds after its sum.
This is an encoder choice, separate from the normative ordered EXP decoder
filter. Native target grids are computed backwards only for unique coarser
resolutions. No converted full-resolution target raster is allocated. Coarse
target buffers are reused as references after their last use, unscaled
reference updates occur in place, and a final unscaled spatial correction
consumes the reference buffer in place. Retained frame data, temporary grids,
metadata, scans and output are bounded; these implementation choices have not
yet been measured with allocator or throughput benchmarks.

Each residual uses the actual reconstructed previous reference. Predictive
base Pt discards unsigned low bits; differential Pt divides signed residuals
toward zero. DCT residuals use a signed FDCT without a level shift and reject
quantized coefficients that do not fit the model's i16 storage. Custom
progressive scripts mask unseen coefficients and apply the actual final
DC/AC precision before reconstructing the next reference. No intermediate
JPEG encode/decode is needed. A prepared model contains the realized frame
values; the source-encoding convenience reuses the supplied controls when
writing it. A final Pt=0 correction reproduces the sampled source component
grids exactly with the matching native IDCT. Different decoder IDCTs can give
different mixed DCT/lossless output, as discussed in K.7.2.1. Explicit chroma
subsampling itself discards source spatial detail.

### Source-encoding gate

`tests/reference_pyramid.py` snapshots its executables and checks **560 source
pyramids**: 240 predictive and 320 DCT, all P=2..16 for predictive coding,
P=8/12 for DCT, both entropy coders, sequential/progressive output, fixed and
optimized Huffman tables, restarts, base/middle Pt, partial DC-only progressive
bases, final spatial corrections, all five color spaces, and 4:4:4/4:2:0/4:2:2
sampling. Sources include flat, ramp, noise and alternating-extrema patterns.
The independent Python source model integrates component footprints, reduces
the source grids with the stated filter, checks every predictive residual,
and computes a floating-point FDCT from source targets and preceding native
reference planes. Full-precision FDCT coefficients differ by at most **one
quantized level**; partial DC masks are checked against the masked candidates
at floating-point coefficient +/-1, and unseen AC must be zero.

All **10,512 DCT blocks** and **425,916 visible predictive samples/residuals**
match the independent C frame observer exactly. All 425,916 predictive values
also match the independent source-residual model. All **400** cases that end
in a Pt=0 predictive frame match the sampled source grids exactly. Runtime C
warnings are zero. The report is `tests/reports/pyramid-jpegxt.json`.

The report separates **280 original-header C comparisons** from **280 C-input
copies with explicit workarounds**. In 120 predictive cases, base Pt is cleared
in the C-input copy to bypass its already documented initial-prediction defect;
base observations are shifted accordingly, and all differential entropy bytes
and expectations remain unchanged. In 160 DCT cases, the C decoder incorrectly
retains old EXP flags when a same-resolution final correction omits EXP. The
C-input copy inserts EXP00 before that correction to clear the stale state.
The original native output omits this redundant marker and is checked
unchanged by the native decoder. No C final-raster tolerance is used as proof
of either source fidelity or entropy correctness.

Twelve representative native encodes, their original source integers,
pre-entropy frame expectations, exact target planes and file/tool hashes are
retained under `tests/fixtures/pyramid-native`. These are independently checked
encoder regressions, not an independent standards conformance corpus. Tests
also cover single-axis and fractional grids, 255-component repeated singleton
resolutions, explicit 16-bit color conversion, shared images, ICC chunking,
metadata and exact resource boundaries.
All **122 tests** pass in release and isolated debug builds. The twelve new
fixtures check all **30,164 truncated prefixes** and **1,200 bounded mutations**;
rustfmt, Clippy and rustdoc with warnings denied pass.

Common transform/scan refactoring retains all **5,200 hierarchy**, **4,448
Huffman lossless**, **600 coefficient rewrite**, **172 pixel** and **192 CMYK/YCCK** encoder output hashes.
The 600 rewrites, 172 pixel and 192 CMYK files were decoded again by libjpeg-turbo with exact
coefficient/table matches and their existing float-FDCT bounds. Byte-identity
regression reports explicitly reuse previous C evidence rather than claim
another C decoding run. The consolidated audit is
`tests/reports/pyramid-refactor-regressions.json`; it records baseline/candidate
report hashes, exact counts and tool hashes without duplicating 10,612 old
per-case records.

```text
cargo build -p deadlib-image-jpeg --release --example pyramid_encode --example hierarchical_inspect --jobs 1 --target-dir crates/deadlib-image/jpeg/experiments/pyramid-target
python crates/deadlib-image/jpeg/tests/reference_pyramid.py --native crates/deadlib-image/jpeg/experiments/pyramid-target/release/examples/pyramid_encode.exe --inspect crates/deadlib-image/jpeg/experiments/pyramid-target/release/examples/hierarchical_inspect.exe --reference crates/deadlib-image/jpeg/experiments/jpegxt-reference/vs15.0/jpeg/Release/jpeg.exe --out crates/deadlib-image/jpeg/experiments/pyramid-final-gate --fixtures crates/deadlib-image/jpeg/tests/fixtures/pyramid-native
```

Broader rare scan/Pt combinations, abbreviated streams, incremental I/O,
independent standards-conformance and additional reference corpora,
coverage-guided fuzzing, and measured optimization remain required.
