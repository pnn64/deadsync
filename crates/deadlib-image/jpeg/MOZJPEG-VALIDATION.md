# mozjpeg and the three ordinary-process C references

The test-only build uses unmodified mozjpeg 4.1.1 from the official repository:

- Release: <https://github.com/mozilla/mozjpeg/releases/tag/v4.1.1>
- Commit: `a2d2907ff023227e80c1e4efa809812410275a12`
- Archive: <https://codeload.github.com/mozilla/mozjpeg/tar.gz/refs/tags/v4.1.1>
- Archive SHA-256: `66b1b8d6b55d263f35f27f55acaaa3234df2a401232de99b6d099e2bb0a9d196`

`tests/build_mozjpeg.py` verifies the archive and every original source file
before building separate 8/12-bit static libraries and command-line producers.
It uses the upstream CMake configuration, with a compatibility policy option
for current CMake. SIMD, shared libraries, TurboJPEG and PNG input are disabled.
This is a numerical reference configuration, not a speed benchmark. Upstream
`WITH_12BIT` disables arithmetic coding; the gate explicitly records skipped
12-bit arithmetic inputs rather than treating them as successful comparisons.

`tests/oracle/classic_oracle.c` uses public coefficient, raw-data and scanline
APIs, compiled against each precision's JSAMPLE layout. It emits visible
natural-order JCO1 coefficients/quantizers, LE-u16 float-IDCT component planes
and native-precision full-resolution color samples with nearest upsampling.
Fractional upsampling and unknown color spaces omit the color output. No codec
source is modified or linked into Rust. Executable identities and configurations
are retained in `tests/reports/mozjpeg-build.json`.

## Differential matrix

`tests/reference_mozjpeg.py` requires exact coefficients, quantizers, visible
block dimensions, component IDs and sampling to agree between the native
decoder, mozjpeg, libjpeg-turbo 3.2.1 and configured IJG libjpeg 10. The Turbo and
IJG references are the existing pinned builds. mozjpeg shares implementation
ancestry with Turbo; three library builds do not constitute three unrelated
implementations or an independent conformance corpus.

All **759 comparisons** pass exact native/three-C coefficient checks with no
runtime C warnings. There are **758 checked raw/color comparisons**, with
observed native maxima of **one** float-IDCT sample unit and **three** nearest
color sample units. One synthetic entropy-model input has the explicit pixel
exclusion below. Fifteen 12-bit arithmetic replays are explicitly skipped by
mozjpeg's upstream configuration.

The matrix includes:

- 486 mozjpeg source encodes: 8-bit Huffman/arithmetic and 12-bit Huffman,
  sequential/progressive, gray/RGB and seven YCbCr sampling layouts,
  qualities 20/83/100, constant/checker/noise sources, 1x1/17x19/47x33 dimensions
  and restart rows 0/1/7. Progressive restart cases explicitly use `-fastcrush`.
- 78 native coefficient rewrites of selected source encodes, crossing fixed
  and optimized Huffman, both ordinary coding processes, arithmetic at 8 bits,
  and restart intervals 0/1/7. Coefficients must also equal the pre-rewrite IJG
  observation. These bytes come from the native writer.
- 81 default scan-optimized progressive encodes of nontrivial noise sources,
  with no restarts, crossing all nine layouts and three quality settings.
- 45 custom encodes: reversed sequential component scans, explicit AC bit
  refinement from Al=2, nine quantization table families, three trellis tuning
  choices and disabled trellis. The refinement script sends full DC precision;
  full DC refinement's reference-producer limitation is retained separately.
- 69 earlier pinned ordinary, arithmetic and CMYK/YCCK regressions, with
  15 explicit 12-bit arithmetic exclusions.

