# JPEG-1 implementation plan

The requested end state is a complete, independently reusable, native Rust
JPEG-1 decoder and encoder. Register only this package in the workspace;
do not integrate it into DeadSync or change the existing image codecs/loaders.
Production code may depend on shared `deadlib-image` primitives, never another
JPEG implementation. Preserve unrelated work and commit every milestone.

1. Bounded marker/segment parsing, native JPEG model, table validation,
   entropy bit handling, sequential Huffman coefficient reconstruction,
   8/12-bit integer IDCT, component planes, metadata and color conversion.
2. Progressive Huffman decoding including successive approximation,
   arbitrary legal scan/component ordering, restarts and exact coefficient
   differential validation. Streaming input/output and resource boundaries.
3. Native DCT encoder: quality, precision, subsampling, restart intervals,
   optimized Huffman tables, sequential/progressive scans, metadata retention.
4. Predictive lossless, arithmetic entropy coding, and hierarchical JPEG-1
   processes in both directions. Exact sample validation for lossless modes.
5. Independent, pinned conformance fixtures and libjpeg-turbo/libjpeg/mozjpeg
   oracles: coefficient equality, justified pixel tolerances, malformed input,
   every streaming boundary, metadata/ICC, unusual subsampling and fuzzing.
6. Reproducible stage/pipeline benchmarks: initialization, decoding, streaming,
   encoding, cycles, allocations and peak memory. Compare throughput, size and
   visual quality to the references. Optimize measured bottlenecks and recheck
   correctness. SIMD requires measured benefit.

These are implementation stages, not reductions in scope. The goal remains
open until all requested modes, validation and performance work are verified.
See COVERAGE.md for current evidence and gaps. Never infer conformance from
internal roundtrips alone.

Normative sources:
- ITU-T T.81 / ISO/IEC 10918-1, especially Annexes A–H, J and K:
  https://www.w3.org/Graphics/JPEG/itu-t81.pdf
- JFIF 1.02: https://www.w3.org/Graphics/JPEG/jfif3.pdf
- ITU-T T.871 / ISO/IEC 10918-5:
  https://www.itu.int/rec/T-REC-T.871/en

Completion evidence must identify each supported process, precision, entropy
mode and API, external oracle versions/commands, fixture provenance, numerical
error bounds and actual benchmark/fuzz runs. Unsupported features must return
explicit errors until implemented.

Verified milestones:
- `cdc8e854d`: package registration, complete SOF process model, native borrowed
  marker/entropy segment reader, marker/precision/truncation tests.
- `647cf77fa`: native Huffman, sequential 8/12-bit coefficients,
  bounded allocations, integer IDCT, component planes and initial color pipeline.
  All 25 pinned libjpeg-turbo cases have exact coefficients/tables and native
  IDCT error <=1. Twelve Rust tests pass in debug, release and isolated builds;
  rustfmt and Clippy with warnings denied pass. See tests/oracle/README.md.
  Encoding, rare modes and remaining gates are still open.
- `a65c4eabf`: SOF2 spectral selection and successive
  approximation in native Rust, with band precision tracking, DC/AC refinement,
  EOB runs, partial progressions, unused-table selectors and quantizer stability.
  Another 25 pinned 8/12-bit libjpeg-turbo cases match exactly in coefficients
  and quantizers, IDCT error <=1. Maximum Al=13 negative DC refinement and bad
  scan/restart/EOB transitions have focused tests. Streaming, encoding, rare
  modes, broader reference validation and all performance work remain open.
  Final gates: 17 tests pass in debug, release and isolated builds; rustfmt,
  Clippy and rustdoc with warnings denied pass. All 50 checked-in oracle cases
  retain exact coefficient/table parity and IDCT sample error <=1.
