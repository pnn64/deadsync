# Decoder correctness completion (2026-10-28)

PNG/APNG decoding is complete for this package's native-sample API. The final
review found no missing required decoder feature or known correctness defect.
No production change was needed: the current decoder passes the full regression
suite and independent reference checks. This milestone retains the verification
and updates the completion status.

The user's current goal is finished decoding, passing tests and accuracy against
other decoders. Performance was explicitly excluded on 2026-10-28. The existing
scope includes all native PNG formats, streaming, metadata, APNG frame decoding
and composition, resource/error handling and independent package reuse. Encoder
completion and application integration are outside that scope.

## Requirements and evidence

| Requirement | Authoritative evidence | Result |
| --- | --- | --- |
| All standardized PNG pixel formats, filters and Adam7 | `static_png`, `pngsuite`, `scalar_fields`, `reference_samples`; independent Rust png and C libpng native samples | All 15 legal formats retain their exact channel layout, palette indices and 1/2/4/8/16-bit samples; 3,240 format/filter/geometry cases and 161 valid PNGSuite files pass |
| Native zlib/DEFLATE correctness | `compression`, `zlib_headers`, `huffman`, `huffman_permutations`, `primary_pairs`, `stream_literals`, `deflate_prefix`, `inflate_stream`; flate2 and separately built C zlib | Stored/fixed/dynamic blocks, code trees, overlap/history, checksums, suffix consumption, truncations and exact limits pass; selected complete-tree shapes cover every legal symbol/depth cell |
| Chunk grammar, metadata and standards behavior | `chunks`, `ancillary_order`, `decoder_boundaries`, scalar/text/profile/HDR/language suites; [clause audit](CONFORMANCE.md) | All twelve PNG Third Edition decoder conditions have implementation and direct regression evidence; field/name/property/order/budget classes pass |
| Buffered, reader and push decoding | Fragmented/empty input and output, injected reader failures, row coordinates, terminal errors and native sample comparisons in streaming/boundary suites | APIs agree on samples, controls, metadata and completion/error boundaries |
| APNG native frames and displayed canvases | Rust png control/frame comparisons; 35 WPT and 240 generated animations; independently regenerated C libpng/Python Fraction RGBA16 answers | Both default-image roles, plain/Adam7 subframes, all blend/disposal modes, partial rectangles, reset/replay and exact native precision pass; 2,121 independent displayed canvases agree |
| Invalid input and recovery | Independent malformed known answers, PNGSuite negatives, strict/recovery animation policies, indexed black recovery, budgets and injected IO failures | Expected fatal errors/recovery diagnostics and exact resource boundaries pass; reference permissiveness is not used to accept invalid core data |
| All tests, including optional references | Fresh release suite and explicit `--ignored` run, standalone offline debug suite, strict-window and extended-count C variants | All pass; see the retained logs and totals below |
| Independent reuse and dependency boundary | Fresh temporary workspace copied by `scripts/verify_independent.ps1`; offline tests and normal dependency tree | Only `deadlib-image` is a production dependency; no DeadSync loader integration |
| Sanitizer evidence applies to the current sources | All 57 files in `fuzz/reports/huffman-permutations-source.json` rehashed against the worktree | Exact match, including production sources and exercised harness/helpers; the retained ASan PNG/raw campaigns each completed 100,000 executions without a finding |
| Remaining coverage gaps reviewed | `coverage-reports/README.md`, `boundaries-decoder-gaps.json`, current reader/row/inflate code and subsequent field/control/Huffman suites | The twelve focused annotations are a brace, parser-established invariants, defensive guards or a cumulative counter overflow beyond the bounded per-input campaign; no missing decode operation is indicated |

The last row is a reachability review, not a claim of 100% source coverage. The
whole-package reports include unused encoding utilities and must not be described
as decoder-only percentages. Systematic matrices and bounded fuzzing supply
engineering evidence, not enumeration of every possible PNG or a formal proof.

## Fresh validation

The machine-readable [validation record](decoder-completion-validation.json)
contains commands, tool versions, source/input identities, reference binary
identities and log hashes. Full output is retained in:

- [Release tests](decoder-completion-release.log)
- [All optional reference tests](decoder-completion-oracles.log)
- [Standalone offline debug tests and dependency tree](decoder-completion-independent.log)
- [Strict-window C zlib](decoder-completion-strict.log)
- [Extended-count C zlib](decoder-completion-pkzip.log)
- [Clippy](decoder-completion-clippy.log) and [format check](decoder-completion-fmt.log)

The ordinary release suite passes 138 tests and explicitly skips twenty optional
checks. The separate reference run executes all twenty, with zero failures and
zero ignored tests. The standalone debug suite passes the same 138 regular tests
outside the application workspace. The two C configuration checks also pass.
Clippy passes for all targets with warnings denied; scoped rustfmt passes.

The C oracle is freshly built from the checked-in helper and pinned libpng
1.6.50 / zlib 1.3.1 source checkouts, not linked into the Rust decoder. The
reference run includes 1,061 native-sample files, 24 Kodak originals plus eight
rewrites each (216 comparisons), 480 ICC/Exif files, the field/order/text audits,
136,352 exact Rust png APNG control/sample comparisons, and regeneration of all
275 independent animation golden files. The broad matrices use known answers in
addition to references; they do not merely roundtrip through our own encoder.

After building the helper as described in [the oracle README](tests/oracle/README.md),
the main checks can be reproduced from the workspace root in PowerShell:

```powershell
$env:DEADLIB_LIBPNG_ORACLE = (Resolve-Path target/png-completion-oracle/Release/deadlib_png_oracle.exe).Path
$env:DEADLIB_PNG_CORPUS = (Resolve-Path target/deadlib-image-reference/kodak).Path
cargo test -p deadlib-image -p deadlib-image-png --release -- --include-ignored --nocapture
cargo clippy -p deadlib-image -p deadlib-image-png --all-targets -- -D warnings
cargo fmt -p deadlib-image -p deadlib-image-png -- --check
./crates/deadlib-image/png/scripts/verify_independent.ps1 -LogPath target/png-independent-fresh.log
```

The optional C strict-window and PKZIP compatibility configurations are described
in the oracle README and recorded separately. Their environment flags must only
be set when using the corresponding reference binary.

## Accuracy and API boundaries

Exact comparisons concern stored native samples, frame controls, supported
metadata values and RGBA16 composition in stored sample space. ICC/gamma/HDR
transforms, display scaling and animation scheduling remain caller operations.
These boundaries were already part of the API; they do not hide missing native
decode functionality. Rows emitted before `ImageEnd` are provisional; `Done`
establishes complete push-parser validation. Whole-file APIs reject trailing
bytes after IEND, while the push API preserves them for its caller.

Malformed-input acceptance need not match a reference's recovery policy. Tests
explicitly retain known differences: reserved-bit ancillary names, invalid core
ordering, pre-output or over-advertised-window distances, extended distance
alphabet definitions, and libpng's hIST ordering/drop behavior. Supported valid
sample comparisons remain exact; these differences are not concealed mismatches.

The PNG Third Edition and published errata were rechecked for this audit. The
errata still contain only the non-conformance-affecting mDCV example correction.
No unimplemented decoder clause was identified. Historical performance results,
earlier open-work statements and rejected optimization trials remain available
as development history; they are superseded by this correctness completion audit.
