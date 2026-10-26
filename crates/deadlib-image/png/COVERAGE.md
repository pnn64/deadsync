# PNG Third Edition implementation and verification

This is an incremental implementation, not yet a claim of complete conformance.
The target remains complete independent PNG support and measured competitive
performance. Milestones are committed only after their relevant checks pass.
The [clause audit](CONFORMANCE.md) maps the Third Edition to current code,
evidence, application responsibilities and explicitly open items.

| Requirement | Implementation / authoritative evidence | State |
| --- | --- | --- |
| Separate packages, no application integration | Package names, production tree, isolated offline workspace tests (`independent-build.log`) | Verified outside the application workspace; production dependency is only `deadlib-image` |
| Native samples, all legal formats | Shared `Image`, `Samples`, `ColorType` | Primitives implemented |
| CRC-32 / Adler-32 | Known-answer tests | Verified known answers |
| Native zlib / DEFLATE decode | Stored, fixed, dynamic trees, advertised/actual history and output limits; compression / zlib-header suites | All 65,536 headers and 92 window fixtures pass; C zlib default/strict comparisons recorded; broader tree fixtures still pending |
| Native compression | Stored/fixed Huffman LZ77, bounded 16-candidate chains and per-block coding choice; `zlib::Deflater`, `tests/deflate_stream.rs` | Independent flate2 checks, arbitrary fragments, mixed coding and 32 KiB block/history boundaries pass |
| PNG chunk framing and normative ordering | `codec.rs`, `tests/malformed.rs` | Critical chunks, PLTE, tRNS verified; full ancillary validation pending |
| All five color types and all legal bit depths | `tests/static_png.rs`, `tests/pngsuite.rs` | 3,240 native/reference matrix cases and 161 valid PNGSuite files pass |
| Five filters and Adam7 decode / encode | Same format matrix, PNGSuite pass geometry cases | Verified exact native samples |
| Indexed-color error recovery (13.1) | `tests/palette.rs`; all indexed depths, Adam7, APNG, one-byte reads/events, independent Rust png encoding | Native indices retained, opaque-black expansion/rendering; opt-in strict decode and always-strict encoding |
| Metadata, including Third Edition color chunks | `metadata.rs`, metadata / text / field / IANA / profile tests, PNGSuite | Typed wire roundtrips, UTF-8, BCP47 registration, method distinctions, sample masks, histogram usage and profile containers checked; extension-specific and payload semantics still require audit |
| sRGB fallback metadata consistency | `tests/metadata_fields.rs` | Both encoders enforce Table 17 gAMA/cHRM values with sRGB; all 15 formats / four intents and incompatible fallback orders pass |
| cICP code-point validity | `tests/cicp.rs`, H.273 (07/2024) | All 131,072 primaries/transfer/range combinations, all matrix/flag bytes and reserved-code fallback behavior pass |
| APNG frame decoding, compositing, encoding | `apng.rs`, `composite.rs`, APNG/compositing tests | 60 format/default-role/interlace combinations, independent encodings and 20-frame fixture pass; broader external APNG corpus pending |
| Bounded streaming with arbitrary input boundaries | `zlib::{Inflater,Deflater}`, `StreamDecoder`, `decode_reader`, `encode_writer` | PNG/APNG native row pipelines, arbitrary IO fragments, finish/errors and capacity bounds verified |
| Differential tests: Rust png, libpng, PNGSuite | Rust png 0.18.1, libpng 1.6.50 + zlib 1.3.1, vendored PNGSuite | 1,061 native-sample plus 480 exact ICC/Exif libpng comparisons; format matrices and PNGSuite pass |
| Independent fixtures / malformed streams / properties | Independent stored-stream known answer, malformed fixtures, mutation campaign | Partial; larger campaigns pending |
| Fuzz targets and reproducible campaigns | Isolated `fuzz/`, generated-image properties, deterministic seeds and reports | Seven targets; 200,000 recorded sanitizer executions across milestones, including 10,000 final-encoder runs; longer campaigns and native-only coverage audit pending |
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
`cargo test -p deadlib-image-png --release --test libpng libpng_native_sample_conformance -- --ignored` on
2026-10-26. It compares every sample without transforming channel layout or
precision: 161 valid PNGSuite images plus 450 new encodings (15 legal formats,
three geometries, five filters, both interlace modes). Setup and pinned source
commits are in `tests/oracle/README.md`. This external test is explicitly ignored
by ordinary Cargo runs because it needs a separately built C executable.