- `92f504ce8`: bounded native bit/marker writer, sequential
  and spectral progressive output, restart intervals, Annex K length-limited
  optimized Huffman tables, fixed fallback tables covering 12-bit magnitudes,
  and quantizer/metadata retention. All 600 combinations of 50 source fixtures,
  two output processes, two table policies and three restart intervals decode
  in libjpeg-turbo with exact visible coefficients/tables and zero warnings.
  All 21 tests pass in release and isolated debug builds; rustfmt, Clippy and
  rustdoc with warnings denied pass. The external report is retained under
  tests/reports/rewrite-turbo.json.
  Pixel/quality encoding and all remaining plan items are still required.
- `4723ff45b`: native integer FDCT/quantization, quality scaling,
  8/12-bit output, shared grayscale/RGB and native U8/U16 source APIs, common
  and fractional chroma sampling, explicit precision scaling, JFIF/Adobe tags,
  encoded CMYK/YCCK preservation, and coefficient inspection. The pinned C
  decoder recovers exact pre-entropy coefficients/tables for all 172 generated
  cases with zero warnings. All 132 float-encoder comparisons pass: observed
  maximum coefficient error 1 for gray/RGB and 2 for YCbCr. Size/quality data and
  justified numerical bounds are in ENCODER-VALIDATION.md and the retained
  tests/reports/pixel-turbo.json report. Full remaining scope stays open.
  Final gates: all 29 tests pass in release and isolated debug builds; rustfmt,
  Clippy and rustdoc with warnings denied pass. After restricting fixed
  Huffman tables to each process/precision's legal symbols, both external
  gates were rerun: 600 rewrites plus 172 pixel encodes still pass exactly.
- `03d5583ab`: borrowed typed JFIF/JFXX/Adobe/Exif views,
  bounded TIFF IFD/value and orientation/thumbnail access, ordered zero-copy
  ICC chunks, header inspection, explicit copy budgets, and metadata/ICC
  encoding controls. The independent libjpeg metadata API agrees on all 114
  valid cases and rejects the same six malformed ICC sequences. Pillow verifies
  both Exif byte orders after rewriting and accepts the transported real sRGB
  profile. Maximum 255-chunk profiles and 5,000 malformed Exif mutations pass.
  See METADATA-VALIDATION.md and tests/reports/metadata-turbo.json. Full remaining
  codec, streaming, conformance and performance work stays open.
  Final gates: all 37 tests pass in release and isolated debug builds; rustfmt,
  Clippy and rustdoc with warnings denied pass. Source metadata and existing
  PNG/GIF code remain untouched by the new package APIs.
- `b34941740`: explicit CMYK polarity/native raster,
  in-place polarity conversion, YCCK-to-CMYK and direct RGB output, signed green
  rounding, exact rational interpolation for fractional sampling, and bounded
  validation of public reconstruction models. The C/independent filter gate
  passes 486 comparisons across 243 unique files, observed RGB error <=3 at
  both precisions. Pillow agrees on all 26 comparable tagged 8-bit CMYK/YCCK
  cases. See COLOR-VALIDATION.md and tests/reports/colors-turbo.json.
  Encoder source-polarity and CMYK-to-YCCK transforms, streaming, rare modes,
  full oracle/conformance corpora, fuzzing and all performance work remain open.
  Final gates: all 43 tests pass in release and isolated debug builds; rustfmt,
  Clippy and rustdoc with warnings denied pass. The direct RGB memory-budget
  regression confirms conversion does not require a full CMYK intermediate.
- `9a08f1a6e`: explicit source polarity, native CMYK-to-YCCK
  conversion during area sampling, Y/K resolution presets, native-raster
  encoding entry points and source precision scaling. All 192 C decoder gates
  preserve pre-entropy coefficients exactly. Equivalent source polarities
  produce byte-identical JPEGs; all 192 C float-encoder comparisons pass, with
  maximum observed coefficient error 4. See CMYK-ENCODER-VALIDATION.md and
  tests/reports/cmyk-encode-turbo.json. Arithmetic/lossless/hierarchy, scan and
  streaming breadth, other references, fuzzing and performance remain open.
  Final gates: all 47 tests pass in release and isolated debug builds; rustfmt,
  Clippy and rustdoc with warnings denied pass.
