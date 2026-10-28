# Generic JPEG-1 components

Sequential DCT decoding and encoding cover 1..255 frame components at 8/12-bit
precision, including ordinary/differential Huffman and arithmetic processes.
Progressive frames retain the normative 1..4 limit. Complete conformance,
pixel tolerances and competitive performance are not claimed by these gates.

`CoefficientImage::from_components(frame, components, metadata, limits)` moves
native padded coefficient grids into a validated model without decoding or
copying buffers. Metadata stays borrowed. IDs, sampling, quantizers, dimensions,
grid storage and resource ceilings are checked. Decoder allocation accounting
now includes component/frame descriptor records before coefficient allocation.

The coefficient, abbreviated and direct hierarchical writers, plus generic
source-plane pyramids, support the full sequential range. Interleaved color
conveniences retain their gray/RGB/YCbCr/CMYK/YCCK interfaces. Generic components
remain native planes with application-defined color interpretation.

Generated scans pack consecutive components into groups of at most four and
at most ten blocks per interleaved MCU. A component whose sampling product
exceeds ten uses a non-interleaved scan. Progressive DC uses the same legal
groups, followed by one AC scan per component. Caller scripts remain borrowed.
A fixed 256-byte boundary array resolves generated groups without an owned
scan vector. Predictors and arithmetic difference classes use four scan-local
slots instead of indexing a four-element array by frame component number.

Up to four distinct quantizers preserve the earlier emission order. Otherwise
the sequential writer assigns destinations per scan and redefines them after
earlier components finish. It supports 255 distinct quantizers, not merely
255 components sharing four tables. Abbreviated output routes initial tables
to the companion packet and leaves later changes inline. Decoded components
retain their own snapshots. Source pyramids use full sequential band precision
directly; progressive tracking remains bounded to four components. Matching
IDCT references and final Pt=0 corrections preserve sampled source integers.

## Independent comparisons

| Gate | Cases | Evidence |
| --- | ---: | --- |
| Native-authored | 1,367 | Pre-entropy coefficients/quantizers and native reference/target planes |
| C-authored | 672 | Exact native/IJG/JPEG XT coefficients; Turbo also checks supported counts |
| Fixed Huffman supplement | 18 | Wide distinct quantizers, restarts, ordinary/hierarchical/source data |
| Earlier C observer regression | 198 | Byte-identical observations before/after the added observer |
| IJG pinned replay | 47 | Exact coefficients/quantizers; 12 genuine B.5 table/image pairs |

The native matrix checks every sequential frame count 1..255. Boundary matrices
cross counts 5/17/255 with both precisions/entropy coders, four sampling layouts,
one/four/distinct narrow/distinct wide quantizers, generated/custom scan order,
restart intervals 0/1/7 and abbreviated output. It includes 216 direct/source
pyramid cases and 128 progressive cases with DC grouping and Al=3 refinement.
The fixed-table supplement uses counts 5/17/255, both precisions and restart 3.

The public Turbo producer uses generic channels, float FDCT, explicit scripts
and four quantizer destinations. Its 672 cases cover counts 1..10 sequential
and 1..4 progressive, both precisions/entropy coders, 1x1/mixed 2x2/uniform 4x4
sampling, restart rows 0/1 and forward/reversed separate-component scans.
These are independently authored inputs, not native roundtrips.

IJG libjpeg 10 checks all 1,151 native ordinary cases, including fractional
sampling and 255 components. Visible coefficients, natural-order quantizers
and geometry must match exactly. The official archive is pinned:

- `https://www.ijg.org/files/jpegsrc.v10.tar.gz`
- SHA-256 `8b9eaa13242690ebd03e1728ab1edf97a81a78ed6e83624d493655f31ac95ab5`
- `jconfig.vc`, default sample storage, documented `MAX_COMPONENTS` setting
  raised from 10 to 255. Codec algorithms are unchanged. The coefficient API
  compares 8/12-bit coded values without a pixel-output scaling claim.

JPEG XT checks 1,096 native cases: 884,733 DCT blocks and 2,706,492 predictive
sample/residual integers. It also checks all C-authored inputs: 25,040 DCT
blocks. An independent DQT parser checks 146,000 native and 6,480 C-authored
component/scan quantizer snapshots against expected natural-order values.
These are configured comparisons, not distinct output hashes or official
conformance vectors.