Current limitations: the original slice `decode` path buffers complete compressed
and filtered images. `StreamDecoder`, `decode_reader` and `encode_writer` avoid
unbounded intermediate image/output buffers; broader malformed/fuzz campaigns,
conformance auditing, reproducible
benchmarks and measured optimization remain required. ICC/Exif interpretation,
color management, HDR tone mapping, and animation scheduling are caller concerns.
The compositor works in stored sample space; no color-management parity is claimed.

First optimization validation: release tests, Clippy with warnings denied, the
3,240-case format matrix, APNG frame matrix, and 611 libpng checks pass with bulk
DEFLATE copies, contiguous row copies and slicing-by-eight CRC. Additional
independent bitstream fixtures cover 400 fixed/stored/fixed block boundaries and
65 dynamic literal-only/single-symbol/empty-distance streams. CRC bulk/tail and
split-boundary checks use an independent bitwise reference. Decode throughput
improved locally by about 1.8â€“2.3x; see the benchmark document for limits.

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
The seed generator and full logs are in the crate. Longer structured campaigns
and coverage review remain required.

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

Streaming milestone 4b: `StreamDecoder` parses chunk headers, CRCs, metadata,
APNG sequence/control fields and native rows without storing IDAT/fdAT payloads
or complete images. It emits plain/Adam7 rows and validated image boundaries.
`decode_reader` collects final native images and frames from arbitrary `Read`
fragments, retries Interrupted, and rejects trailing bytes. Chunk/sample/metadata
validators are shared with buffered APIs. All 3,240 format cases and the 60 APNG
combinations also pass through the new collector. PNGSuite (161 valid / 14 invalid)
and the independent APNG fixture match across 1/2/7/4096-byte reads. Tests cover
every truncation, IO failures, limits, fused errors, prefix consumption and rows
that precede a bad data-chunk CRC. `ImageEnd` is never emitted for an unverified
image. Identical-width 2-row and 4,096-row images report identical pipeline buffer
capacity. Both buffered and streaming collectors pass the 611 libpng cases.

Two additional Linux AddressSanitizer campaigns pass: 20,000 fragmented zlib
inputs (389 MiB peak RSS) and 20,000 PNG inputs with repaired CRCs (396 MiB).
The latter compares buffered acceptance and every native default/APNG-frame row
sample; the existing buffered differential campaigns supply independent codec
comparisons. Full logs and deterministic seed generation are checked in.
The local streaming benchmark reports 248â€“413 MiB/s for collected images and
288â€“519 MiB/s for row consumption, with approximately 40â€“49 KiB of image-pipeline
storage for the 512-pixel RGBA fixtures. See `BENCHMARKS.md` for measurement limits.

Streaming milestone 4c: `zlib::Deflater` preserves bit state, a single 32 KiB input
block and cross-block match history, with fixed memory across arbitrary input /
output fragmentation. Empty streams, exact block ends, separately signaled EOF,
zero-capacity output, fused misuse errors and long wraparound histories pass
native and independent flate2 checks. `encode_writer` generates filtered native
plain/Adam7 rows, writes IDAT/fdAT chunks directly and returns measured byte /
buffer statistics. It handles Interrupted, partial and zero/error writes, honors
smaller chunk limits, and caps compressed chunk payloads at 64 KiB. Image-pipeline
capacity is independent of height. The caller retains the input images; no
entire filtered/compressed/output PNG is buffered internally.