- `16294e732`: native Annex D probability coder,
  Table D.3 and exact K.4 golden sequence, Annex F DC/AC contexts, DAC/default
  conditioning, four destinations, restart resets and separate component scans.
  All 1,314 arithmetic coefficient checks from 219 unique sources and 2,628
  native Huffman rewrites pass exactly in libjpeg-turbo, with zero warnings.
  Fifteen pinned C fixtures, every truncated prefix, DAC/restart corruption,
  resource limits and 5,000 bounded mutations run in Cargo tests. All 55 tests
  pass in release and isolated debug builds; rustfmt, Clippy and rustdoc with
  warnings denied pass. See ARITHMETIC-VALIDATION.md and the retained reports.
  Progressive arithmetic, arithmetic encoding, lossless/hierarchy, streaming,
  broader references/conformance, fuzzing and performance remain open.
- `d78552252`: SOF10 spectral selection, initial
  point transforms, DC/AC refinement, per-block band history and scan validation.
  All 1,314 progressive arithmetic files from 219 sources and 2,628 native
  Huffman rewrites match C coefficients/quantizers exactly, with zero warnings.
  Custom C scans refine from Al=13 (12-bit) or Al=10 (8-bit) through every bit,
  reverse component/band scan order and exercise shared/four-destination tables.
  Another 15 pinned fixtures, partial progressions, invalid ordering/quantizers,
  unused selectors, every truncated prefix and 2,000 additional bounded mutations
  run in Cargo tests. All 57 tests pass in release and isolated debug builds;
  rustfmt, Clippy and rustdoc with warnings denied pass. Executable snapshots
  and hashed resumable checkpoints isolate long external gates from Cargo builds.
  See ARITHMETIC-VALIDATION.md and tests/reports/arithmetic-progressive-turbo.json.
  Arithmetic encoding, lossless/hierarchy, streaming, broader references and
  conformance, coverage-guided fuzzing and all performance work remain open.
- `400df4bc5`: Annex D.1 interval/carry/byte-stuffing
  writer, exact K.4 golden bytes, Annex F DC/AC models, configurable DAC values,
  sequential SOF9 and spectral progressive SOF10 output, shared pixel/raster
  APIs, metadata and restart controls. The writer has constant entropy state
  and emits into the bounded output without a second entropy buffer or Huffman
  histogram pass. All 4,140 output files from 345 distinct inputs plus 456
  coefficient-extrema encodes match C/native coefficients exactly, with zero C
  warnings. All 113 native probability states match pinned reference data.
  An endpoint audit corrected inherited Huffman limits: arithmetic decisions
  use the full signed 16-bit model independently of sample precision. Another
  228 C progressive full-model cases match exactly; two are pinned with C
  coefficients, including -32768 refinements from Al=13/10.
  The shared MCU traversal leaves all 600 original Huffman output hashes
  unchanged. Tests cover arithmetic budgets/conditioning/markers/metadata,
  public coefficient errors, five color spaces and 200,000 adaptive/fixed
  decisions with carry/stuffing/restart handling. All 63 tests pass in release
  and isolated debug builds; rustfmt, Clippy and rustdoc with warnings denied
  pass. See ARITHMETIC-VALIDATION.md and the retained reports. Validation resumed
  from verified checkpoints after concurrent builds exhausted disk space;
  serialized builds completed within available space.
  Successive approximation encoding, lossless/hierarchy, DNL/abbreviated modes,
  streaming, other reference/conformance corpora, coverage-guided fuzzing and
  all benchmark/optimization work remain required.
