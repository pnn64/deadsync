# Encoder correctness completion (2026-10-28)

PNG/APNG encoding is complete for both `encode` and `encode_writer`, including
native/reference sample precision, metadata and animation. The final audit found
no missing required encoder feature or known correctness defect. No production
fix was necessary. Milestone `337da6b64` adds the encoder-specific regression
and independent-encoder checks; this completion retains the full validation.

The current user goal covers completed PNG encoding, passing tests and accuracy
against other encoders. Performance and compression ratio are not completion
gates. This audit includes both native writers, all PNG formats, filters and
Adam7, native/reference sample precision, metadata, APNG, compression correctness,
input rejection and writer IO behavior. Compressed byte identity with another
encoder is not expected; exact stored-image semantics and conforming output are.

## Verified encoder field and reference milestone

The new encoder-specific targets provide evidence beyond the earlier decoder
audit. No production change has been needed.

| Evidence | Verified behavior |
| --- | --- |
| `tests/encoder_fields.rs` | Both writers match 109,376 independently constructed literal PNG files, reject 82,396 invalid scalar inputs, and match an independent Table 7 model for 49,419 metadata-order pairs. Another 308,033 invalid native palette/transparency/chunk-size inputs fail before writing output. The 15,576 skipped malformed wire lengths cannot be represented by the native API's whole palette entries and fixed-width gray/RGB transparency words; representable invalid lengths remain tested |
| `tests/encoder_boundaries.rs` | Both writers reject 687 malformed native image/animation/frame inputs. A 450-valid/2,430-invalid APNG rectangle matrix covers all formats and both default-image roles; valid plain/Adam7 files have independent CRC, raw control and flate2 frame-stream answers. Every output-byte failure offset is injected across eight writer configurations: 2,616 prefix checks, short/Interrupted writes and invalid returned byte counts |
| `tests/encoder_reference.rs` | 2,160 independent Rust png animations and 180 static encodings agree with original samples, palettes, transparency and controls. Both native writers produce 8,640 corresponding animations, including all formats/filters, plain/Adam7, both compression modes, all disposal/blend modes and both default-image roles |
| `tests/oracle/encode.c` | A separately compiled C libpng encoder consumes packed native samples, palette and transparency bytes without PNG input or transforms. Its 1,620 outputs cover all fifteen formats, three geometries, six filters, plain/Adam7 and zlib levels 0/1/9. Native and Rust png decoders check exact source semantics; another 3,240 native outputs use the same source cases |

The [milestone record](encoder-milestone-validation.json), [focused test log](encoder-milestone.log)
and [C encoder log](encoder-reference-c.log) retain the source identities and
results. Strict all-target Clippy passes. The new reference adapter accounts for
Rust png's one-byte-per-component transparency-key API at depths up to eight,
and uses a zero-initialized Adam7 scatter buffer and the canvas stride for
interlaced APNG subframes. Initial adapter failures did not require changes to
production image data or codec logic.

## Full verification and completion audit

The [completion record](encoder-completion-validation.json) retains commands,
source identities, tool versions, reference binaries and log hashes. Every
process has completed successfully:

| Gate | Current authoritative result |
| --- | --- |
| [Complete release suite](encoder-completion-release.log) | 145 passed, zero failed; 21 optional checks explicitly skipped here and executed separately |
| [All optional reference checks](encoder-completion-oracles.log) | 21 passed, zero failed or ignored, including the C encoder, C native/metadata getters and independent APNG golden regeneration |
| [Standalone offline debug suite](encoder-completion-independent.log) | 145 passed outside the application workspace; all 892 copied package files and the shared source match the current worktree; normal dependency tree contains only `deadlib-image` |
| [Clippy](encoder-completion-clippy.log) / [rustfmt](encoder-completion-fmt.log) | All-target Clippy with warnings denied and scoped formatting pass |
| [Fresh sanitizer campaigns](fuzz/reports/encoder-source.json) | 30,000 PNG/APNG writer and 20,000 raw buffered/streaming DEFLATE executions; zero findings; all 57 source identities match current files |
| Native-feedback instrumentation audit | Both native archives contain feedback; fourteen dependency archives and four harness objects contain no feedback counters or PC tables. Both fuzz binaries define the ASan runtime |
| Encoder requirements | The three conditions in PNG Third Edition 15.3.2 and their datastream prerequisites are mapped in [CONFORMANCE.md](CONFORMANCE.md) |

