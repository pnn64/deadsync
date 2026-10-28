# Native sequential and progressive arithmetic coding

SOF9 (extended sequential arithmetic DCT) and SOF10 (progressive arithmetic
DCT) are decoded at 8/12-bit precision.
The crate implements the binary arithmetic decoder in T.81 Annex D, the 113
probability states in Table D.3, and the DC/AC bin assignments in F.1.4.
Implementation was derived from the standard, without production reference
code or codec dependencies. The Table D.3 constants were transcribed from a
rendered page: text extraction corrupts some digits in the published PDF.

The core passes the exact 256 decoded decisions of T.81 K.4, including its
stuffed FF byte and trailing zero padding. DC conditioning defaults to L=0,
U=1; AC defaults to Kx=5. DAC segments override each of four DC/AC table
destinations. Statistics are shared by table destination, while previous DC
classes and predictors belong to each scan component. Every scan/restart
resets the applicable statistics, predictors and arithmetic registers.
Magnitude and zero-run loops are bounded before indexing or coefficient writes.
Arithmetic DC differences and AC values use the full signed 16-bit model
specified by F.1.4/F.2.4.3, independent of 8/12-bit sample precision. Huffman
category restrictions remain process/precision specific. Arithmetic-to-Huffman
rewriting requires coefficients/differences to fit those target categories.

Interleaved and separate component scans use the existing MCU traversal.
`decode_coefficients` preserves original quantizers and visible coefficients;
reconstruction uses the same native IDCT/color pipeline as Huffman images.
`encode_coefficients` can transcode arithmetic coefficients to sequential or
spectral progressive Huffman output without another DCT.

The test-only reference is libjpeg-turbo 3.2.1, pinned and built as described
in tests/oracle/README.md. `arithmetic_transcode.c` uses its public coefficient
API to encode arithmetic JPEGs, with custom conditioning and table selectors.
It can write an interleaved sequential scan, separate sequential component
scans, or progressive scans. Only critical image parameters are copied; this coefficient gate does
not measure preservation of source APP metadata or pixel color interpretation.

The gate uses the 50 checked-in Huffman fixtures and the native pixel encoder
corpus. Source bytes are deduplicated. Six configurations per source cover
restart intervals 0/1/7, default conditioning, L/U pairs 0/0, 3/7 and 15/15,
AC splits 1/5/32/63, all four table destinations, shared tables and separate
component scans. Sources include 8/12-bit precision, quality 1/50/83/90/100,
wide quantizers, gray/RGB/YCbCr/CMYK/YCCK and common/fractional sampling.

For each arithmetic file, both the native decoder and C decoder must recover
the original visible coefficients and quantizers **exactly**, with no C warnings.
Two native Huffman rewrites are then decoded by C and must match exactly.
All **1,314 arithmetic files from 219 unique sources** and **2,628 native
Huffman rewrites** pass exactly, with zero C warnings. The initial, separate
jpegtran gate passed all 150 default-conditioning files and 300 native rewrites.
Its report is retained separately. The wider run is recorded in
tests/reports/arithmetic-turbo.json; no pixel tolerance is involved in these
coefficient comparisons.

```text
cmake --build <oracle-build> --config Release
cargo build -p deadlib-image-jpeg --examples
cargo run -p deadlib-image-jpeg --example encode_fixtures -- <pixel-corpus>
python crates/deadlib-image/jpeg/tests/reference_arithmetic.py --transcode <jpeg-arithmetic-transcode> --oracle <jpeg-oracle> --inspect target/debug/examples/inspect.exe --rewrite target/debug/examples/rewrite.exe --pixel-corpus <pixel-corpus> --pin crates/deadlib-image/jpeg/tests/fixtures/arithmetic --out <arithmetic-gate>
```