- `9e1773b11`: generated successive approximation
  through Al=13, borrowed caller `ScanSpec` scripts, DC/AC point transforms and
  refinements for Huffman/arithmetic, custom spectral bands, partial progressions
  and separate/mixed sequential scans. Scan planning validates each actual MCU,
  so layouts such as YCCK 4:1:0 and four 4x4 components can use separate scans.
  The encoder/decoder now enforce SOS component order from T.81 A.2.3/B.2.3.
  Correction storage is bounded to 63 bits; no second coefficient/scan vector
  is allocated by the encoder. All 5,214 structured C/native comparisons from
  409 distinct inputs and 128 randomized legal scan graphs match coefficients
  and quantizers exactly, with zero C warnings. Random graphs span 47..427 scans,
  independent band precisions and mixed component/DC/AC order. Full signed
  16-bit model extrema also transcode through Huffman point transforms exactly.
  All 600 default Huffman output hashes remain unchanged. All 69 tests pass in
  release and isolated debug builds; rustfmt, Clippy and rustdoc with warnings
  denied pass. See PROGRESSIVE-ENCODER-VALIDATION.md and retained reports.
  Validation resumed from verified checkpoints after disk exhaustion; temporary
  builds used an alternate drive and the final matrix used a bounded case
  directory. Full component counts, lossless/hierarchy, DNL/abbreviated streams,
  incremental I/O, other references/conformance, coverage-guided fuzzing and
  measured performance/optimization remain required.
- `5e807d1c4`: native SOF3 at every unsigned
  precision from 2 through 16, all seven predictors, unclamped integer
  prediction, arithmetic half shifts, modulo-65536 differences, category 16
  without appended bits, per-component Pt, whole-row restarts and interleaved
  or separate scans. Exact native planes retain borrowed metadata/ICC views
  and move into the common planar/color API without copying sample buffers.
  Native conversion now covers 2..16-bit samples with wide color intermediates.
  All 4,628 C/native/source comparisons pass exactly with zero C warnings.
  Forty-five pinned fixtures span every precision and all predictor selections;
  every prefix and 4,500 bounded mutations run in Cargo tests. Literal Annex H
  cases cover subsampling, odd padding, vertically sampled restart borders,
  16-bit wraparound, entropy destinations and 255 frame components. A retained
  ten-component C-encoded file matches independently constructed source samples;
  the reference decoder rejects its own output at its fifth component scan.
  The successful C matrix stays within its supported 1x1 sampling/1..4 component
  range. All 78 tests pass in release and isolated debug builds; rustfmt,
  Clippy and rustdoc with warnings denied pass. See LOSSLESS-VALIDATION.md and
  tests/reports/lossless-turbo.json. SOF3 encoding, arithmetic/differential
  lossless, hierarchy, DNL/abbreviated streams, incremental I/O, other references
  and conformance, coverage-guided fuzzing and all measured performance work
  remain required.
- `57ee5b0d7`: native SOF3 at P=2..16, all
  seven predictors, explicit Pt, fixed/Annex K optimized tables, whole-row
  restarts, generated and borrowed caller scans, mixed predictor/Pt/restart
  parameters, original component IDs/sampling and the full 255-component range.
  Shared gray/RGB and native interleaved gray/RGB/YCbCr/CMYK/YCCK APIs preserve
  original integers without a DCT, color transform, inversion or precision
  scaling. Planar construction/rewriting reads existing samples without a raster
  copy or predictor row buffer. Generated scans split large layouts into legal
  MCUs automatically; per-scan DRI changes include clearing a prior interval.
  Metadata additions and ICC preserve/remove/replace share the DCT writer.
  All 4,448 native encoder C/native/source comparisons pass exactly, with zero
  C warnings: 2,520 raw files, 848 boundary cases covering every Pt at selected
  boundary precisions, and 1,080 rewrites of independently C-encoded sources.
  Literal Annex H entropy bytes validate category 16, full integer wrapping,
  subsampled odd edges and vertically sampled restart prediction independently
  of the native decoder. Tests also cover fractional/4x4 sampling, 255 components,
  source polarity, metadata/ICC, shared U8/U16 sources, malformed models/scripts
  and resource limits. All 600 earlier C-verified DCT output hashes stay
  unchanged after metadata helper extraction. All 84 tests pass in release and
  isolated debug builds; rustfmt, Clippy and rustdoc with warnings denied pass.
  See LOSSLESS-VALIDATION.md and tests/reports/lossless-encode-turbo.json.
  Arithmetic/differential lossless, hierarchy, DNL/abbreviated streams,
  incremental I/O, other reference/conformance corpora, coverage-guided fuzzing
  and all measured performance/optimization work remain required.
