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
| PNG chunk framing and normative ordering | Pending | Pending |
| All five color types and all legal bit depths | Pending | Pending |
| Five filters and Adam7 decode / encode | Pending | Pending |
| Metadata, including Third Edition color chunks | Pending | Pending |
| APNG frame decoding, compositing, encoding | Pending | Pending |
| Bounded streaming with arbitrary input boundaries | Pending | Pending |
| Differential tests: Rust png, libpng, PNGSuite | Pending | Pending |
| Independent fixtures / malformed streams / properties | Pending | Pending |
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
CRC is temporarily unused outside tests until PNG framing is added.