The 15 checked-in arithmetic JPEGs and C `.coef` outputs run without a C
toolchain. Their manifest records source/output hashes, conditioning, reference
commands and expected coefficient hashes. Focused tests also remove DAC tables
to verify defaults, reject invalid DAC lengths/selectors/bounds, corrupt/remove
restart markers, accept marker fill, reject surplus restarts, test every
truncated prefix and resource limits, and run 5,000 bounded deterministic
mutations. Mutation smoke tests do not replace coverage-guided fuzzing.

Successive approximation encoding, predictive lossless arithmetic and
hierarchical arithmetic remain open. Independent IJG/mozjpeg and
external conformance fixtures, broader corruption semantics, streaming,
performance and allocation measurements are still required. No speed claim or
complete arithmetic conformance claim is made by this milestone.

Normative source: ITU-T T.81, Annexes D/F/G and K.4,
https://www.w3.org/Graphics/JPEG/itu-t81.pdf.

## Progressive arithmetic

The SOF10 decoder shares scan-order and band-precision validation with the
Huffman progressive decoder. Initial DC/AC scans use the Annex F contexts with
the sign-preserving AC point transform. DC refinement decodes fixed-probability
two's-complement bits. AC refinement follows G.1.3.3/Table G.2: the prior
end-of-band is computed from nonzero coefficients of the current block/band;
EOB decisions are bypassed before that position. Existing nonzero coefficients
use SC correction bins; newly nonzero coefficients use S0 and a fixed sign
decision. Arithmetic EOBs do not use Huffman EOB runs.