- `b842e81a5`: native SOF11 decoding/encoding at P=2..16,
  Annex H's 25 two-dimensional contexts/158 bins, four decoder destinations,
  per-table DAC/defaults, modulo differences through magnitude 32768, row
  restarts, sample planes, custom scans and shared/native pixel entry points.
  A bounded flat difference-class history stores one row per vertical sampling
  phase. Allocation is checked before use, and encoder output reserves room for
  it. Full/fractional/4x4 sampling and 255 components run through both entropy
  modes in native tests. All 4,448 earlier Huffman lossless output hashes remain
  unchanged after sharing the sample traversal.
  A new independently built JPEG XT reference is pinned at commit
  702114c8130ae818ccd2354cc03e879dac69e579. Its predictive scan setup needs a
  documented two-line table-selection fix; its coder, predictors and decoder
  are unchanged. All 3,240 exact comparisons pass with zero runtime reference
  warnings: 180 reference encodes, 540 native rewrites and 2,520 native raw
  encodes. The evidence distinguishes 857 direct streams from 2,383 comparisons
  with only Pt headers normalized, preserving entropy and applying the derived
  constant prediction offset. Both observed reference output failures (Pt and
  a partial vertical MCU row) are retained with independent expected samples;
  the native codec preserves Annex H semantics. See LOSSLESS-VALIDATION.md for
  the proof, oracle limitations, patch and reproduction commands.
  Forty-five additional pinned reference files, every prefix, 4,500 new bounded
  mutations, all destinations/default DAC, restart corruption, history budgets
  and the reference limitations run without a C compiler. All 90 tests pass in
  release and isolated debug builds; rustfmt, Clippy and rustdoc with warnings
  denied pass. The retained reports are lossless-arithmetic-jpegxt.json and
  lossless-arithmetic-refactor-huffman.json under tests/reports.
  Differential/hierarchical modes, DNL/abbreviated streams, incremental I/O,
  other reference/conformance corpora, coverage-guided fuzzing and all measured
  performance/optimization work remain required.

- `0e5b64e7e`: native DHP/EXP and SOF5/6/7/13/14/15 decoding,
  with all supported ordinary initial frames, P=2..16 lossless/P=8,12 DCT,
  original per-frame coefficient/quantizer or sample-difference retention,
  direct DC/PSV0, ordered integer expansion, modulo-65536 composition and the
  common planar/color interface. Metadata remains borrowed with global scan
  positions. Frame rules include component groups, entropy/family transitions,
  baseline exceptions, final lossless corrections and completed DHP resolution.
  Public model and memory/scan/pixel/metadata limits are checked before rendering.
  Partial-MCU completion values are discarded; visible lossless precision is
  still enforced. The validation state advances once per frame.
  An independently instrumented JPEG XT decoder recovers 18,768 DCT blocks
  and 2,226,363 sample/residual values exactly from 1,798 files. All 1,078
  pure-lossless compositions also match an independent integer row filter.
  The full matrix attempts 2,340 cases; 542 explicit reference Huffman encoder
  failures are recorded separately. Zero runtime reference warnings occur.
  Forty-two pinned cases, all 49,300 prefixes and 4,200 bounded mutations run
  without C. Literal tests cover ordered truncation, signed/Pt/wrap arithmetic,
  differential progressive refinement, additional Huffman AC categories,
  fractional cropping, 255 components and twelve lossless frames.
  The gate documents its one-line DHP precision parser fix and observation-only
  hook, plus empty DAC/EXP00 syntax normalization. Retained minimal examples
  expose C expansion truncation, missing modulo output and a constant-source
  hierarchy encoder failure. Final C raster differences are observations,
  never substituted for exact entropy validation or claimed as IDCT tolerances.
  See HIERARCHICAL-VALIDATION.md and tests/reports/hierarchical-jpegxt.json.
  All 104 tests pass in release and isolated debug builds; rustfmt, Clippy and
  rustdoc with warnings denied pass. Hierarchical encoding, DNL/abbreviated
  streams, incremental I/O, other reference/conformance corpora,
  coverage-guided fuzzing and all measured performance/optimization work remain
  required. The full user goal stays open.

