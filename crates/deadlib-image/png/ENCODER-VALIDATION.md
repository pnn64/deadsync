# Encoder correctness audit (2026-10-28)

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

## Checks still running before completion

- Full release suite, including existing compression, sample-precision, metadata,
  malformed-input, PNGSuite, APNG and decoder regression tests.
- All optional independent reference checks with fresh C encoder/oracle binaries.
- Complete offline debug suite in a fresh standalone workspace.
- Fresh native-feedback ASan campaigns for the PNG/APNG writer and buffered /
  streaming DEFLATE encoders, with source/binary identities and object audits.
- Final encoder-clause traceability, retained evidence and completion audit.

This milestone alone does not close the encoding goal. The completed decoder
audit remains documented separately in [DECODER-VALIDATION.md](DECODER-VALIDATION.md).
