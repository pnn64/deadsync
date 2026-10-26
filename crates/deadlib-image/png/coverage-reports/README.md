# Native library coverage

This snapshot uses Rust 1.101.0-nightly (76456a688, 2026-10-25) and that
toolchain's LLVM coverage tools in Debian WSL. All 22 Rust test executables run:
76 tests pass and five optional C tests are ignored. The report maps only the
ordinary shared/PNG library in 20 integration executables. Library unit-test
binaries are excluded so embedded test functions do not enter the totals.
Explicit source operands also exclude reference codecs, test harnesses and stdlib.

`huffman-report.txt` and `huffman-summary.json` report 96.60% lines, 92.33% regions
and 83.75% instrumented branch outcomes across 20 native source files. These are
integration-test coverage metrics, not native fuzz coverage or conformance proofs.
LLVM does not assign runtime counters to generated constants and const evaluation.
Its branch instrumentation and line/region aggregation are toolchain-specific.

The manifest records production/test source hashes, instrumented executable
hashes, environment and commands. The command log retains test output and an
initial reporting-CLI error: all tests had passed, but LLVM needed a positional
object before the source operands. Reporting resumed from the same retained
profiles after correcting only the script. Native source remained unchanged.
The final script automates the corrected workflow:

```sh
rustup component add llvm-tools-preview --toolchain nightly
python3 crates/deadlib-image/png/scripts/coverage_native.py --branches \
  --target-dir /tmp/deadlib-image-native-coverage-target \
  --output-dir target/deadlib-image-reference/native-coverage-fresh
```

The output directory must be new. Raw profiles, full LLVM JSON and annotated
source stay in ignored build storage. The committed zero-count annotation list
supports source review; it is not a replacement for LLVM's full region mapping
or its 133 missed-line total. Some missed spans have no explicit zero-count
annotation in LLVM's text output.

The reviewed gaps call for targeted work:

- APNG: control chunk lengths, duplicate/order/frame-count failures, inconsistent
  supplied frame storage, output limits and nonzero timing denominators.
- PNG framing: IHDR/chunk length boundaries, zero dimensions, PLTE/tRNS duplicate
  order, encoder dimensions/chunk sizes and incompatible supplied transparency.
- Streams: IO failures and remaining buffered/fragmented parser error branches.
- Public API: invalid native storage variants, limits and error formatting.

Compression branches are substantially exercised (buffered zlib 96.27%,
streaming inflate 93.55%); APNG remains 67.92%. More generated grammar tests,
external rendered-frame comparisons and native fuzz coverage analysis remain
open. Increasing a percentage alone is not a reason to change codec behavior.