The progressive reference gate uses the same 219 source images and six
conditioning/table/restart configurations. Four configurations use the C
library's standard progressive sequence. Two use a custom script that starts
at Al=13 for 12-bit sources (Al=10, the reference encoder's limit, for 8-bit),
refines every bit, visits components in reverse scan order and sends the
6..63 band before 1..5. This produces up to 126 scans per file. The script is
implemented only in the test C client. Run the same command with
`--progressive`, a distinct `--out`, and
`--pin crates/deadlib-image/jpeg/tests/fixtures/arithmetic-progressive`.
All **1,314 progressive arithmetic files** and **2,628 native Huffman rewrites**
match the original visible coefficients and quantizers exactly, with zero C
warnings. The report is tests/reports/arithmetic-progressive-turbo.json.
The report records executable hashes and source/expected-coefficient hashes.
Use executable copies outside Cargo's target directories for long gates:
Cargo checks can remove or replace example binaries. `--resume` verifies the
same tools, inputs, arithmetic bytes and all four coefficient outputs before
reusing a completed checkpoint. Checkpoints are written only after both C
rewrites pass without warnings.

Another 15 pinned JPEGs and C coefficient outputs exercise these progressions
without reference tools. Focused tests verify negative DC refinement from
Al=13, partial progressions, unused table selectors, quantizer stability,
invalid scan ordering, every truncated prefix and 2,000 additional bounded
mutations. Broader independent conformance and coverage-guided fuzzing remain
required.

## Native arithmetic encoding

`CodingOptions::entropy_coding = EntropyCoding::Arithmetic` selects native
SOF9 sequential or SOF10 spectral progressive output. Arithmetic coding applies
to coefficient rewriting and all pixel/native-raster entry points. The writer
emits explicit DAC tables; `ArithmeticConditioning` configures L/U (0 <= L <= U
<= 15) and Kx (1..63). Both encoder table groups use those values. Group zero
codes the first component; group one codes the remaining components. Huffman
optimization is ignored in arithmetic mode.

The encoder implements the Annex D.1 interval coder and F.1.4 coefficient
decision models directly. It reproduces the 29 stuffed entropy bytes of K.4
exactly. A 200,000-decision test exercises adaptive/fixed probabilities, carry
propagation, stuffed FFs and restart register resets. Carry resolution uses a
pending byte and FF count; trailing zero runs remain pending and are discarded
at Flush as required by D.1.8. Stuffed zeros are preserved. Output uses the
existing bounded writer without a separate entropy vector. Arithmetic mode
does not run Huffman frequency collection. Both entropy modes share MCU/block
traversal, with mode-specific predictors and restart state.

The external encoder gate reads pinned Huffman/arithmetic images plus the
native pixel and CMYK/YCCK corpora. Six restart/conditioning configurations are
written in both sequential and spectral progressive form. C and native decoders
must recover the original visible coefficients and quantizers exactly. Native
tests additionally verify APP preservation, output process/DAC/DHT/DRI markers,
exact output budget boundaries, conditioning validation, malformed coefficients,
and pixel encoding with all five color spaces at both precisions. A Huffman DC
coefficient range check also prevents a later out-of-range value from slipping
through when its difference from the preceding value fits.

All **4,140 native arithmetic outputs from 345 distinct JPEG input files**
pass both decoder gates exactly, with zero C warnings. The retained report is
tests/reports/arithmetic-encode-turbo.json. The independently transcribed native
probability table also matches all 113 Qe/transition/switch entries of the pinned
C reference table; tests/reports/arithmetic-table-turbo.json records the data
comparison and hashes. `reference_arithmetic_table.py` performs this test-only
fact comparison; no reference implementation is used in the production crate.

Another 38 deterministic inputs in `examples/arithmetic_fixtures.rs` exercise
the full signed 16-bit DC/AC model including -32768, mixed signs, dense/sparse
blocks, 1/65535 quantizers, gray/RGB/CMYK/YCCK and 1x1/1x17/17x1 image edges.
All 456 arithmetic encodes from this corpus pass C/native coefficient gates
exactly, with zero C warnings. The retained report is
tests/reports/arithmetic-encode-extrema-turbo.json. Rechecking the common MCU
walker against the original report also leaves all 600 Huffman rewrite outputs
byte-identical, with exact C coefficients and zero warnings; the repeated
report is tests/reports/rewrite-arithmetic-refactor-turbo.json.

The C encoder additionally generates 228 full-model progressive cases from
these inputs. Both decoders recover coefficients exactly, with zero warnings,
including negative endpoints refined from Al=13/10. See
tests/reports/arithmetic-full-model-progressive-turbo.json and
`reference_arithmetic_precision.py`. Two representative C progressions are
checked in under tests/fixtures/arithmetic-model and run without C tools.
These tests isolate entropy-model limits; IDCT/color accuracy follows the
separate pixel validation gates.

```text
cargo build -p deadlib-image-jpeg --release --example rewrite --example inspect
python crates/deadlib-image/jpeg/tests/reference_arithmetic_encode.py --native <rewrite> --inspect <inspect> --oracle <jpeg-oracle> --corpus <pixel-corpus> --corpus <cmyk-corpus> --out <arithmetic-encoder-gate>
cargo run -p deadlib-image-jpeg --release --example arithmetic_fixtures -- <extrema-inputs>
python crates/deadlib-image/jpeg/tests/reference_arithmetic_encode.py --only-corpus --native <rewrite> --inspect <inspect> --oracle <jpeg-oracle> --corpus <extrema-inputs> --out <arithmetic-extrema-gate>
python crates/deadlib-image/jpeg/tests/reference_arithmetic_precision.py --transcode <jpeg-arithmetic-transcode> --oracle <jpeg-oracle> --inspect <inspect> --corpus <extrema-inputs> --out <full-model-progressive-gate> --pin crates/deadlib-image/jpeg/tests/fixtures/arithmetic-model
```

This driver copies executables into each gate's `tools` directory before
starting, records tool/input/output hashes, and writes a checkpoint only after
both decoders agree without warnings. `--resume` verifies retained bytes and
hashes before reuse. An incomplete final checkpoint record from an interrupted
write is discarded and that case is rerun; earlier corruption is an error.
Production code still has only the shared `deadlib-image` dependency.
Arithmetic successive approximation scan generation, predictive lossless and
hierarchical entropy coding, other reference implementations, streaming and
measured speed/size/quality comparisons remain part of the overall plan.
