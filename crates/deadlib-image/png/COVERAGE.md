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
| APNG frame decoding, compositing, encoding | `apng.rs`, `composite.rs`, APNG/compositing tests | 60 format/default-role/interlace combinations, independent encodings and 20-frame fixture pass; broader external APNG corpus pending |
| Bounded streaming with arbitrary input boundaries | Native `zlib::Inflater` and fragmented-output tests | Incremental inflation verified; PNG row/chunk decoder and streaming encoder pending |
| Differential tests: Rust png, libpng, PNGSuite | Rust png 0.18.1, libpng 1.6.50 + zlib 1.3.1, vendored PNGSuite | 611 external libpng native-sample checks and normal Rust png / PNGSuite checks pass |
| Independent fixtures / malformed streams / properties | Independent stored-stream known answer, malformed fixtures, mutation campaign | Partial; larger campaigns pending |
| Fuzz targets and reproducible campaigns | Isolated `fuzz/`, seed generator, checked-in reports | 10,000 PNG and 20,000 zlib AddressSanitizer smoke runs pass; long campaigns and coverage audit pending |
| Benchmarks and profile-guided optimization | `benches/pipeline.rs`, `BENCHMARKS.md`, checked-in TSV | Baseline and first scalar bulk-copy improvement measured; competitive performance still pending |

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

Milestone 3b validation: APNG supports both default-image roles, native-precision
frame rectangles, contiguous sequence numbers, frame counts, plays and rational
delays, all disposal/blend operations, and encoding using IDAT/fdAT split at even
one-byte compressed boundaries. The first frame reuses the default sample
allocation when applicable. An explicit RGBA16 compositor retains one canvas and
saves only the affected PREVIOUS region. Tests independently verify native frame
samples with Rust png, including its canvas-stride output layout for interlaced
subframes, and a licensed 20-frame APNG fixture. Known-answer blending/disposal
tests cover integer rounding, reset, separate default images, first PREVIOUS,
transparency, palette expansion and sub-byte precision. Malformed controls,
sequence numbers, rectangles, methods, frame counts and aggregate output limits
are rejected.

The C libpng oracle was built with MSVC 19.44.35229 on Windows x86-64 and passed
`cargo test -p deadlib-image-png --release --test libpng -- --ignored` on
2026-10-26. It compares every sample without transforming channel layout or
precision: 161 valid PNGSuite images plus 450 new encodings (15 legal formats,
three geometries, five filters, both interlace modes). Setup and pinned source
commits are in `tests/oracle/README.md`. This external test is explicitly ignored
by ordinary Cargo runs because it needs a separately built C executable.

Current limitations: decoding buffers complete compressed and filtered images;
incremental bounded streaming, broader malformed/fuzz campaigns, reproducible
benchmarks and measured optimization remain required. ICC/Exif interpretation,
color management, HDR tone mapping, and animation scheduling are caller concerns.
The compositor works in stored sample space; no color-management parity is claimed.

First optimization validation: release tests, Clippy with warnings denied, the
3,240-case format matrix, APNG frame matrix, and 611 libpng checks pass with bulk
DEFLATE copies, contiguous row copies and slicing-by-eight CRC. Additional
independent bitstream fixtures cover 400 fixed/stored/fixed block boundaries and
65 dynamic literal-only/single-symbol/empty-distance streams. CRC bulk/tail and
split-boundary checks use an independent bitwise reference. Decode throughput
improved locally by about 1.8–2.3x; see the benchmark document for limits.

PNG editor behavior is explicit: decoded unknown chunks remain accessible,
while reencoding omits unrecognized chunks whose safe-to-copy bit is unset.

Fuzz smoke validation (2026-10-26): isolated libFuzzer targets type-check on
Windows. The instrumented MSVC link failed with missing sanitizer coverage
symbols, so campaigns ran on Debian WSL with Linux nightly, cargo-fuzz 0.13.2,
default AddressSanitizer, fixed seed 782347, 64 KiB input cap, 1 MiB native output
cap, 5-second per-input timeout and 1024 MiB RSS cap. PNG: 10,000 runs, no failure,
170 MiB peak RSS; zlib: 20,000 runs, no failure, 360 MiB peak RSS. Zlib compares
native-accepted output against flate2; PNG compares native samples on inputs also
accepted by Rust png. These are smoke checks, not exhaustive fuzz evidence.
The seed generator and full logs are in the crate. Longer structured campaigns,
CRC-repaired chunk mutations and APNG frame differential fuzzing remain required.

## Remaining completion gates

Streaming milestone 4a: `zlib::Inflater` keeps a single 32 KiB ring, two flat
Huffman tables and bounded grammar state. Caller-supplied input/output fragments
can be one byte or empty; partial fields, repeats, literals, matches and checksum
bytes resume without replay. Tests cross six input sizes and six output sizes
for independent stored/fixed/dynamic encodings, stress wraparound and 32 KiB
distances, and run the 400 mixed-block / 65 special-tree fixtures with one-byte
input and output. Prefix consumption leaves trailing bytes untouched; errors are
terminal, output limits are cumulative and checksum verification is mandatory.
Decoder-owned memory stays below 40 KiB across every tested fragment.

The requested end state is not yet achieved. Continue without application
integration, preserving unrelated work and committing verified milestones:

1. Implement genuinely incremental, bounded zlib/PNG decoding and encoding,
   including arbitrary read/write fragmentation, frame/row boundaries, and
   resource/error behavior without replaying complete input prefixes.
2. Extend independent conformance evidence to the full Third Edition clause
   matrix, including ancillary semantic edge cases, BCP47 language-tag grammar,
   all Huffman tree corner cases, malformed APNG ordering, and unknown chunks.
3. Add shrinking/generated image properties, structured CRC-repaired mutations,
   APNG differential fuzzing, longer sanitizer campaigns and coverage review.
4. Improve compression ratio/match selection and profile filters/inflation on a
   representative external corpus. Add controlled repetitions, allocation and
   peak-memory measurements, streaming benchmarks, and comparable libpng timings.
   Select SIMD only after a measured bottleneck justifies it. Current decode and
   compression results do not meet the competitive-performance objective.
5. Audit every original requirement against current code and reproducible
   evidence; publish the final coverage/performance/limitations documentation.
   Keep the goal active until every gate is proved, rather than treating the
   present feature list or green tests as full conformance.