The main abbreviated C comparisons restore initial external tables into an
equivalent interchange stream; native decoding consumes the actual pair.
The supplementary IJG client reads real B.5 tables with
`jpeg_read_header(FALSE)` and the image on one persistent object. Four added
pairs specifically cover Nf=255, both precisions/entropy coders, fractional
sampling and distinct quantizer redefinitions.

## Reference limitations and patches

The pinned Turbo decoder's SOS lookup searches only the first four frame
components. It rejects valid selectors beyond them, including its own
producer's output. IJG and JPEG XT supply those comparisons. This is not a
native relaxation or a tolerated coefficient mismatch.

JPEG XT rejects non-integer sampling ratios in its parser; IJG checks ordinary
fractional streams unchanged. Rare fractional combinations have native tests;
broader independent rare-mode coverage remains open. Its stale EXP flag also
requires EXP00 in 108 C-input copies of source pyramids. Those copies leave
entropy unchanged and are recorded per case. Native inputs are unchanged.

Two patches supplement the previously documented four JPEG XT patches:

- `jpegxt-many-component-observation.patch` reads decoded block/line buffers
  directly above four components, bypassing the four-channel bitmap/color
  client. The CLI requests its full subsampling-array capacity. Frame kinds
  explicitly choose block or line storage: `isDCTBased()` describes hierarchical
  scaling and cannot identify lossless storage. No buffer values, parser,
  entropy decisions, transforms or reconstruction results are modified.
- `jpegxt-many-component-cleanup.patch` widens the destructor index traversing
  eight line pointers per component. Its original eight-bit index loops forever
  above 31 components. Only teardown bookkeeping changes.

All six patches apply to commit `702114c8130ae818ccd2354cc03e879dac69e579`
and reproduce all thirteen affected source files exactly. Old/new builds emit
identical observations on 198 earlier files. Completed non-pyramid comparisons
from the first observer revision retain their original per-case tool/driver
hashes. Mixed-frame cases use corrected block/line dispatch; failed observer
calls are never reused. No C pixel-output claim is made for the bypassed client.

## Reproduction

From the package directory, with the existing pinned Turbo/JPEG XT builds:

```text
python tests/build_ijg.py --work experiments/ijg10
cmake --build experiments/oracle-build --config Release --target jpeg-components
cargo build --release --examples --jobs 1
python tests/reference_components.py --author AUTHOR --decode DECODE --c-author C_AUTHOR --turbo TURBO --ijg IJG --reference JPEGXT --part native --out experiments/components-native --fixtures experiments/components-fixtures
python tests/reference_components.py --author AUTHOR --decode DECODE --c-author C_AUTHOR --turbo TURBO --ijg IJG --reference JPEGXT --part c --out experiments/components-c --fixtures experiments/components-fixtures
python tests/pin_components.py --raw experiments/components-fixtures --out tests/fixtures/components --native-report experiments/components-native/report.json
python tests/reference_components_b5.py --author AUTHOR --ijg IJG --fixtures tests/fixtures/components --out experiments/components-ijg-b5
python tests/reference_components_fixed.py --author AUTHOR --ijg IJG --reference JPEGXT --out experiments/components-fixed
python tests/reference_component_observer.py --before OLD_JPEGXT --after JPEGXT --out experiments/components-observer
```

Drivers snapshot tools and record SHA-256 identities. `--resume` checks the
driver/tool identity. `--reuse-checkpoint` is limited to completed non-pyramid
cases after an observer-only update, requires every other tool hash to match,
checks parameters and preserves original provenance. Mixed-frame cases rerun.
Reports are in `tests/reports/components-*`; `ijg10-build.json` records the
archive/configuration and both public adapter revisions.

The compact corpus retains pre-entropy expectations and C-checked input data.
Large overlapping cases are selected rather than duplicated. All matrix
inputs remain reproducible from recorded parameters and author hashes. The
fixtures are regressions, not an independent standards-conformance corpus.
Malformed tests check complete prefixes of representative small images,
marker/strided boundaries of larger images and bounded deterministic mutations.
The final 75-session corpus contains 250 hashed artifacts plus its manifest
(6,089,446 bytes). Its regression test passes 36,586 prefixes/boundaries and
2,600 mutations. All 145 tests pass in release and isolated debug builds;
rustfmt, all-target Clippy and rustdoc with warnings denied also pass.

All 10,612 earlier default-writer output hashes are unchanged; 964 also pass
fresh Turbo C decodes. Other rows explicitly reuse prior C evidence through
byte identity. Incremental I/O, independent conformance corpora, broader
IJG/mozjpeg/rare-mode checks, coverage-guided fuzzing and measured performance
remain required. The full goal stays active.
