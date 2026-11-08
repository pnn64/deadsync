# Official T.83 test data

The retained independent corpus comes from ITU's public T.83 (11/1994) test
signals, rather than a native or C encoder regression generator:

- [Official recommendation catalogue](https://www.itu.int/ITU-T/recommendations/rec.aspx?id=2635)
- [Public test-file catalogue](https://www.itu.int/mws/api/testsignals/signalfiles?ts_id=6)
- [Original data archive](https://www.itu.int/wftp3/public/t/testsignal/SpeImage/T83/v1994_11/T83_data_files.zip)
- Archive size: 3,924,283 bytes
- SHA-256: `9f5ed0a74a3087c6c93f4cbb0bd5128c2a87b128e4f59f5b6649e32941da7069`

`tests/fetch_t83.py` verifies the archive and copies only unchanged compressed
streams and numeric reference files, deduplicating repeated archive members.
It does not extract arbitrary paths or include the standard's text. The
manifest records all 45 original files, their hashes and archive member names.
The data retain their ITU/ISO/IEC copyright; see the fixture NOTICE and original
archive Readme. Two additional `.coef` files are separately labeled C/native
observations and are not official DREF data.

## Original streams and reference decoders

All 21 unique streams are retained: 20 images plus the B1 table packet. They
exercise baseline/extended/progressive Huffman and arithmetic DCT at 8/12 bits,
8/16-bit predictive lossless in both entropy families, arbitrary component IDs,
sampling products up to the ten-unit MCU limit, scan scripts, restarts, four
table destinations, opaque metadata and real abbreviated tables/image syntax.
B2 contains 255 components and consumes the actual B1 packet on a persistent
native/IJG table object.

`tests/reference_t83.py` checks original stream bytes without header repairs:

| Check | Images | Required result |
| --- | ---: | --- |
| Native DCT / IJG libjpeg 10 | 16 | Exact visible coefficients, quantizers and geometry; includes genuine B1/B2 |
| Native DCT / libjpeg-turbo 3.2.1 | 15 | Exact coefficients/quantizers; excludes Turbo's selector lookup limit on B2 |
| Native DCT / mozjpeg 4.1.1 | 11 | Exact coefficients/quantizers; excludes B2 and four 12-bit arithmetic inputs |
| Native lossless / official SRC files | 4 | Exactly 219,640 sample integers |
| JPEG XT lossless PGX / official SRC files | 4 | The same exact samples, original headers, no Pt normalization |

All ordinary four-component streams at each precision have identical numeric
coefficient models. B2 repeats four such models with different IDs; all 255
selectors and their visible coefficient grids are checked in Cargo tests.
The model snapshots have their own executable identities and hashes. Reference
versions, scalar configurations, source pins and patches are the existing
documented builds; no reference codec enters production dependencies.

## Transform accuracy and reference interpretation

EREF/DREF values are signed big-endian 16-bit integers in zigzag order. They
must not be compared indiscriminately to entropy-decoded coefficients. DREF
describes a reconstructed-sample/FDCT/quantization chain. The published
[T.84 G.3.3 construction procedure](https://www.itu.int/rec/dologin_pub.asp?id=T-REC-T.84-199607-I%21%21PDF-E&lang=f&type=items)
and the T.83 definitions distinguish these stages.

The test-only module `tests/support/t83_transform.rs` calls the actual native
FDCT/quantizer and IDCT used by the encoders and decoder. It independently
constructs a double-precision cosine basis from the mathematical equations,
without sharing the native Q20 constants or transform functions.

- Encoder: 119,936 native coefficients are checked against EREF, using the
  supplied eight-bit component sources, 16x sample scaling for 12 bits, the
  official quantizers and repeated source edges. Maximum observed error is one
  quantized coefficient unit. The independent f64 FDCT also stays within one.
- Decoder: 119,936 native reconstructed/forward-transformed coefficients are
  checked against DREF. Maximum observed error is one quantized coefficient
  unit; native IDCT samples differ from the independent f64 IDCT by at most one
  sample unit. All stages retain clipping and level-shift semantics explicitly.

The distributed DREF arrays exhibit different edge treatments: eight-bit data
include reconstructed samples throughout partial 8x8 edge blocks; twelve-bit
data repeat the visible image edge after reconstruction. The verifier implements
both explicitly. This is an empirical interpretation of the provided arrays,
not a change to the codec's cropping or an assertion about an unstated universal
padding rule. The independent f64 reconstruction chain agrees with every
reference coefficient within one unit. Normalization/summation around exact
half-bucket ties can choose adjacent integers; no larger discrepancy is accepted.

These are explicit numerical gates on the supplied data. A formal full Part 2
compliance claim additionally requires auditing all applicable acceptance
procedures and criteria; stream acceptance or an observed pixel tolerance alone
is insufficient. The available preview of T.83 does not include Annex A, and
this work does not replace that audit with an inferred certification.

## Native writers and malformed input

Cargo tests rewrite the official models in 118 configurations: both entropy
families, all seven predictive selectors, separate/interleaved lossless scans,
progressive refinement and restarts. Original lossless SRC integers and visible
DCT coefficient models remain exact. No illegal 255-component progressive frame
is generated.

`tests/reference_t83_encode.py` separately checks native coefficient output
against the C APIs, crossing fixed/optimized Huffman, arithmetic,
sequential/progressive and restart intervals 0/7. The B2 source for the standalone
rewrite CLI is explicitly converted to equivalent interchange by inserting its
initial B1 tables. Original B1/B2 parser checks remain separate; no entropy is
changed in that source copy. These writer checks do not substitute for the
source-pixel EREF transform checks above.

All 186 native coefficient-output configurations pass: 186 exact IJG,
180 exact Turbo and 156 exact mozjpeg comparisons. Unsupported reference
component/precision combinations are recorded explicitly, and the real B1/B2
source parser checks are not counted again as abbreviated-output successes.

The malformed test covers 4,284 prefixes/marker boundaries and 672 bounded
mutations, including every B1 prefix. Large streams use sparse boundaries rather
than claiming exhaustive truncation of every byte. Pixel, storage and scan
ceilings apply during mutations. Coverage-guided fuzzing remains open.

All 153 tests pass in release and isolated debug builds. rustfmt, all-target
Clippy and rustdoc with warnings denied pass, as do Python driver compilation
and original/observed artifact hash audits. Production codec algorithms are
unchanged by this validation milestone.

## Reproduction and limits

From the package directory, using the existing reference builds:

```text
python tests/fetch_t83.py --work experiments/t83 --fixtures tests/fixtures/t83
python tests/reference_t83.py --native ABBREVIATED --ijg IJG --turbo TURBO --moz8 MOZ8 --moz12 MOZ12 --reference JPEGXT --out experiments/t83-gate --fixtures tests/fixtures/t83
python tests/reference_t83_encode.py --rewrite REWRITE --ijg IJG --turbo TURBO --moz8 MOZ8 --moz12 MOZ12 --decoded experiments/t83-gate --out experiments/t83-encode --fixtures tests/fixtures/t83
cargo test -p deadlib-image-jpeg t83_transform -- --nocapture
cargo test -p deadlib-image-jpeg --test t83 -- --nocapture
```

The original archive says I1, M1, Q1/Q2, R1/R2, S1/S2 and T1/T2 are unavailable.
They are not fabricated or counted as passing; the official download provides
no hierarchical test stream. Existing independent JPEG XT hierarchy gates
remain separate. Full remaining conformance audits, broader rare combinations,
incremental byte I/O, coverage-guided fuzzing and measured performance are still
required. No application integration or production codec dependency is added.