- `2156c8918`: direct hierarchical frame encoder milestone: native DHP/EXP and all six
  differential processes in one bounded output buffer, borrowed coefficient
  and sample views, source-model construction without a decoder, fixed/optimized
  Huffman or arithmetic, direct DC/PSV0, signed integer Pt, generated/custom
  scripts, progressive refinement through Al=13, DRI clearing and metadata/ICC.
  Frame/process rules and global source/output/scratch limits are validated;
  nested output byte budgets apply to individual frame writers. Quantizer
  deduplication uses a fixed stack array. No frame is cloned or separately
  compressed before concatenation. DCT frames support 1..4 components;
  predictive frames support the full 255-component range with generated scans.
  All 5,200 C/native frame-data comparisons pass with zero C warnings: 42
  reference-derived and 62 native-authored inputs, 104 direct source checks and
  5,096 rewrites. Original native inputs include thirty arithmetic differential
  Pt=1 streams; rewrites preserve their restored values at Pt=0 through both
  entropy coders. Huffman signed Pt has an independent entropy-bit golden.
  The matrices cover 20,800 DCT blocks and 2,927,550 sample/residual values;
  3,900 pure-lossless compositions match an independent integer implementation.
  All 3,000 native-authored lossless comparisons also match prescribed targets.
  Pre-entropy expectations and source target planes are retained with the 62
  new fixtures; these are C-checked encoder regressions, not an independent
  standards conformance corpus. Reports distinguish repeated configurations
  from distinct output hashes. A 256-symbol fixed progressive differential
  Huffman table is valid and exercises its ten-bit run/magnitude code.
  All 600 earlier DCT and 4,448 Huffman lossless output hashes remain unchanged;
  the DCT files are also rechecked against C coefficients. All 112 tests pass
  in release and isolated debug builds; rustfmt, Clippy and rustdoc with
  warnings denied pass. See HIERARCHICAL-VALIDATION.md and the retained
  hierarchical-encode and hierarchical-refactor reports under tests/reports.
  Automatic native/shared raster pyramids and differential FDCT, DNL and
  abbreviated streams, incremental I/O, broader reference/conformance and rare
  scan/Pt coverage, coverage-guided fuzzing and all measured performance and
  optimization work remain required. The full user goal stays active.

- `3fef847f8`: automatic source-pyramid encoder milestone: native/shared source APIs with
  explicit color/precision/sampling controls, backwards global/component grids,
  unique coarse targets, replicated-edge K.5 reduction and matching reconstructed
  references. Signed differential FDCT has no level shift and checks coefficient
  range. Partial progressive band precision and per-component predictive Pt
  are realized before the next residual. Final unscaled spatial corrections
  reuse the reference buffer in place; coarse targets and unscaled updates also
  reuse storage. The common bounded writer accepts borrowed frame controls.
  All 560 source-pyramid cases pass exact independent C frame observations:
  10,512 DCT blocks and 425,916 predictive samples/residuals, zero warnings.
  Independent rational sampling/filter/signed-Pt and float-FDCT checks pass;
  full-precision coefficient error is <=1 and partial DC masks use +/-1
  candidates. All 400 final-Pt0 cases match sampled source grids exactly.
  Reports distinguish 280 original-header C comparisons from 120 base-Pt and
  160 omitted-EXP C-input workarounds; no C raster tolerance substitutes for
  entropy/source evidence. Twelve source encodes and pre-entropy expectations
  are pinned, with all 30,164 truncated prefixes and 1,200 bounded mutations
  tested. These fixtures are encoder regressions, not an independent standards
  conformance corpus. All 10,612 prior output hashes remain unchanged;
  964 are also re-decoded in C. All 122 tests pass in release and isolated
  debug builds; rustfmt, Clippy and rustdoc with warnings denied pass.
  See HIERARCHICAL-VALIDATION.md, tests/reports/pyramid-jpegxt.json and
  tests/reports/pyramid-refactor-regressions.json. Full DCT component counts,
  broader rare scan/Pt combinations, DNL/abbreviated streams, incremental I/O,
  independent conformance and additional reference corpora, coverage-guided
  fuzzing and measured performance/optimization remain required. The full
  user goal remains active and the crate is not integrated into DeadSync.