Both encoding APIs pass all 3,240 format combinations against Rust png. The
stream writer also passes both compression modes across the 60 APNG combinations,
metadata checks, 540 fragmented-writer cases and both interlace encodings of all
161 valid PNGSuite files. Libpng now checks 1,061 files: the original 611 plus
450 stream-writer encodings. The generated-image/APNG property target passes
20,000 AddressSanitizer executions (409 MiB peak RSS), comparing both native
decoders and independent Rust png samples. An initial harness-only placement
assumption was corrected and preserved with a replay log and regression test:
without PLTE, BeforePalette and BeforeData map to the same wire position.

Independent reuse validation: `scripts/verify_independent.ps1` copies only the
shared primitives and PNG package into a new temporary workspace, then runs
offline debug tests and the normal production tree. All tests pass there, with
the C oracle explicitly ignored and separately verified above. The source copy
is retained; `independent-build.log` records the run. The original application
workspace and image loaders are not used by this isolated build.

Conformance recovery milestone: PNG Third Edition 13.1 opaque-black recovery is
implemented by explicit RGBA16 expansion and APNG rendering. Buffered and streamed
decoding retain original indices and palette entries; `PalettePolicy::Strict`
is available for rejection instead. Both encoders reject invalid indices before
writing output. Tests cover all four indexed depths, plain/Adam7, both default
image roles, SOURCE/OVER, indices only in fdAT, every index in each depth's
sample range, one-byte chunk/IO boundaries, borrowed row events and an independent
Rust png fixture. The malformed-input suite still rejects illegal palette
structure and uses explicit strict policy for index errors.

