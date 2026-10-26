# PNG Third Edition implementation and verification

This is an incremental implementation, not yet a claim of complete conformance.
The target remains complete independent PNG support and measured competitive
performance. Milestones are committed only after their relevant checks pass.

| Requirement | Implementation / authoritative evidence | State |
| --- | --- | --- |
| Separate packages, no application integration | Workspace member list; `cargo tree -p deadlib-image-png --edges normal` contains only `deadlib-image` | Verified |
| Native samples, all legal formats | Shared `Image`, `Samples`, `ColorType` | Primitives implemented |
| CRC-32 / Adler-32 | Known-answer tests | Verified known answers |
| Native zlib / DEFLATE decode | Stored, fixed, dynamic trees, history and output limits; `tests/compression.rs` | Bidirectional flate2 differential tests pass; exhaustive tree fixtures still pending |
| Native compression | Stored and fixed Huffman LZ77 encoder; `tests/compression.rs` | Bidirectional flate2 differential tests pass |
| PNG chunk framing and normative ordering | `codec.rs`, `tests/malformed.rs` | Critical chunks, PLTE, tRNS verified; full ancillary validation pending |
| All five color types and all legal bit depths | `tests/static_png.rs`, `tests/pngsuite.rs` | 3,240 native/reference matrix cases and 161 valid PNGSuite files pass |
| Five filters and Adam7 decode / encode | Same format matrix, PNGSuite pass geometry cases | Verified exact native samples |
| Metadata, including Third Edition color chunks | `metadata.rs`, `tests/metadata.rs`, PNGSuite | Typed wire roundtrips and reference text checks pass; language registry and embedded ICC/Exif semantics left to applications |
| APNG frame decoding, compositing, encoding | Pending | Pending |
| Bounded streaming with arbitrary input boundaries | Pending | Pending |
| Differential tests: Rust png, libpng, PNGSuite | Rust png 0.18.1 and vendored PNGSuite | Rust png / PNGSuite pass; libpng pending |
| Independent fixtures / malformed streams / properties | Independent stored-stream known answer, malformed fixtures, mutation campaign | Partial; larger campaigns pending |
| Fuzz targets and reproducible campaigns | Pending | Pending |
| Benchmarks and profile-guided optimization | Pending | Pending |

Milestones: (1) shared primitives and native compression; (2) static PNG codec and
sample matrix; (3) metadata and APNG; (4) streaming and external conformance;
(5) fuzzing, benchmarks, measured optimization, and final requirement audit.

Known limitations are recorded here as development proceeds; an implemented
feature without corresponding independent evidence is not marked verified.

Milestone 1 validation (2026-10-26, Rust 1.99.1, Windows x86-64):
`cargo test -p deadlib-image -p deadlib-image-png` passes one checksum test and
three integration tests. Differential cases cover empty, short, random,
repetitive, >32 KiB history, and >64 KiB stored-block boundaries, with flate2
levels 0/1/6/9, both native compression modes, every truncation of two streams,
checksum and limit failures, and single-bit mutations of a dynamic stream.

Milestone 2 validation: native static PNG decode and encode cover all 15 legal
color/depth combinations, five filters plus adaptive selection, both compression
modes, Adam7 and plain scanlines, nine dimensions including empty Adam7 passes,
and seven-byte IDAT splits (3,240 combinations). The independent Rust png decoder
checks every encoded case; 90 independent reference encodings check native
decoding. One-byte IDAT splits, every truncation of a small file, malformed
critical/chunk/sample/filter cases, CRC failures, resource limits, and the
vendored PNGSuite (161 valid / 14 malformed files) also pass. All 161 valid
PNGSuite images reencode with both interlace settings. No implicit gamma,
precision, palette, alpha, or color-space transformations are performed.

Milestone 3a validation: all metadata chunk types in PNG Third Edition have typed
representations, exact fixed-point integer serialization and parsing. Raw chunks
remain available for lossless retention. Tests cover compressed/uncompressed
Latin-1 and international text (including Japanese and emoji), ICC streams,
palette metadata, Exif retention, all HDR/color chunks and precedence, singleton
and placement errors, invalid methods/values, and cumulative expansion limits.
Rust png independently checks decoded text. `cargo clippy -p deadlib-image
-p deadlib-image-png --all-targets -- -D warnings` passes.

Current limitations: APNG chunks are retained but not yet
interpreted; decoding currently buffers the complete compressed and filtered
image; no performance parity claim has been made.