- `4e72997a9`: DNL and unknown-height milestone: whole-input decoding/encoding for all
  thirteen JPEG-1 frame processes, resolving the first-scan height before
  sample/coefficient allocation. Zero, redundant and redefined SOF heights,
  extended DNL lengths, strict adjacency, malformed MCU rows, resource limits
  and zero-height DHP component inference are covered. Public models contain
  resolved heights; raw marker events retain declarations. Shared bounded
  frame writers emit canonical DNL without an intermediate JPEG buffer.
  All 1,788 native files pass exact frame/target and canonical-byte checks:
  1,163 original-header C matches and 625 explicit known-header comparisons.
  All 336 independently C-authored DNL inputs decode exactly against C frame
  data (248 original-header, 88 known-header), and all 336 native rewrites pass
  (207 original-header, 129 known-header). Reference restart/redefinition/DNL
  failures are retained rather than counted as parsing successes. A fourth
  test-only observer records ordinary DCT coefficients before IDCT and changes
  no codec decision. Twenty-six pinned regressions cover all thirteen
  processes, all 11,861 truncated prefixes and 2,600 bounded mutations.
  All 129 tests pass in release and isolated debug builds; all 10,612 prior
  output hashes remain unchanged, with 964 fresh Turbo C decodes. See
  DNL-VALIDATION.md and the dnl reports under tests/reports.
  Abbreviated streams, incremental I/O, full DCT component counts, broader
  independent conformance and IJG/mozjpeg corpora, coverage-guided fuzzing and
  all measured performance/optimization work remain required. The full goal
  stays active; no application integration is made.

- `16ec8b10c`: abbreviated table/image milestone: caller-owned persistent DQT/DHT banks,
  validated external definitions, table-only metadata/serialization and direct
  table/image emission through all existing native/source encoders. All thirteen
  frame processes reuse caches without cloning compiled tables per image.
  SOI resets DAC/DRI; later table redefinitions stay at their image scan/frame.
  Empty unused Huffman destinations are retained under B.2.4.2 and fail if used
  to decode a symbol. Combined output/frame budgets include the table packet;
  encoder write failures clear state to prevent omission of undelivered tables.
  The genuine Turbo B.5 API gates pass 3,238 C-authored and 1,524 native image
  cases, each in separate-file and concatenated delivery on one codec object.
  All 1,932 native rare-process cases pass independent C frame-data checks of
  equivalent interchange streams: 9,984 DCT blocks and 1,291,572 sample/residual
  integers. Those are not counted as C B.5 parser successes; 96 explicit EXP00
  C-input workarounds are retained. The C lossless YCbCr/YCCK client uses generic
  native component input plus Adobe tags to avoid its default selector reset
  mismatch; no production parser relaxation is made. The zero-Li check has a
  separate C/native gate, including expected rejection of an empty used table.
  Forty-four pinned sessions contain 64 images across all process markers;
  34,201 image prefixes and 3,300 bounded mutations pass, alongside external
  table prefix/mutation checks. These are encoder regressions, not an independent
  conformance corpus. All 10,612 earlier default-writer hashes are unchanged,
  with 964 fresh Turbo C decodes; the subsequent zero-Li parser exception has
  targeted C evidence. All 139 tests pass in release and isolated debug builds;
  rustfmt, Clippy and rustdoc with warnings denied pass.
  See ABBREVIATED-VALIDATION.md and abbreviated reports in
  tests/reports. Incremental byte I/O, full DCT component counts, broader rare
  scan/Pt combinations, independent conformance/IJG/mozjpeg corpora,
  coverage-guided fuzzing and measured performance/optimization remain required.
  The full goal remains active; there is no application integration.