Raw native component samples must differ from mozjpeg's float IDCT by at most
one coded sample unit. This separates transform precision from upsampling/color
rounding. Nearest grayscale output has a limit of one; three/four-component comparisons
allow at most four native sample units, following the earlier color gate's
matrix/rounding limits. Reported maxima and means are observed, not substituted
for exact coefficient checks. Turbo/mozjpeg raw-plane differences are recorded
separately. This gate does not compare final RGB from CMYK, fancy upsampling,
ICC-managed pixels, encoder distortion/size tradeoffs or performance.
The synthetic full-magnitude arithmetic-model regression has exact coefficient
checks only. Its coefficients were constructed for entropy boundaries rather
than obtained from bounded source samples. C float range-limit behavior differs
by up to 255 in that case; the report retains the observation and explicit pixel
exclusion. It does not widen the one-unit source-image IDCT tolerance.

Thirty selected original source encodes and their exact coefficient/float-plane
expectations are retained in `tests/fixtures/mozjpeg`. Cargo tests check them
without C libraries, exercise native rewrites through both entropy families,
and check truncation boundaries and bounded mutations. These are reproducible
interop regressions, not separately authored standards-compliance vectors.
The 90 hashed JPEG/coefficient/plane artifacts plus manifest occupy 313,152
bytes. Cargo checks 16,666 prefixes/boundaries and 3,000 bounded mutations,
and retains both producer failures and their separately labeled passing inputs.
All 148 tests pass in release and isolated debug builds. rustfmt, all-target
Clippy and rustdoc with warnings denied pass; the new Python drivers compile.

## Explicit reference-producer limitations

`tests/reference_mozjpeg_limitation.py` retains four inputs and original decoder
diagnostics. The valid-stream gate never accepts runtime C warnings.

1. A 17x19 checker, quality 20, 4:2:2 progressive image with restart rows 1
   and mozjpeg's default scan optimization contains extraneous entropy bytes.
   All three C clients warn; the native decoder rejects entropy padding.
   The same source/options with `-fastcrush` decode warning-free and have exact
   native/C coefficients. No entropy recovery is added to the native decoder.
2. An explicit DC/AC refinement script causes mozjpeg's combined-table writer
   to emit two `FF C4 00 02` segments during DC refinement. T.81 B.2.4.2 defines
   a DHT segment with at least one table specification; these contain none.
   All C clients ignore them, while the native parser rejects `empty DHT`.
   A separately labeled copy removes exactly those eight marker bytes, leaving
   all entropy unchanged. Its native/C coefficients equal the original C
   observations exactly. This is distinct from a real table whose sixteen Li
   values are zero; that earlier supported case is unchanged.

See [T.81 Annex B](https://www.w3.org/Graphics/JPEG/itu-t81.pdf), the unmodified
`emit_multi_dht` implementation in the pinned source, and
`tests/reports/mozjpeg-limitations.json`. These findings describe the tested
release/configuration and inputs; they do not imply that all optimized output
or all progressive restart combinations fail. Native parser rules are unchanged.

## Reproduction

From the package directory on Windows/MSVC, with the existing Turbo/IJG tools:

```text
python tests/build_mozjpeg.py --work experiments/mozjpeg-reference
cargo build --release --examples --jobs 1
python tests/reference_mozjpeg.py --build experiments/mozjpeg-reference/build.json --native INSPECT --colors COLORS --rewrite REWRITE --turbo TURBO --ijg IJG --out experiments/mozjpeg-gate --fixtures tests/fixtures/mozjpeg
python tests/reference_mozjpeg_limitation.py --build experiments/mozjpeg-reference/build.json --native INSPECT --turbo TURBO --ijg IJG --out experiments/mozjpeg-limitations --fixtures tests/fixtures/mozjpeg-limitations
cargo test -p deadlib-image-jpeg --test mozjpeg -- --nocapture
```

The matrix snapshots executables and records hashes, source options, input
hashes, per-case errors and an identity-checked resume checkpoint. Rebuilds can
have different executable hashes because the upstream build embeds its date;
each actual run retains its own identities. Independent conformance corpora,
broader IJG/mozjpeg process coverage, incremental I/O, coverage-guided fuzzing
and measured optimization remain required.
