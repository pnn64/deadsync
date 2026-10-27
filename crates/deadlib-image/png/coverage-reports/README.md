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

The `inflate-*` snapshot covers the selected wider-refill/canonical-prefix
decoder. The corrected script runs all 22 test executables and reporting stages
in one pass. It maps the same 20 integration executables, reporting 96.61% lines,
92.35% regions and 83.80% instrumented branch outcomes (3,793/3,926 lines;
1,081/1,290 branch outcomes). Its manifest also records the instrumentation
environment and runner hash. There are still 133 missed lines; the earlier
gap categories remain applicable. This snapshot is separate from the selected
code's four 5,000-execution sanitizer campaigns.

## Native compositor fuzz-corpus replay (2026-10-27)

`compositor-*` maps only `src/composite.rs` while replaying the final 1,040-file /
112,802-byte corpus from the new target's 20,000-run ASan campaign. Cargo-fuzz's
coverage mode builds a separate instrumented binary without a sanitizer, then
executes a fixed corpus. Replay does not add to fuzz campaign totals. The
instrumented binary has no Rust png, flate2 or libdeflate codec symbols; the
target calls only native code and its u128 rational test model.

Explicit LLVM source operands exclude the model, helper fixtures, stdlib and
reference libraries from reported counts. The result is 177/182 lines (97.25%),
231/245 regions (94.29%) and 41/46 instrumented branch outcomes (89.13%). This is
scoped source coverage, not whole-decoder coverage or proof of conformance.
LLVM's branch mapping and denominator are toolchain-specific.

The four explicit zero-count annotations identify invalid supplied sample
storage, transparency incompatible with its color type, absent palette
transparency and a compositor call without animation. There are five missed
lines overall; annotations are not a replacement for the complete mapping.
These gaps guide future corpus/API checks. All three disposal operations,
both blends, partial rectangles, snapshot-limit failures, palette black
recovery and reset execute in the corpus.

The manifest retains native/model source hashes, coverage binary and profile
hashes, final corpus digest, environment and all reporting commands. Raw profiles
and annotated source remain in `fuzz/coverage/compositor` and the retained
`/tmp/deadlib-image-compositor-coverage-20261027-a` output directory. To reproduce,
run `cargo fuzz coverage compositor CORPUS --sanitizer none --jobs 4` on the
selected nightly with `RUSTFLAGS='-Z coverage-options=branch'`, then use that
toolchain's `llvm-cov` with the resulting binary/profile and the single explicit
`src/composite.rs` source operand, as recorded in the manifest.