- `620ea620d`: generic component-count milestone: sequential DCT coefficient decoding,
  native construction, ordinary/abbreviated/direct hierarchical encoding and
  source pyramids support 1..255 components at P=8/12. Progressive DCT keeps
  the normative 1..4 range. Generated scans obey Ns/MCU bounds; predictors use
  scan-local slots; sequential DQT destination reuse supports distinct tables
  per component. Earlier legal default outputs remain byte-identical.
  The native gate passes 1,367 cases, including every sequential count, 216
  hierarchies and 128 progressive cases. IJG libjpeg 10 checks all 1,151 ordinary
  cases, including fractional sampling and 255 components. JPEG XT checks 1,096
  native cases: 884,733 DCT blocks and 2,706,492 sample/residual integers.
  All 672 C-authored cases match IJG/JPEG XT/native decoding. Supplemental gates
  cover 18 fixed-Huffman cases, 198 unchanged earlier C observations, and 47
  pinned IJG replays including 12 genuine B.5 pairs. Quantizer snapshots are
  checked independently. Reference configuration, observer/cleanup patches,
  intermediate tool provenance and 108 explicit EXP00 C-input copies are
  recorded; rare fractional C coverage remains open. All 10,612 prior default
  hashes are unchanged, with 964 fresh Turbo decodes. The 75-session retained
  corpus passes 36,586 prefixes/boundaries and 2,600 bounded mutations.
  All 145 tests pass in release and isolated debug builds; rustfmt, Clippy and
  rustdoc with warnings denied pass. See COMPONENT-VALIDATION.md and component
  reports in tests/reports. Incremental byte I/O, broader IJG/mozjpeg and rare
  combinations, independent conformance corpora, coverage-guided fuzzing and
  measured performance/optimization remain required. The full goal stays active;
  there is no application integration or added production codec dependency.

- mozjpeg and three-C ordinary-process validation milestone: unmodified pinned
  mozjpeg 4.1.1 8/12-bit builds and a public classic-API observer. All 759 inputs
  match native/Turbo/IJG/mozjpeg visible coefficients and quantizers exactly:
  612 mozjpeg source encodes, 78 native rewrites and 69 earlier regressions.
  There are 758 raw/color comparisons, with observed errors at most one IDCT
  and three nearest-color native sample units. One full-magnitude synthetic
  arithmetic model keeps coefficient-only checks and records the C pixel
  range-limit difference; no source-image tolerance is widened. Upstream's
  12-bit arithmetic exclusion is recorded for 15 replays. Separate original
  and passing inputs document the scan-optimization/restart producer defect
  and empty DHTs during explicit DC refinement; native parser rules remain
  unchanged. Thirty source images plus coefficient/float-plane expectations
  pass 16,666 prefixes/boundaries and 3,000 bounded mutations.
  All 148 tests pass in release and isolated debug builds; rustfmt, Clippy and
  rustdoc with warnings denied pass. See
  MOZJPEG-VALIDATION.md and mozjpeg reports in tests/reports. These are configured
  interoperability regressions, not an independent standards-conformance corpus
  or performance results. Incremental I/O, broader reference/conformance and
  rare combinations, coverage-guided fuzzing and all measured optimization
  remain required; the full goal stays active with no application integration.
