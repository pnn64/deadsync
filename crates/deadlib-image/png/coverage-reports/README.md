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

## Native-crate feedback and decoder corpus replay (2026-10-27)

`native-feedback-*` maps the final corpora from 50,000 PNG-stream and 20,000
raw-stream ASan executions. A compiler wrapper restricted mutation feedback to
the two native image crates. Object-section audits independently found no
feedback counters or PC tables in fourteen dependency archives or four harness
objects. These checks separate mutation feedback from the source-coverage report:
the former can include std/alloc monomorphizations compiled inside native crates;
the latter selects only native source filenames and excludes stdlib source.

The fixed replay uses 28,770 PNG files and 1,096 raw-stream files. It maps twenty
native PNG source files plus shared primitives. The generated language registry
is constant data and has no runtime mapping. Package totals include uncalled
encoding utilities, which are outside decoder completion requirements. They
must not be presented as decoder-only coverage. The ordinary compiled library
is mapped; embedded cfg(test) functions are absent.

| Decoder-only file | Line coverage | Instrumented branch outcomes |
| --- | ---: | ---: |
| `stream/decode.rs` | 541/555 (97.48%) | 209/228 (91.67%) |
| `stream/read.rs` | 112/120 (93.33%) | 14/20 (70.00%) |
| `stream/rows.rs` | 169/171 (98.83%) | 47/50 (94.00%) |
| `zlib/inflate.rs` | 254/265 (95.85%) | 59/62 (95.16%) |

`native-feedback-decoder-gaps.json` lists 35 explicit zero-count annotations in
these four files, with per-file summaries. They identify resource-limit paths,
accessors/error repetition, interrupted or invalid reader results, internal
invariants and byte draining after Huffman lookahead before a stored block.
The next corpus/API work should vary resource budgets, exercise reader failures
and include independent mixed-block prefix fixtures. Internal guards also need
reachability review. Percentages alone are not a reason to change behavior.
The four-file list does not replace the complete mapping or the other decoder
field/metadata/APNG audits.

The manifest retains source, corpus, binary and merged-profile hashes plus
commands and the exact toolchain. Full annotated source and raw profiles remain
under `/tmp/deadlib-image-native-feedback-coverage-20261027-a` and the per-target
`fuzz/coverage` folders. Fixed replay is excluded from sanitizer campaign totals.
The complete replay command/output trace is retained as
`native-feedback-commands.log.gz`, with compressed and original byte hashes in
the manifest; the uncompressed trace remains in the temporary output directory.
The campaign manifest records an initial artifact-directory audit failure and
its recovery against the same retained binaries; no fuzzing was restarted.
Production sources remain identical to `fae2e5bd2`.

## Decoder field and resource corpus union (2026-10-27)

`boundaries-*` replays 6,926 corrected resource-target inputs, 5,524 raw-stream
inputs and the earlier 28,770 PNG-stream inputs. The first two corpora are new;
the broad PNG corpus is reused after verifying unchanged native and harness
source hashes. Only that additional PNG target was replayed when extending the
two-target profile. Replay adds no sanitizer executions. The associated
development and final ASan campaigns contribute 80,000 successful executions;
their exact helper versions and object audits are retained in `../fuzz/reports/`.

All twenty-one PNG production files retain `fae2e5bd2`/`41366d843` bytes. Explicit
source operands map twenty of them plus shared primitives; the constant registry
has no runtime mapping. Package totals include unused encoding utilities and
must not be described as decoder-only coverage.

| Decoder-only file | Line coverage | Instrumented branch outcomes |
| --- | ---: | ---: |
| `stream/decode.rs` | 554/555 (99.82%) | 218/228 (95.61%) |
| `stream/read.rs` | 114/120 (95.00%) | 17/20 (85.00%) |
| `stream/rows.rs` | 170/171 (99.42%) | 48/50 (96.00%) |
| `zlib/inflate.rs` | 261/265 (98.49%) | 60/62 (96.77%) |

`boundaries-decoder-gaps.json` lists twelve explicit zero-count annotations,
down from 35. This is a focused review of four files, not a complete inventory
of every uncovered line, region or branch. The remaining annotations are:

- `stream/decode.rs:293`: a closing brace in LLVM's source display.
- `stream/read.rs:48,62,102,155,158,164`: representation/header/frame invariants,
  a `)?` span, input-progress checking and the final missing-header guard. Parser
  state establishes these invariants before the collecting reader uses them;
  ordinary resource and injected reader errors now execute in the corpus.
- `stream/rows.rs:172`: an extra-data guard after complete row output. The parser
  already caps inflation at the exact filtered size. Retain the defensive check.
- `zlib/inflate.rs:140-141`: cumulative input-counter overflow, outside a fresh
  inflater's bounded per-input fuzz campaign.
- `zlib/inflate.rs:255-256`: draining prefetched stored-block payload. Under the
  current refill logic, a fast lookup holds at most 39 bits after a symbol
  (at most eight already buffered plus 32 loaded minus at least one code bit);
  a canonical fallback holds at most 36 (fourteen plus 32 minus at least ten).
  After EOB, the next three block-header bits and alignment leave at most 32
  bits, all consumed by stored LEN/NLEN. No payload bits remain prefetched at
  this guard. Keep it defensive against future refill changes.

These reachability observations apply to the current implementation. They are
not a reason to fabricate private decoder state or remove guards to increase a
percentage. Further chunk-field, metadata/APNG and performance work remains
independent of these annotations.

The fresh isolated fuzz project, full annotated source, raw profiles and merged
profile remain under `/tmp/deadlib-image-boundaries-coverage-20261027-a`; builds
remain under `/tmp/deadlib-image-boundaries-coverage-target-20261027-a`. The
manifest retains all source/copy/corpus/binary/profile hashes, commands and
toolchain. `boundaries-commands.log.gz` retains the complete replay trace with
both compressed and original hashes. Its three linked repository campaign
manifests distinguish development helper snapshots from corrected source.
