# Native sequential arithmetic decoding

SOF9 (extended sequential arithmetic DCT) is decoded at 8/12-bit precision.
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

Interleaved and separate component scans use the existing MCU traversal.
`decode_coefficients` preserves original quantizers and visible coefficients;
reconstruction uses the same native IDCT/color pipeline as Huffman images.
`encode_coefficients` can transcode arithmetic coefficients to sequential or
spectral progressive Huffman output without another DCT.

The test-only reference is libjpeg-turbo 3.2.1, pinned and built as described
in tests/oracle/README.md. `arithmetic_transcode.c` uses its public coefficient
API to encode arithmetic JPEGs, with custom conditioning and table selectors.
It can write an interleaved sequential scan, separate sequential component
scans, or progressive scans. The latter are reserved for the next decoder
stage. Only critical image parameters are copied; this coefficient gate does
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

SOF10 progressive arithmetic decoding, arithmetic encoding, predictive lossless
arithmetic and hierarchical arithmetic remain open. Independent IJG/mozjpeg and
external conformance fixtures, broader corruption semantics, streaming,
performance and allocation measurements are still required. No speed claim or
complete arithmetic conformance claim is made by this milestone.

Normative source: ITU-T T.81, Annexes D/F and K.4,
https://www.w3.org/Graphics/JPEG/itu-t81.pdf.
