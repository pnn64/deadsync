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
- Predictive lossless Huffman encoder milestone: native SOF3 at P=2..16, all
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