The existing suites also verify 3,240 static format/filter/geometry combinations,
all fifteen native formats with ordinary and Adam7 layout, 161 valid PNGSuite
files, reference sample depth declarations and significant-bit recovery, native
16-bit precision, checksum/framing failures and private-chunk copy rules. The
explicit C run includes 1,061 native-sample files, 480 ICC/Exif files, 576 exact
reference-depth files and 576 sBIT recoveries, and the HDR/text/palette/scalar
audits. It checks 24 original Kodak images and eight rewrites each, and
regenerates all 35 WPT and 240 precision animation answers. Independent known
answers remain authoritative where a reference has a documented recovery policy
or hIST ordering/drop difference.

Both raw compression APIs agree with flate2; the new raw campaign includes
empty/short inputs, 32 KiB history and 32/64/128 KiB boundaries, overlapping
matches, mixed/noisy/repetitive blocks, one-byte/empty output and separate final
input. The PNG/APNG campaign varies every native format, both compression modes,
all filters, Adam7, frame rectangles, disposal/blending, transparency, metadata
and data chunk splits. Seed construction calls no production encoder. Scripts,
seed hashes, logs, binary/source identities and compiler/object audits are
retained in `experiments/encoder-fuzz-*.py` and `fuzz/reports/encoder-*`.

No required correctness gate remains. These are engineering tests and bounded
sanitizer campaigns, not a formal proof over every input. Encoding preserves
supplied stored samples and metadata; color management, quantization, profile /
Exif payload interpretation and animation scheduling remain caller operations.
Stored and fixed-Huffman compression are legal output choices; compressed bytes
and file sizes need not match another encoder. Performance was not a completion
requirement and no performance change is claimed.

## Reproduce

Use the pinned libpng/zlib checkouts described in [the oracle README](tests/oracle/README.md).
From the workspace root in PowerShell:

```powershell
$referenceRoot = (Resolve-Path target/deadlib-image-reference).Path
cmake -S crates/deadlib-image/png/tests/oracle -B target/png-encoder-reference "-DREFERENCE_ROOT=$referenceRoot" -A x64
cmake --build target/png-encoder-reference --config Release --target deadlib_png_encoder deadlib_png_oracle --parallel 4
$env:DEADLIB_LIBPNG_ENCODER = (Resolve-Path target/png-encoder-reference/Release/deadlib_png_encoder.exe).Path
$env:DEADLIB_LIBPNG_ORACLE = (Resolve-Path target/png-encoder-reference/Release/deadlib_png_oracle.exe).Path
$env:DEADLIB_PNG_CORPUS = (Resolve-Path target/deadlib-image-reference/kodak).Path
cargo test -p deadlib-image -p deadlib-image-png --release -- --include-ignored --nocapture
cargo clippy -p deadlib-image -p deadlib-image-png --all-targets -- -D warnings
cargo fmt -p deadlib-image -p deadlib-image-png -- --check
./crates/deadlib-image/png/scripts/verify_independent.ps1 -LogPath target/png-encoder-independent-fresh.log
```

The test-only C encoder's bounded version-1 protocol is described in its source.
It reads packed native rows directly; it does not reencode a native-produced PNG
or borrow the production encoder. Rust png supplies the independent animation
encoder; the unpatched C reference is used for static images and frame samples.
The completed decoder audit remains documented separately in
[DECODER-VALIDATION.md](DECODER-VALIDATION.md).