Text/field conformance milestone: typed iTXt follows [WHATWG UTF-8 decode](https://encoding.spec.whatwg.org/#utf-8-decode),
as recommended by [PNG 13.7](https://www.w3.org/TR/png-3/#13Text-chunk-processing).
It consumes exactly one leading BOM, retains interior/additional BOMs and uses
replacement characters for malformed maximal subparts. Thirteen known-answer
inputs cover truncated sequences, overlong encodings, surrogates and the Unicode
maximum, in compressed and uncompressed text and translated keywords. Raw chunks
remain unchanged. Both encoders reject malformed UTF-8 and nonzero uncompressed
methods; decoders ignore that method as required (all 256 values tested).
Explicit typed reserialization can replace malformed text before reencoding.

[RFC 5646](https://www.rfc-editor.org/rfc/rfc5646.html) language-tag grammar,
case-insensitive variant/singleton uniqueness, grandfathered tags and private-use
sequences are checked. Tests cover all 26 grandfathered tags, PNG and RFC examples,
malformed subtags/order/duplicates and long private tags. This checks syntax;
IANA registration and extlang prefixes are validated by the following milestone.
Extension-specific semantics remain under audit. The implementation does not
perform network requests while processing images.

Decoders mask unused tRNS/bKGD bits before use while retaining original wire
values. Both encoders zero those unused bits; tests cover every legal nonindexed
format and verify that 16-bit transparency compares both bytes before scaling.
Encoders reject zero hIST frequencies for used indexed colors, including frame
samples, while allowing zero frequencies for unused entries. Typed metadata
access validates the public Image format before bit-depth-dependent operations.
The official Third Edition errata corrects an mDCV example without changing
conformance.

The conformance follow-up passes a further 20,000 Linux AddressSanitizer runs of
the CRC-repaired PNG row target with both palette policies. Its fresh 176-file
initial corpus consists only of the checked-in PNGSuite and independent APNG;
peak RSS was 396 MiB. The report records the full command, seed and diagnostics.
Full release tests, strict Clippy, the 1,061-file C libpng oracle and isolated
offline builds pass with these field changes.

IANA registration milestone: encoders and typed iTXt serialization check primary
languages, scripts, regions, variants, extlang registrations and their prefixes,
and registered extension singletons against the bundled 2026-09-17 registry
(extensions registry 2014-04-02). Reserved private ranges and deprecated records
remain accepted; private-use sequences require no registration. The tables are
flat, sorted, read-only byte keys with binary search and no runtime IO or new
dependencies. `LANGUAGE_SUBTAG_REGISTRY_DATE` identifies the encoder snapshot.
Well-formed tags outside that snapshot are retained as unknown raw ancillary
metadata by decoders (PNG 13.2), so future registries do not make images unreadable.
Malformed language grammar still reports an error. Reencoding unknown tags
requires a registry update or an explicitly chosen registered tag.

The pinned CC0 IANA source snapshots and SHA-256 hashes are checked in.
`scripts/update_language_registry.py --check` verifies reproducible offline
generation; `--refresh` explicitly updates source data. An independent test
parser checks all 9,902 records/expanded private codes: 8,795 languages, 258
extlangs, 274 scripts, 343 regions, 139 variants, 26 grandfathered and 67 redundant
tags, in original and upper case. Every extlang also rejects a wrong prefix.
Tests exercise all private-range endpoints, deprecated codes, unregistered keys,
extension singletons, encoder rejection before writing, and forward-compatible
buffered/reader/row-event retention. Unicode extension payload interpretation is
not claimed by these registration checks.
Full release tests (52 top-level tests), strict Clippy, the external 1,061-file
libpng oracle, fuzz-target builds and an isolated offline package build pass.
A further fresh-corpus AddressSanitizer campaign passes 20,000 PNG executions
in 90 seconds with 395 MiB peak RSS. The full report is
`fuzz/reports/png-registry-linux.log`; the current instrumented binary's
`__asan_init` symbol was verified. Recorded sanitized campaigns now total
130,000 executions, without a saturation or exhaustive-conformance claim.

Profile-container milestone: ICC framing now checks declared size, `acsp`,
RGB/GRAY correspondence to the PNG color type, connection space, embedded class,
rendering intent and tag-table bounds/alignment. ICC v4.4 additionally checks
unique signatures, complete-data sharing and contiguous nonoverlapping elements.
Older storage layouts remain accepted; pinned independent v2.1 compact profiles
exercise overlap/reuse that must not be rejected using the newer v4.4 rules.
Exif checks both TIFF byte orders, magic, first-directory offset/count and table
bounds. Unsupported containers become unknown raw ancillary metadata when
decoding, and encoders reject them before writing. ICC transform/tag semantics,
Exif tag meanings, additional directories and thumbnails remain opaque.

`color_description_with_limits` checks candidates in precedence order, skips
unsupported ICC containers and propagates resource errors. The convenience method
uses default limits and returns None on errors. It does not scan image samples
or inflate lower-precedence metadata after finding a supported description.

Eight CC0 [independent ICC profiles](tests/fixtures/icc/README.md), with pinned
source commits/hashes, pass 960 native/reference wire cases across every legal
PNG format, both interlace settings, both metadata compression modes, reversed
tag-table order and both writers. Mutation tests cover profile lengths/signatures,
color mismatches, tables, aliases, v4.4 overlap/gaps and Exif framing/truncations.
The C oracle adds 480 byte-exact ICC/Exif comparisons across all formats and both
Exif byte orders. Full release tests (57 passed, two optional C tests ignored),
strict Clippy, separate external C tests and isolated offline package tests pass.

The new structured profile fuzzer reconstructs metadata compression and chunk
CRCs to reach these validators; it checks buffered/fragmented acceptance, raw
retention, encoder rejection, supported roundtrips and Rust png metadata parity.
It passes 20,000 Linux AddressSanitizer executions in 245 seconds, peak RSS
372 MiB, from 1,800 deterministic eight-byte seeds. The full log is
`fuzz/reports/profiles-linux.log`; `__asan_init` was verified in its binary.

sRGB relationship audit: encoder validation now rejects a supplied sRGB chunk
whose accompanying gAMA or cHRM values differ from PNG 11.3.2.5 Table 17. The
condition is independent of chunk order and still applies when cICP is present.
Tests cover all 15 formats and four intents, four incompatible gamma values and
each chromaticity component, before/after sRGB. The decoder retains lower-priority
wire values and selects supported sRGB; native samples remain unchanged. Both
encoders reject before writer output, while the same gAMA/cHRM values remain
encodable when no sRGB chunk is supplied.
The complete release suite passes 58 tests (two optional C tests ignored), strict
Clippy passes, and the updated isolated offline package build passes. The
production dependency tree remains only the shared `deadlib-image` package.

`CONFORMANCE.md` now inventories the functional clauses through 15.3.4 with
concrete evidence and open items. It explicitly distinguishes native sample /
metadata processing from caller color/display policies. Current audit priorities
include systematic unknown/order/field mutations,
DEFLATE header/tree equivalence classes and APNG error-recovery policy. This is
not a declaration of complete conformance or competitive performance.

cICP audit milestone: [H.273 (07/2024)](https://www.itu.int/rec/T-REC-H.273-202407-I/en)
assigns primaries 1, 2, 4â€“12, 22 and transfers 1, 2, 4â€“18. Code 2 means unspecified;
it remains supported. Reserved primaries/transfer values decode as unknown raw
ancillary metadata, so supported lower-precedence descriptions can be selected.
Both encoders and typed serialization reject reserved fields. PNG's matrix-0 and
binary range-flag requirements remain enforced. Tests enumerate the complete
131,072 primaries/transfer/range combinations, all 256 values of each PNG-local
field, every individual reserved code through buffered/reader/one-byte events,
and all 408 assigned combinations through both writers and readers.
The full release suite passes 61 tests (two optional C tests ignored), and strict
Clippy passes. No production dependencies or application integration changed.

Compression matcher milestone: both compression APIs use four-byte flat hash
heads, a 32 KiB backward-link ring, at most 16 adaptive candidate searches,
portable eight-byte prefix comparisons, sparse insertion and incompressible-input
backoff. Fixed blocks have a bit/byte checkpoint and are replaced with stored
coding when cheaper, before output is drained. Streaming history is copied once
per block; current-block references read the input block. Constant-time integer
length/distance mappings replace linear searches. All 33,024 legal code inputs
are checked against the RFC tables. No unsafe code, SIMD or production codec
dependencies were added.

Tests cover mixed coding, full-window reuse, every prefix mismatch position,
wrapped u32 heads, one-byte fragments, constant capacity and a 16-bit gradient
size regression through independent readers. The full release suite passes 66
tests, with three optional C tests run separately. Strict Clippy, the isolated
offline build, 1,061 native-sample C cases, 480 exact profile cases and 216
external-corpus C cases pass. The latter use 24 original RGB8 images and both
writers / compression modes / interlace settings. Only URL/hash manifests are
committed; those downloaded images remain in ignored build storage.

Three warmed, single-CPU synthetic repetitions measure writer throughput at
36?58 MiB/s versus 22?46 before. The 16-bit gradient shrinks from 501,030 to
40,287 PNG bytes, while measured image-pipeline capacity falls by 196,584 bytes
at width 512, independent of height. Raw artifacts, failed performance trials,
source/binary hashes and methodological limits are in `BENCHMARKS.md` and
`experiments/`. Token planning helped synthetic noise but regressed the photo
corpus, so the selected version uses direct emission and deferred history copies.
The host is shared; neither these synthetic results nor the 24-image RGB8 corpus
establish general competitive performance. Dynamic-Huffman encoding remains absent.

The earlier token prototype's new raw-data `deflate_stream` sanitizer target
passes 20,000 executions in 994 seconds (406 MiB peak RSS), starting with 185
files / 1,878,670 bytes and a 128 KiB input cap. Its fresh 1,080-seed structured
`encode_stream` follow-up passes 20,000 in 469 seconds (404 MiB). Both use the
same pinned Linux nightly/AddressSanitizer toolchain and seed as earlier runs;
their instrumented binaries exported `__asan_init`. Complete logs preserve these
40,000 prototype executions, bringing historical smoke campaigns to 190,000
across seven targets. Reference code contributes to counters; native-only coverage
and saturation are not claimed.

The selected direct-emission encoder then passes 5,000 raw streaming executions
in 231 seconds (389 MiB) and 5,000 structured image executions in 140 seconds
(393 MiB), using fresh 185-file / 1,080-file seed directories. The source, harness
and binary hashes, options and exact logs are recorded in
`fuzz/reports/final-source.json`. Both selected binaries export `__asan_init`.
These 10,000 runs bring recorded sanitizer history to 200,000 executions across
seven targets. The retained prototype counts remain separate from final-code
evidence; no saturation or complete conformance claim follows from these totals.

Zlib framing audit milestone: `tests/zlib_headers.rs` enumerates all 65,536
CMF/FLG pairs through buffered and one-byte zlib input, and buffered/reader PNG
decoding. Exactly 32 dictionary-free combinations succeed, 34 otherwise-valid
dictionary headers fail, and 65,470 invalid headers fail. This includes all four
FLEVEL hints and the two dictionary combinations with two legal FCHECK values.
Valid header fixtures also match independent flate2 and Rust png output; prefix
consumption, zero-output decoding and terminal header errors are checked.

Independently assembled stored/fixed streams exercise 92 combinations of eight
advertised window sizes, distances immediately below/at/above the boundary,
history beyond 64 KiB, zero/one alignment padding and four IO fragment pairs.
Another 24 fixtures reject references before any available output, including PNG
wrappers with valid filtering, dimensions, CRC and Adler values.

The C oracle adds a bounded `--zlib-batch` protocol and an optional build using
zlib's `INFLATE_STRICT` flag. Both builds check 65,652 raw cases. Default zlib
accepts all 28 over-advertised-window references; its strict build rejects them
and matches native behavior. Both reject the 24 unavailable-history references;
libpng also rejects all 24 PNG wrappers at distance validation. The flate2/miniz
reference accepts those 24 raw malformed streams, while Rust png rejects the
PNG wrappers. Negative-oracle acceptance is therefore not used to weaken native
validation. These behaviors follow the context-sensitive RFC 1950/1951 checks,
not an assumption that every reference library has identical recovery policy.

`zlib-conformance.log` records both configurations and the existing 1,757
PNG/profile C regression comparisons. `zlib-conformance.json` records exact
source/binary hashes, reference features and pinned commits. Full release tests
(70 passed, four optional C tests ignored), strict Clippy, both C configurations
and an isolated offline package build pass. Production source is byte-for-byte
unchanged from `5a6747e32`, so this milestone strengthens conformance evidence
without changing the measured encoder/decoder implementation. Broader malformed
Huffman and PNG/APNG grammar audits remain open.

The requested end state is not yet achieved. Continue without application
integration, preserving unrelated work and committing verified milestones:

1. Continue the clause-by-clause field and error-behavior audit beyond the now-
   covered indexed-color recovery, text decoding and encoder field distinctions.
2. Resolve the open rows in the Third Edition clause matrix, including ancillary
   semantic edge cases and the scope of BCP47 extension semantics,
   all Huffman tree corner cases, malformed APNG ordering, and unknown chunks.
3. Extend the generated image properties, broader structured mutations,
   independent APNG compositing comparisons, longer sanitizer campaigns and
   coverage review beyond the current streaming and CRC-repaired smoke targets.
4. Improve compression ratio/match selection and profile filters/inflation on a
   representative external corpus. Add controlled repetitions, allocation and
   peak-memory measurements, streaming benchmarks, and comparable libpng timings.
   Select SIMD only after a measured bottleneck justifies it. Current decode and
   compression results do not meet the competitive-performance objective.
5. Audit every original requirement against current code and reproducible
   evidence; publish the final coverage/performance/limitations documentation.
   Keep the goal active until every gate is proved, rather than treating the
   present feature list or green tests as full conformance.
