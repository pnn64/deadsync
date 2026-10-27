# PNG Third Edition implementation and verification

This is an incremental implementation, not yet a claim of complete conformance.
The current deliverable is independent PNG/APNG decoding, exact native samples,
rendering and measured competitive decoder performance. Encoder features,
compression ratio and encoder speed are outside the completion requirements.
Milestones are committed only after their relevant checks pass.
The [clause audit](CONFORMANCE.md) maps the Third Edition to current code,
evidence, application responsibilities and explicitly open items.

| Requirement | Implementation / authoritative evidence | State |
| --- | --- | --- |
| Separate packages, no application integration | Package names, production tree, isolated offline workspace tests (`independent-build.log`) | Verified outside the application workspace; production dependency is only `deadlib-image` |
| Native samples, all legal formats | Shared `Image`, `Samples`, `ColorType` | Primitives implemented |
| CRC-32 / Adler-32 | Known-answer tests | Verified known answers |
| Native zlib / DEFLATE decode | Stored, fixed, dynamic trees, advertised/actual history and output limits; compression / zlib-header / Huffman / prefix suites | All 65,536 headers, 92 window fixtures, 19,645 Huffman fixtures, 88,414 prefix/suffix cases and virtual cursor overflow boundaries pass; arbitrary tree permutations and coverage review remain open |
| PNG chunk framing and normative ordering | Independent `tests/chunks.rs` fixtures, malformed/metadata/APNG suites | All 16 property bits, all name-byte values, 25 standard names' case variants, 720 core permutations and every small-stream IDAT split pass; broader ancillary/APNG ordering audit remains open |
| All five color types and all legal bit depths | `tests/static_png.rs`, `tests/pngsuite.rs` | 3,240 native/reference matrix cases and 161 valid PNGSuite files pass |
| Five filters and Adam7 decode | Same format matrix, PNGSuite pass geometry cases | Verified exact native samples |
| Indexed-color error recovery (13.1) | `tests/palette.rs`; all indexed depths, Adam7, APNG, one-byte reads/events, independent Rust png fixtures | Native indices retained, opaque-black expansion/rendering; opt-in strict decode |
| Metadata, including Third Edition color chunks | `metadata.rs`, metadata / text / field / IANA / profile tests, PNGSuite | Typed wire roundtrips, UTF-8, BCP47 registration, method distinctions, sample masks, histogram usage and profile containers checked; extension-specific and payload semantics still require audit |
| Latin-1 text-body recovery | `tests/text_body.rs`, independent stored-zlib fixtures and C named-metadata oracle | All 256 byte values across 6,144 files; non-null decoder recovery, NBSP/long text and iTXt Unicode distinctions verified |
| sRGB fallback metadata | `tests/metadata_fields.rs`, metadata selection API | All 15 formats / four intents and fallback orders pass; original metadata values remain available |
| cICP code-point validity | `tests/cicp.rs`, H.273 (07/2024) | All 131,072 primaries/transfer/range combinations, all matrix/flag bytes and reserved-code fallback behavior pass |
| mDCV/cLLI exact fixed-point metadata | `tests/hdr.rs`, independent stored-zlib/CRC fixtures, C fixed-point getters and Rust png metadata | All 8,192 field-byte mutations, 60 integer boundaries, 132 length cases and 120 ordering permutations pass; exact units/order checked across all 15 formats |
| APNG frame decoding and compositing | `apng.rs`, `composite.rs`, APNG/compositing/recovery/rendered tests | Exact native frames, 4,530 independent faults, legal fcTL-before-acTL ordering, 983,040 rational alpha pairs and 2,121 independent RGBA16 canvases pass, including replay/reset |
| Bounded streaming with arbitrary input boundaries | `zlib::Inflater`, `StreamDecoder`, `decode_reader` | PNG/APNG native row pipelines, arbitrary IO fragments, finish/errors and capacity bounds verified |
| Differential tests: Rust png, libpng, PNGSuite | Rust png 0.18.1, libpng 1.6.50 + zlib 1.3.1, vendored PNGSuite | 1,061 native-sample plus 480 exact ICC/Exif libpng comparisons; format matrices and PNGSuite pass |
| Independent fixtures / malformed streams / properties | Independent stored-stream known answers, 6,005 chunk-framing/grammar cases across three parser entry patterns, malformed fixtures, mutation campaign | These equivalence classes pass; larger campaigns pending |
| Fuzz targets and reproducible campaigns | Isolated `fuzz/`, generated-image properties, deterministic seeds and reports | Twelve targets; 1,410,000 recorded successful sanitizer executions across milestones, including historical encoder runs; newest 50,000 use native-crate feedback with artifact-verified isolation; targeted decoder gaps and longer campaigns remain open |
| Benchmarks and profile-guided optimization | Decoder-only harnesses, `BENCHMARKS.md`, checked-in TSV and profiles | Bulk copying, filter reconstruction and buffered Adler-32 improvements measured; competitive decoder performance still pending |

Historical milestones: (1) shared primitives and native compression; (2) static PNG codec and
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
without changing the measured encoder/decoder implementation. The subsequent
Huffman audit below extends malformed compression evidence; PNG/APNG grammar
audits remain open.

Huffman grammar milestone: `tests/support/huffman.rs` constructs 19,645 RFC 1951
wire fixtures without importing a codec or the native table builder. There are
3,091 valid streams and 16,554 malformed streams. Six normal tests check native
buffered decoding, four IO fragment pairs, whole-input/one-byte-output prefix
consumption, terminal errors and PNG buffered/reader wrappers. Valid fixtures
also check flate2 and Rust png samples, except their explicitly asserted
rejections of the 68 published-range distance-definition cases described below.

The matrix includes all 16,384 dynamic count-field bit combinations with an
invalid empty code-length alphabet, plus 1,024 otherwise valid bodies isolating
literal/distance count bounds and 15 valid code-length alphabet counts. It covers
all 1,525 complete code-length histograms with 2–19 leaves and width at most seven,
89 empty/single/incomplete/oversubscribed tree cases, 187 repeat-boundary cases,
74 reserved-symbol/definition cases and 347 canonical/range cases. Histogram
coverage does not enumerate all symbol permutations. The 32 large fixed/dynamic
fixtures test all distances 1–32,768 at length three and every length 3–258 at
eight edge/overlap distances, at all eight starting bit offsets and both padding
values. This is not the Cartesian product of all lengths/distances or every
possible wire encoding of a value.

Default C zlib and a strict-window compatibility build each check all 19,645
records. Default zlib, flate2/miniz and Rust png reject 68 otherwise valid
HDIST 31/32 definitions, even when the reserved symbols are unused. The published
RFC permits those definitions; native decoding and the compatibility build
accept them. Both still reject actual reserved distance symbols. The same C
compatibility flag also accepts 64 otherwise valid streams with 287/288 literal
definitions, exceeding the published limit. Native decoding and default C zlib
reject them. Tests record both differences rather than treating every reference
as an interchangeable positive/negative oracle. `huffman-conformance.log` and
`huffman-conformance.json` retain exact commands, flags, source/binary hashes and
the full release/isolated results: 76 passed and five optional C tests ignored.
Strict Clippy also passes; production source remains unchanged from `5a6747e32`.

The eighth sanitizer target, `huffman`, selects these independent streams and
mutates compressed bits, truncation, suffixes, limits and IO fragments. A fresh
649-file / 7,788-byte selector corpus passes 20,000 executions in 857 seconds,
with a 394 MiB peak RSS. Its AddressSanitizer binary exports `__asan_init`.
`fuzz/reports/huffman-source.json` binds source/harness/binary hashes to the
complete log, toolchain and seed. An initial 697-execution harness timeout is
retained separately: its stack identified a full 4 KiB sentinel scan per
one-byte input fragment. Bounded tail guards fix that harness overhead, and the
exact input replay and fresh campaign pass. No production codec change was
needed. The failed preliminary run and fixed-input replay are excluded from the
220,000 successful campaign executions across eight targets.

Native-only LLVM coverage runs all 76 tests under Linux nightly branch
instrumentation, then selects the ordinary library mapped by 20 integration
executables. Unit-test binaries, embedded test functions, reference codecs,
test harness source and the standard library are excluded from reported totals.
The 20 native source files report 3,783/3,916 lines (96.60%), 4,562/4,941 regions
(92.33%) and 1,077/1,286 instrumented branch outcomes (83.75%). These integration
coverage results are separate from libFuzzer's counters and do not imply all
paths, all inputs or complete conformance. The reviewed gaps concentrate in
malformed APNG framing/encoder validation, PNG chunk/field boundaries, stream IO
errors and public error formatting. `coverage-reports/README.md` records scope,
commands, tool versions, uncovered annotations and the next targeted work.

Inflation optimization milestone: portable 32-bit Huffman refill and direct
canonical prefix comparison improve the measured photo pipeline. Three warmed
single-CPU repetitions give 78.98 MiB/s buffered PNG decoding versus 68.19 before,
and 64.03 MiB/s reader decoding versus 56.69. Saved old binaries give later
controls of 70.81 / 58.54; Rust png is 198.02 / 202.58 / 204.23 across the three
groups. Relative to that reference, the evidence suggests about 13% buffered and
10% reader improvement on these inputs. Raw inflation and synthetic measurements,
reference drift, exact source/patch/binary hashes and methodological limits are
retained in `BENCHMARKS.md`, `benchmark-inflate-summary.json` and `experiments/`.
Encoded sizes and measured pipeline capacities match the old-code controls.
The much faster Rust png results show that competitive performance remains open.

`inflate-validation.log` / `.json` record all 81 passing tests including five C
checks, all fuzz harnesses/normal targets under strict Clippy, both special C
configurations and an isolated offline build. Fresh selected-source ASan runs
complete 5,000 executions each of Huffman (118 seconds, 396 MiB), PNG streaming
(15 seconds, 193 MiB), raw zlib (6 seconds, 128 MiB) and profiles (65 seconds,
368 MiB). Their binary definitions of `__asan_init` and source/harness/binary
hashes are retained in `fuzz/reports/inflate-source.json`. These 20,000 executions
bring historical successful campaigns to 240,000 across eight targets.
An initial Linux metadata-copy failure happened before execution; a fresh native
Windows copy was byte-verified and used instead. No failed staging is counted.

The final coverage script also completes a fresh run in one pass, with embedded
unit-test binaries and reference/harness sources excluded from report mapping.
The selected ordinary library has 3,793/3,926 covered lines (96.61%),
4,574/4,953 regions (92.35%) and 1,081/1,290 instrumented branch outcomes (83.80%).
Results, environment and runner/source/executable hashes are under
`coverage-reports/inflate-*`. Native fuzz coverage remains a separate open item.
At that checkpoint, the general raw flate2 fuzzer needed a compatible positive oracle for legal
HDIST 31/32 streams; the structured Huffman and C compatibility checks cover
those positives, while default flate2 rejects them. The next milestone resolves
the general harness gap without changing production code.

Positive-oracle milestone: pinned libdeflater/libdeflate-sys 1.26.1 supplies
libdeflate 1.26 in the isolated fuzz workspace. Every native-accepted raw stream
and structured Huffman mutation now has an independent sample check, including
HDIST 31/32. The old saved harness fails on an exact legal regression fixture;
the updated ASan binary passes it. Deterministic Windows/Linux checks validate
all 3,091 positive fixtures, all 68 extended-distance cases and all 32 zlib
window/level headers. Native rejects all 16,554 malformed fixtures. The C
reference permits some malformed forms, so strict negative validation stays in
the native/C suites. A 64-byte probe records its acceptance of 64 excess-literal
count cases and one unused code branch.

The seed generator adds 649 raw Huffman wires, producing 810 raw seeds totaling
4,803,389 bytes; a 256 KiB input cap accommodates the largest 136,426-byte stream.
Fresh ASan campaigns pass 20,000 raw executions (18 seconds, 378 MiB) and 10,000
structured executions (157 seconds, 404 MiB). The C reference is instrumented
with Clang 19.1.7 AddressSanitizer as well. Exact replay/campaign, source/package,
corpus/archive/binary records are in `fuzz/reports/reference-source.json`.
These add 30,000 successful executions, totaling 270,000 across eight targets;
fixed-input replays are excluded. Normal tests (76 passed, five optional C checks
ignored), both Clippy checks and the isolated offline build pass. The normal
dependency graph remains only the local shared image crate, and production
source is byte-for-byte the `cafedc93d` implementation. Validation records are
`reference-conformance.log` / `.json`. Broader native fuzz coverage remains open.

The requested end state is not yet achieved. Continue without application
integration, preserving unrelated work and committing verified milestones:

1. Continue the clause-by-clause field and error-behavior audit beyond the now-
   covered indexed-color recovery, text decoding and encoder field distinctions.
2. Resolve the open rows in the Third Edition clause matrix, including ancillary
   semantic edge cases and the scope of BCP47 extension semantics,
   arbitrary Huffman symbol permutations, malformed APNG ordering, and unknown chunks.
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


## Literal chunk names, framing and editor-copy audit (2026-10-27)

`tests/chunks.rs` and its shared independent wire constructors cover 6,005 file
cases through buffered decoding and one-/seven-byte readers:

- All 16 property-bit combinations at three legal positions with empty/binary
  payloads, plus all 256 byte values in each of the four name positions (3,168).
- All 15 noncanonical case variants of each of the 25 standard names at those
  positions (1,125). Ancillary variants remain exact unknown data; critical
  variants fail. The defined standard names' payload semantics remain covered
  by their separate format/metadata/APNG tests.
- All 720 permutations of IHDR, PLTE, tRNS, two distinct compressed IDAT pieces
  and IEND; only the original file order is valid. All 768 combinations of every
  small-stream compressed-byte split, property flags and unknown-chunk insertion
  before/between/after the IDAT pair pass their expected acceptance/rejection.
- Sixteen unknown-ancillary insertions before IHDR or after IEND are rejected;
  78 payload-length/alignment positives, 52 CRC-corruption cases and 78 declared
  length-boundary cases pass. Large declared bodies fail without requiring a
  multi-gigabyte fixture or allocation.

Direct push decoding additionally checks 144 property/position/fragment cases,
including exact unknown metadata events, verified image-end order and fused
unknown-critical errors. Both encoders check all 48 property/position choices,
all 816 nonalphabetic name-byte substitutions before writing, and repeated
safe-to-copy chunks with exact empty/binary payloads on both sides of IDAT.
Unknown unsafe chunks are omitted after critical IDAT replacement; known gAMA
and tIME chunks retain their defined behavior. Names with the reserved bit set
are accepted as unknown when decoding and rejected by current-version encoders.

Rust png 0.18.1 independently checks valid case variants, IDAT splits and writer
output. It accepts 11 of the 719 malformed core permutations. The pinned C
libpng 1.6.50/zlib 1.3.1 executable compares 1,893 fixtures and accepts three of
those malformed permutations. It rejects all 312 reserved-bit ancillary files
in that comparison; the native decoder follows Third Edition 13.5's unknown
chunk handling instead. The exact permutations and reference policies are
recorded in `chunks-validation.log`, `chunks-validation.json` and the oracle
README. Runtime parser/encoder logic required no change in this milestone.

Release and isolated offline tests pass 82 tests with six optional C tests
ignored; the new optional C test and strict all-target Clippy check also pass.
The isolated dependency tree remains only `deadlib-image`. Existing format,
PNGSuite, APNG, metadata, streaming and compression checks remain green.

The seed generator adds 1,893 grammar files to each PNG fuzz target, with
case-sensitive wire names encoded into filesystem-safe seed names. Fresh
corpora contain 2,069 files / 383,803 bytes each (maximum file 63,435 bytes).
Linux nightly/Clang ASan completes 20,000 `decode` executions in four fuzzing
seconds (249 MiB peak RSS) and 20,000 `png_stream` executions in 16 seconds
(422 MiB). Both binaries define the ASan runtime entry. Exact commands, corpus,
source and binary hashes are in `fuzz/reports/chunks-source.json`; full logs are
retained alongside it. These 40,000 successful executions bring the historical
successful total to 310,000. Counter totals include harness/reference code and
are not native-only fuzz coverage or proof of completeness. Broader ancillary
and APNG ordering, metadata field boundaries, external rendered APNG comparisons
and competitive pipeline/encoder performance remain open.


## Named ancillary and suggested-palette audit (2026-10-27)

`tests/keywords.rs` constructs 19,026 files from 2,718 independent name cases
and seven wire forms: tEXt, zTXt, plain/compressed iTXt, iCCP, and 8-/16-bit
sPLT. Its constructors use independent stored zlib, big-endian field assembly
and bitwise CRCs. All byte values are tested in single/first/interior/last
positions and five positions of 79-byte names. Lengths 0 through 256, every
single-space position, and every consecutive-space position in a 79-byte name
are covered. There are 13,027 positive and 5,999 malformed files.

Buffered, one-byte and 17-byte readers agree with these expected outcomes and
retain exact raw names, data, typed values and native samples. Both encoders
check the same matrix before output. Typed metadata construction checks both
compression modes; all positive writer outputs have independently compared
Rust png 0.18.1 pixels and text/profile values. Rust png exposes neither the ICC
profile name nor sPLT, which the extended C oracle checks instead. Direct push
metadata and fused-error tests cover 3,584 single-byte-name/fragment cases.
Repeated text keywords are valid; sPLT name uniqueness is case-sensitive, names
can be shared across metadata types, and iCCP remains a singleton.

Suggested palettes additionally cover all 256 depth bytes, all incomplete-entry
remainders, zero through 257 entry-count boundaries, 49 frequency pairs for
each depth, sample range boundaries in all four channels, and both palette
depths in all 15 PNG image formats. Inputs and expected values use independent
big-endian fields. Zero entries, repeated entries, ties, zero frequencies,
non-gray suggested colors in grayscale images, and palette depths independent
of image depth are preserved. More than 256 entries are allowed. Ascending
frequencies, incomplete entries and unsupported depths are rejected; supplied
eight-bit channel values above 255 are rejected before encoding.

The pinned C libpng/zlib oracle compares 3,710 named-field files: all 2,702
positives retain exact names and values, while malformed ancillary fields use
its documented recovery policy. Another 158 sPLT comparisons retain 114 valid
palettes exactly, ignore two valid empty palettes and retain 42 invalid
ascending-frequency palettes. Native validation follows the normative field
constraints rather than those recovery choices. The original oracle modes
still pass 1,061 native-pixel and 480 exact ICC/Exif comparisons.

Release and isolated offline runs pass 86 tests, with eight optional C tests
ignored. Four relevant optional C tests and strict all-target Clippy pass.
The isolated normal dependency tree remains only `deadlib-image`.
`keywords-validation.log`/`.json` retain exact commands and source/oracle hashes.

The seed generator adds 3,710 named-field files to each PNG corpus. Each fresh
corpus contains 5,779 files / 1,251,344 bytes (maximum 63,435). Linux nightly
ASan completes 20,000 decode executions in four fuzzing seconds (372 MiB peak
RSS) and 20,000 CRC-repaired streaming executions in 16 seconds (460 MiB).
Source/corpus/binary hashes and runtime instrumentation checks are retained in
`fuzz/reports/named-source.json`, with full logs. These 40,000 successes bring
the historical successful total to 350,000; native-only fuzz coverage and longer
campaigns remain open.

Runtime library logic did not change in that milestone. Its audit exposed a
separate encoder boundary: tEXt/zTXt construction then accepted all 255 non-null
byte values, including 63 controls outside the printable Latin-1/LF repertoire
specified in 11.3.3.1. The following milestone corrects that boundary. The broader
APNG/fallback/rendering and performance work remains open.


## Strict Latin-1 text encoding with decoder recovery (2026-10-27)

Both PNG writers and typed `Metadata::Text::to_chunk` now enforce the text-body
repertoire from Third Edition 11.3.3.1/11.3.3.2: LF, bytes 0x20?0x7E and
0xA0?0xFF. This differs from the keyword rules: body spaces may lead, trail or
repeat, and NBSP is valid. Invalid C0/C1 controls and NUL are rejected before
writer output. Validation is a single allocation-free pass; no text bytes are
normalized or converted. Reencoding a recovered legacy control body therefore
returns an error until the caller supplies conforming metadata.

Decode still retains every non-null byte, including the 63 non-LF controls,
as raw and typed text. NUL remains invalid. iTXt keeps its distinct Unicode
rules: non-null control characters are discouraged but permitted; the Latin-1
encoder check is confined to tEXt/zTXt. Display filtering remains application
policy. The normative source is the pinned Third Edition used by the clause
audit, especially 11.3.3.1?11.3.3.4 and 13.7.

`tests/text_body.rs` checks all 256 byte values in four contexts, both chunk
types and all three placements (6,144 independent files). Buffered and one-/
seven-byte readers verify exact native samples, raw chunks and typed bodies;
both writers and Stored/Fast typed construction check the separate expected
encoder domain. All 4,608 conforming file cases have exact writer roundtrips
and Rust png text/pixel comparisons. The 6,120 non-null files decode, while
1,536 cases reject encoding. One-byte push events, fused NUL errors, all 256
Unicode code points through iTXt, empty text and body lengths through 65,537
bytes are checked separately. The latter includes NBSP-only bodies and lengths
around 79 and 64 KiB, with both compression modes and both writers.

The final ESC regression test fails against the retained isolated `4adad99dd`
source and passes after the fix. Its before-source and final-test hashes,
command and expected exit 101 are in `text-body-before.json`/`.log`.
Libpng compares 384 conforming raw byte bodies and 768 native writer outputs
exactly. Its recovery retains 126 malformed non-null control bodies and changes
two NUL bodies; those choices are recorded separately from native encoder
conformance. The existing 1,061-case C native-pixel matrix also passes.

Release and independent offline runs pass 90 tests, with nine optional C tests
ignored. Both relevant optional C tests pass, as do main and fuzz all-target
Clippy with warnings denied. The isolated production dependency tree remains
only `deadlib-image`. Exact commands, source/oracle hashes and suite results
are retained in `text-body-validation.log`/`.json`.

The new ninth fuzz target, `text_body`, uses bounded independent PNG/zlib/CRC
fixtures to check raw recovery, typed construction, encoder validation before
output, both writers and independent Rust png values. Its 4,304 structured
seeds total 280,720 bytes, including all single bytes/flags, body lengths
79/80/4096, graphic text, late controls and embedded NUL. Both PNG fuzz corpora
also gain 512 independent byte-body files, for 6,291 files / 1,309,712 bytes
(maximum 63,435 bytes) each.

Fresh ASan `text_body`: 20,000 executions, 46 fuzzing seconds, 437 MiB peak RSS; instrumented runtime entry verified.

Fresh ASan `decode`: 20,000 executions, 5 fuzzing seconds, 414 MiB peak RSS; instrumented runtime entry verified.

Fresh ASan `png_stream`: 20,000 executions, 13 fuzzing seconds, 440 MiB peak RSS; instrumented runtime entry verified.

The 60,000 successful executions bring the historical successful count to
410,000. Full logs and source/corpus/binary hashes are in
`fuzz/reports/text-body-source.json` and the corresponding target logs. Smoke
coverage counters include harness/reference code; longer campaigns, native-only
fuzz coverage, remaining field/APNG audits, broader external rendered-frame
comparisons and competitive pipeline/encoder performance remain open.


## APNG recovery with continued static validation (2026-10-27)

`DecodeOptions::animation_policy` now offers `AnimationPolicy::RecoverDefault`;
`Strict` remains the default. Recovery emits one `DecodeEvent::AnimationError`,
retains the first fault in `Png::animation_error` / `StreamDecoder::animation_error`,
and discards all animation frames. The static image preserves its original
native representation. Both encoders omit the diagnostic and can write a clean
static PNG. Valid animations produce identical strict/recovery results.

```rust
let options = deadlib_image_png::DecodeOptions {
    animation_policy: deadlib_image_png::AnimationPolicy::RecoverDefault,
    ..Default::default()
};
let png = deadlib_image_png::decode_with_options(bytes, &options)?;
// png.image is verified; png.animation_error reports any discarded animation.
```

This implements the recovery recommendation in Third Edition 13.1 without
silently accepting animation order errors. It is an explicit policy: static
IDAT/framing/metadata, unknown critical chunks, resource caps and reader IO
errors remain fatal. Recoverable APNG CRC errors are diagnosed; subsequent APNG
payloads are skipped with bounded state. A low-level consumer must discard
cached frames on the event and use the static image only once its own
`ImageEnd(Default)` has been observed. Fatal later errors still allow an
application to retain a previously verified static image, while the whole-file
reader returns an error.

The recovery slice API uses the streaming collector, avoiding a second copy of
recovery logic. Strict slice parsing remains independent. Fault tags distinguish
control start, frame payload, completed chunk and frame-end failure. Input-prefix
accounting commits only container bytes; failed inflater lookahead is replayed
through the discard/CRC path. Frame output beyond its exact filtered extent is
classified as malformed frame data, while actual caller limits remain fatal.
No new production dependencies were added.

`tests/apng_recovery.rs` and independent stored-zlib/CRC/Adam7 fixtures check
60 valid format/interlace/default-role combinations against literal native
images and Rust png frames, plus 3,000 malformed animation files. They cover
control lengths/counts/plays, sequence gaps/range, rectangles (including smaller
illegal default-frame rectangles), disposal/blend values, missing/duplicate/
misplaced chunks, bad frame zlib/Adler/filter/output length, APNG CRCs and
multiple failures. Whole slice, one-/seven-byte reader and one-/seven-/whole-file
push paths preserve exact static samples/metadata and report one first error.

Additional checks cover all 120 control/data permutations, all truncations of
a damaged animation, empty and byte-split fdAT data with CRC failures in either
part, bounded 64 KiB discarded chunks, IO failures, prefix parsing, trailing
data, diagnostic-free reencoding, palette-policy interactions and fatal resource/
static faults. The 120-permutation fixture keeps core structure legal; moving
IDAT can move text from before to after the data, and the expected metadata
positions are derived from that actual static wire order.

The pinned C oracle returns exact static native pixels for 2,880 fixtures and
rejects 240 APNG CRC failures. Native recovery deliberately handles the latter
with an explicit diagnostic; it does not weaken static PNG CRCs. The previous
1,061-file C native-pixel matrix also passes.

Release and independent offline runs each pass 96 tests, with ten optional C
tests ignored. Both relevant C tests pass separately, as do main and fuzz
all-target Clippy with warnings denied. The isolated production dependency tree
contains only `deadlib-image`. Exact commands and source/oracle hashes are in
`apng-recovery-validation.log` / `.json` and `independent-build.log`.

The seed generator adds 3,060 independent valid/fault APNG files to each PNG
corpus. Each fresh PNG corpus now has 9,351 files / 2,316,164 bytes, with a
63,435-byte largest file. `png_stream` checks three configurations per input:
repaired CRCs with strict animation handling, repaired CRCs with recovery, and
original CRCs with recovery. Successful recovered images must equal independent
strict buffered decoding after stripping only the APNG chunks. Indexed policy
selection uses input-length parity so both policies are reached with an intact
PNG signature. These three checks count as one libFuzzer input execution.

Fresh Linux ASan campaigns each complete 20,000 executions: `png_stream` in 21
fuzzing seconds / 461 MiB peak RSS, `decode` in 3 seconds / 388 MiB, and
`encode_stream` in 89 seconds / 414 MiB. Each binary defines the ASan runtime
entry. Source, fresh-corpus and instrumented-binary hashes are recorded in
`fuzz/reports/apng-recovery-source.json`; all three full target logs are retained.
The 60,000 new successful input executions bring the historical count to
470,000. This is smoke evidence; longer campaigns, native-only coverage, broader
external rendered APNG comparisons, remaining metadata field/order audits and
competitive pipeline/encoder performance remain open.

## Exact HDR field and ordering audit (2026-10-27)

`tests/hdr.rs` adds independent stored-DEFLATE/CRC fixtures for mDCV/cLLI, with
all 256 values at each of their 32 payload-byte positions and 60 explicit
four-byte integer boundaries. Native buffered, fragmented-reader, APNG-recovery
and one-byte push paths accept 7,728 fixtures within PNG's integer range and
reject 524 over-range fixtures. Invalid writer inputs fail before output.
For accepted data, every native sample, raw chunk and typed metadata value is
checked; Rust png independently checks fixed-point metadata and image samples.

Four distinct field profiles across all 15 formats and plain/Adam7 layout are
checked from independent bytes and both native writers (360 files). Literal
BT.2020/D65 bytes catch coordinate order, 50,000 versus 100,000 scaling and
big-endian precision mistakes. White x is 15635 / `3D 13`, using the
[published white-point correction](https://www.w3.org/2025/06/REC-PNG-20250624-errata).
The API now documents the integer units and array order; codec behavior is
unchanged by this audit.

The [Third Edition](https://www.w3.org/TR/2025/REC-png-3-20250624/#mDCV-chunk)
specifies two-byte coordinates, 31-bit luminances, an accompanying cICP for
mDCV, and placement before PLTE/IDAT. cLLI permits unknown values as zero.
[SMPTE ST 2086:2018](https://pub.smpte.org/pub/st2086/st2086-2018.pdf), 5.2 and
Annex A, allow values outside its nominal display ranges for other purposes.
The suite deliberately preserves such raw integers; it does not claim every
tested tuple describes a physical display or invent additional clipping rules.

Grammar checks cover 132 length cases (0–65 bytes for each chunk), all 120
permutations of cICP/mDCV/cLLI/PLTE/IDAT (six conforming orders), singleton
duplication, the mDCV dependency, independent cLLI, and the exact cumulative
36-byte metadata budget. Both encoders validate malformed lengths and duplicates
before output.

The pinned C oracle's new `--hdr-metadata` protocol reads libpng fixed-point
getters without floats. It returns exact values for all 7,728 in-range raw
fixtures plus 60 native writer files. For 524 over-range fixtures it ignores
the affected HDR chunk; the presence flags verify that deliberate C recovery
policy separately from native rejection. Results are retained in
`hdr-oracle-validation.log`. The prior 1,061 native-pixel and 1,280 text-body
C comparisons also pass with the extended oracle.

Full release and independent offline runs each pass 99 normal tests, with 11
optional C tests ignored. Main and fuzz all-target Clippy pass with warnings
denied. The isolated production dependency remains only `deadlib-image`.
`hdr-validation.log` / `.json` retain commands, results and source/oracle hashes;
the complete C HDR run is recorded separately in `hdr-oracle-validation.log`.

The generator adds 8,534 HDR files per PNG corpus, giving 17,885 files /
3,551,398 bytes in each fresh corpus (maximum 63,435 bytes). Fresh Linux ASan
campaigns complete 30,000 inputs each: `png_stream` in 29 fuzzing seconds /
476 MiB peak RSS and `decode` in 6 seconds / 461 MiB. Both instrumented binaries
define the ASan runtime entry; source/corpus/binary hashes and exact commands
are in `fuzz/reports/hdr-source.json`, with full per-target logs. The 60,000
successful inputs bring the historical count to 530,000. Native-only fuzz
coverage, longer campaigns, remaining conformance audits, broader external
rendered APNG evidence and competitive pipeline/encoder performance remain open.


## WPT rendering and fcTL-before-acTL conformance (2026-10-27)

A pinned external WPT input exposed an incorrect native ordering restriction.
The [Third Edition ordering table](https://www.w3.org/TR/2025/REC-png-3-20250624/#5ChunkOrdering)
requires acTL before IDAT, while allowing the first fcTL to precede acTL. WPT
`024.png` uses that legal order. Previously strict decoding rejected it and
recovery discarded a valid animation. Both parsers now accept it. The stream
parser retains one bounded control and delays its public FrameControl event
until the verified Animation event. No frame-row allocation occurs while waiting.
Missing acTL at IDAT, additional pre-IDAT controls, invalid fields/CRCs and caller
limits still fail or recover under their existing policies.

The existing 120-permutation classifier incorrectly required acTL first; it now
recognizes both legal orders. Thirty independent all-format/interlace positives
check exact native samples, Rust png frames, one-/seven-byte push and reader
boundaries, zero-input delivery of the delayed event and fatal frame-count caps.
Another 1,530 malformed prefix cases preserve verified static samples and one
first diagnostic, in addition to the existing 3,000 recovery cases. The final
regression fails against an isolated, hash-verified copy of the pre-fix production
source; `apng-order-before.log` / `.json` retain that evidence.

The new [pinned WPT corpus](tests/fixtures/wpt-apng/README.md) retains 85 exact
upstream files, their BSD license and source hashes. All 35 animations render
201 complete canvases. Test-only libpng decodes each native frame; an independent
Python Fraction compositor uses full-canvas snapshots, immediate disposal and
exact rational straight-alpha arithmetic to produce RGBA16 answers. flate2
independently decompresses the checked-in answers in normal offline tests.
Every sample, frame control, default-image role and plays value matches through
strict/recovery parsing, fragmented readers, reset and both writers with both
interlace and compression modes (280 reencodings). The compositor itself needed
no arithmetic or disposal change.

All 28 applicable published static final-state references match exactly after
projection to their 8-bit precision in the recorded run. The comparison harness
allows one reference level for quantization; the separate 201 RGBA16 comparisons
are exact. One WPT display test requires Display-P3 to sRGB conversion, which is
caller-controlled; its raw stored-space frames still have exact RGBA16 answers.
The ignored regeneration test independently rebuilds all 35 golden files and
compares their uncompressed contents, without relying on native composition.

Release and isolated offline runs each pass 102 normal tests, with 12 optional
C/regeneration tests ignored. Main and final fuzz all-target Clippy pass with
warnings denied. Golden regeneration and the 1,061-file C native-pixel matrix
pass separately. The isolated production dependency remains only
`deadlib-image`; no application integration or production dependency was added.
Commands, source/oracle hashes and results are retained in
`apng-order-validation.log` / `.json` and `independent-build.log`.

The fresh PNG corpora gain 1,560 pending-control and 42 WPT PNG files, for
19,487 files / 4,090,790 bytes each (maximum 63,435 bytes). Both PNG fuzz targets
now allow a bounded 8 MiB of native output and 256 frames so the 128-frame WPT
case is decoded; the independent Rust reference retains separate limits. Fresh
Linux ASan runs complete 30,000 inputs each: `png_stream` in 53 fuzzing seconds /
497 MiB peak RSS, and `decode` in 10 seconds / 553 MiB. Their ASan runtime entries
are verified, with source/corpus/binary hashes in
`fuzz/reports/apng-order-source.json` and full target logs. The historical
successful input count is now 590,000. Native-only fuzz coverage, remaining
conformance audits and measured competitive pipeline/encoder performance remain
open.

## Huffman call-site optimization (2026-10-27)

The measured primary lookup now inlines at symbol call sites, with the unchanged
canonical/error fallback outlined. No codeword, refill, error, limit or format
policy changes. Linux sampling and warmed before/after measurements are retained
in `profiling/` and `benchmark-huffman-call-*`; the interleaved saved-binary
check supports an approximately 12% reference-relative photo decode improvement.
See [BENCHMARKS.md](BENCHMARKS.md#huffman-symbol-call-sites) for rates, experimental
variants, exact source checkpoints, methods and the remaining performance gap.

The full release run passes 114 tests, including all twelve optional C/oracle
and golden-regeneration checks. Main and fuzz strict Clippy pass, as do the
65,652-case strict-window C zlib audit and 19,645-case compatibility C Huffman
audit. An isolated offline source copy passes all 102 normal tests (twelve
optional checks ignored); its only production dependency is `deadlib-image`.
`huffman-call-validation.log` / `.json` and `huffman-call-independent.log`
retain commands, source/oracle hashes and results. Scoped rustfmt checks pass.

Fresh Linux ASan campaigns complete 10,000 structured Huffman, 20,000 raw zlib,
20,000 fragmented inflater and 30,000 PNG-stream inputs. The initial corpora are
fresh byte copies of the existing deterministic seeds, including the legal
distance-count definitions and WPT animations. Respective fuzzing times / peak
RSS are 140 seconds / 409 MiB, 20 / 406, 15 / 401 and 56 / 516. Each binary
defines the ASan runtime entry, and all final source hashes match their initial
snapshot. Exact commands, per-target logs and initial-corpus/source/binary hashes
are in `fuzz/reports/huffman-call-source.json`. These 80,000 successful inputs
bring the historical count to 670,000. Native-only fuzz coverage, longer
campaigns, remaining conformance audits and competitive pipeline/encoder
performance remain open.

## Exact Paeth and byte-stride reconstruction (2026-10-27)

The optimized regional predictor matches the Third Edition's independent
arithmetic and tie order for all 16,777,216 byte triplets. Another 2,550 direct
row cases check independent filtered bytes, adaptive scores and reconstruction
for all six byte strides, all five filters, empty/short rows and first-row
boundaries. All 251 invalid filter bytes retain the original error and leave
the row unchanged. Pixel arrays carry reconstructed neighbors in registers;
two-byte pixels use the measured faster scalar recurrence. No new allocation,
dependency or unsafe code is introduced. Static assembly inspection verifies
compiler SIMD in the selected Linux predictor; its cycle profile attributes
11.89% to unfiltering versus the preceding profile's 22.35%.

The new stored-DEFLATE benchmark independently constructs and checks 80 inputs
covering all filters and eight 8-/16-bit formats (all six byte strides).
Its controls and photo/synthetic pipeline comparisons are retained in
`benchmark-filter-*`, with exact baseline/variant reverse patches in
`experiments/filter-*-checkpoints.json`. The selected hybrid suggests about 12%
better photo decode and 9% better photo reader throughput relative to reference
timings. Synthetic buffered encoding regresses about 10%; writer paths about
5%. An encoder-specific predictor alternative did not consistently remove the
loss, and was rejected. [BENCHMARKS.md](BENCHMARKS.md#byte-stride-filter-reconstruction)
records both experiments, selection, limits and reproducible aggregation.

The full release suite passes 117 tests, including all twelve optional checks.
Strict main/fuzz linting, the 65,652-case strict-window C audit and 19,645-case
compatibility Huffman audit pass. Exact commands and final source hashes are
in `filter-validation.log` / `.json`.

An isolated source copy passes 105 normal offline tests (twelve optional checks
ignored), with only `deadlib-image` in its production dependency tree. The
self-contained filter benchmark also builds and runs offline there, checking
all 80 fixtures and producing 240 operation rows. Its smoke-run timings use the
isolated default release profile and are excluded from comparative results.
`filter-independent.log` and `filter-independent-benchmark*` retain this evidence.

Fresh ASan runs complete 20,000 structured encoder inputs, 30,000 PNG-stream
inputs and 30,000 buffered PNG inputs. The encoder harness checks original
native samples independently through Rust png across all formats, filters,
plain/Adam7 and static/APNG controls. Fuzzing time / peak RSS are respectively
88 seconds / 416 MiB, 57 / 501 and 8 / 501. Each instrumented binary defines the
ASan runtime entry and all final source hashes match the initial snapshot.
`fuzz/reports/filter-source.json` retains commands, initial-corpus/source/binary
hashes and full per-target logs. The 80,000 successful inputs bring the
historical count to 750,000. Competitive pipeline/encoder performance,
compression ratio, remaining conformance audits, broader high-precision APNG
composition and native-only fuzz coverage remain open.

## Reference sample precision (2026-10-27)

`Png::from_reference_samples` explicitly converts straight, interleaved gray,
gray/alpha, RGB or RGBA samples with independently declared 1–16-bit channel
depths. It selects the smallest legal PNG depth, rounds exact integer linear
scaling to nearest, and records original significant bits in sBIT when needed.
Every original value is recoverable from the stored high bits; black, white and
alpha endpoints remain exact. Matching U8/U16 allocations are reused in place.
The native PNG image contract, palette indices and decoder samples retain their
existing meaning. Source color conversion, optional indexing/merging and alpha
reduction are caller choices. This closes the explicit reference-depth input
gap identified in Third Edition 4.2, 4.4.5, 12.4 and 15.3.2; it does not establish
complete encoder conformance.

`tests/reference_samples.rs` independently models scaling with a doubled u64
rational expression. It checks all 69,904 channel-depth declarations, every
uniform source value (1,310,700 wide-container sample checks plus narrow inputs),
and every lower-precision value when a different channel forces 16-bit storage.
Invalid dimensions, counts, depths and values fail; indexed inputs retain their
separate native constructor. Both writers produce 3,456 static and 1,152 APNG
files across plain/Adam7 and Stored/Fast modes; static files also cover every
filter. Native slice and fragmented readers retain exact samples, and independent
Rust png verifies stored values and sBIT, including APNG frame rectangles and
both default-image roles. The rebuilt C libpng oracle checks 576 stored-sample
files and separately uses `png_set_shift` to recover all original samples from
the same 576 files.

The full release suite passes 125 tests, including all thirteen optional checks
and the new API doctest. Strict main/fuzz all-target Clippy and scoped rustfmt
pass, along with the 65,652-case strict-window C audit and 19,645-case compatibility
Huffman audit. A retained independent offline copy passes 112 normal tests with
thirteen optional checks ignored. All 21 native source hashes match the isolated
copy; its only production dependency is `deadlib-image`. Commands, source/oracle
hashes, counts and logs are retained in `reference-samples-validation.json`,
`reference-samples-validation.log` and `reference-samples-independent.log`.

The tenth fuzz target, `reference_samples`, independently checks constructor
validation, exact scaling, high-bit recovery, both writers and Rust png values.
Its fresh initial corpus has 640 structured files. Another 256 converted PNG
seeds extend each PNG corpus to 19,743 files / 4,314,004 bytes. Fresh Linux ASan
campaigns pass 20,000 reference-input, 30,000 structured encoder, 30,000 PNG-stream
and 30,000 buffered PNG executions. Fuzzing time / peak RSS are respectively
48 seconds / 404 MiB, 128 / 417, 44 / 654 and 6 / 464. All four instrumented
binaries define the ASan runtime entry and final source hashes match the initial
snapshot. `fuzz/reports/reference-samples-source.json` retains exact commands and
initial-corpus/source/binary hashes, with full target logs alongside it.
These 110,000 successful inputs bring the historical count to 860,000 across
ten targets. No comparative performance measurements were taken for this API
addition; existing encoder throughput/ratio gaps, remaining field/order audits,
broader high-precision APNG composition and native-only fuzz coverage remain open.

## Decoder ancillary ordering audit and scope clarification (2026-10-27)

The user clarified that decoding is the deliverable. Completion now concerns
native PNG/APNG decoding and rendering, conformance, comprehensive independent
validation, standalone reuse and competitive measured decoder performance.
Existing encoder utilities and historical results remain available; encoder
feature completeness, compression ratio and speed are outside the completion
requirements. The pending encoder-specific change was removed. All 21 production
source files retain their `1b70081ae` bytes.

`tests/support/ancillary_order.rs` builds literal chunks, stored-zlib data and
checksums independently of native encoding. A separate Table 7 model describes
18 standard non-animation ancillary types across all 15 native formats and
19 contexts including optional truecolor palettes. `tests/ancillary_order.rs`
checks 2,052 single placements and all 55,404 ordered pairs, including duplicate
singletons and repeated text keywords. The 21,637 valid cases in this matrix
retain exact native images, transparency, raw metadata, order and placement;
independent Rust png checks their exact stored samples. Other cases must fail
through buffered and one-/seven-byte readers. The model applies palette ordering
constraints only when PLTE exists, and allows either cICP/mDCV order.

Another 16,470 cases exercise every small-stream IDAT split, including empty
chunks, with known ancillary chunks before, between and after the two IDATs.
Distinct/case-sensitive sPLT names and all 720 permutations of six ancillary
types in each context add 14,022 cases. Together these are 87,948 wire cases,
each checked through three native parser paths. Direct event tests add 4,104
fragmented/whole-buffer runs, verify exact metadata order, and verify that
ordering errors remain terminal. Whole-file APIs reject chunks outside IHDR/IEND;
direct-event cases respect the documented prefix API boundary. The optional C
libpng test independently verifies 678 exact native-sample files, covering every
valid single placement and both cICP/mDCV orders. Reference pixels support valid
inputs; negative assertions come from the explicit ordering model rather than
assuming reference libraries reject every invalid ancillary chunk.

The normal release suite passes 116 tests, with fourteen optional checks ignored.
The focused ordering suite passes all five tests including its C oracle. Strict
main/fuzz all-target Clippy and scoped rustfmt pass. A retained standalone source
copy passes 116 normal tests offline, with fourteen optional checks ignored.
Its 21 production source hashes and copied test/example hashes match the working
source; its only production dependency is `deadlib-image`. Exact commands,
counts and source/oracle hashes are retained in `ancillary-order-validation.json`,
`ancillary-order-validation.log` and `ancillary-order-independent.log`.
No comparative timings were taken and no production codec dependency was added.

Fresh decoder corpora each gain 7,884 ordering fixtures, bringing them to 27,627
files / 5,758,804 bytes (maximum 63,435). Fresh Linux ASan campaigns pass 30,000
`png_stream` inputs in 30 fuzzing seconds / 523 MiB peak RSS and 30,000 `decode`
inputs in 6 seconds / 470 MiB. Both binaries define the ASan runtime entry and
their final source hashes match the initial snapshot. Exact commands and
initial-corpus/source/binary hashes are in
`fuzz/reports/ancillary-order-source.json`, with full per-target logs alongside it.
These 60,000 successful decoder inputs bring historical campaigns across all ten
targets to 920,000 executions, including earlier encoder campaigns. Remaining
decoder field audits, broader high-precision APNG composition, native-only fuzz
coverage and competitive decoder performance remain open.

## Native-precision APNG composition (2026-10-27)

`tests/compositor_precision.rs` checks every one of the 65,536 source alpha values
against fifteen destination-alpha boundaries, for 983,040 exact RGBA16 blends.
The independent u128 oracle performs reduced rational operations and rounds to
nearest; production uses a u64 weighted sum. Other checks build 240 literal APNGs
covering all fifteen native formats, plain/Adam7, both default-image roles and
four geometry/disposal patterns, with eight frames each. All six disposal/blend
states and nine adjacent disposal transitions occur. Buffered and one-/seven-byte
readers retain exact source samples, rectangles, controls, palettes and
transparency; independent Rust png checks every native frame sample. Explicit
expansion, full-canvas rendering, replay/reset and exact logical canvas plus
PREVIOUS-snapshot byte budgets are checked.

The checked-in corpus under `tests/fixtures/compositor-precision/` contains all
240 PNGs and their 1,920 full-canvas answers. An external C libpng decoder reads
each original frame as a standalone PNG, then Python `Fraction` composes it with
full-canvas snapshots and immediate disposal. The Rust oracle independently uses
the same mathematical operation, while production saves rectangles and defers
disposal until the next frame. Every displayed 16-bit sample matches, including
hidden colors, fractional alpha and low bits; no 8-bit display comparison is
substituted. Golden regeneration independently reproduces every uncompressed
answer. Manifests retain input/golden hashes and the C/Python tool hashes. PNGs
total 609,282 bytes; compressed goldens 103,127, representing 1,017,600 uncompressed
answer bytes. These extend the previous WPT answers to 2,121 independent canvases.
No production encoder constructs these input files.

The eleventh target, `compositor`, generates bounded native animations and
independent wire bytes, and compares expansion/rendering with the rational model.
It varies native formats, geometry, alpha, default roles, Adam7, disposal/blend,
recovered palette indices, snapshot budgets and reset. It calls no reference
codec. Its 480 initial structured files total 61,440 bytes. Each PNG corpus gains
the 240 precision animations, reaching 27,867 files / 6,368,086 bytes each.
Fresh Linux ASan campaigns pass 20,000 compositor, 30,000 PNG-stream and 30,000
buffered PNG inputs. Fuzzing time / peak RSS are respectively 68 seconds /
430 MiB, 33 / 500 and 7 / 466. All binaries define the ASan runtime entry and
final source hashes match the initial snapshot. Exact commands and
source/corpus/binary hashes are in `fuzz/reports/compositor-source.json`, with
full logs alongside it. These 80,000 successful executions bring historical
campaigns across all eleven targets to 1,000,000, including earlier encoder runs.

A separate LLVM replay of the compositor's final 1,040-file corpus maps only
native `src/composite.rs`: 97.25% lines, 94.29% regions and 89.13% instrumented
branch outcomes. The model, harness and reference libraries are excluded from
reported counts. This replay is excluded from fuzz execution totals. The
remaining gaps and exact scope are reviewed in `coverage-reports/README.md`;
whole-decoder native-only feedback coverage remains open.

The release and standalone offline suites each pass 120 normal tests, with
fifteen optional checks ignored. The focused precision suite also passes its
independent C/Fraction regeneration check (five tests total). Strict main/fuzz
all-target Clippy and scoped rustfmt pass. All 21 production files retain their
baseline bytes; the isolated source, tests/examples and all 480 fixture hashes
match. The standalone production dependency remains only `deadlib-image`.
`compositor-validation.json`, `compositor-validation.log` and
`compositor-independent.log` retain commands, hashes and counts. No comparative
timings were taken. Remaining decoder field audits, scoped corpus gaps,
whole-decoder feedback review and measured decoder performance remain open.

## Exact zlib prefix consumption and overflow (2026-10-27)

The buffered bit reader previously computed consumed bytes as
`ceil((pos * 8 - buffered_bits) / 8)`. Its intermediate multiplication can
overflow before the result is converted back to bytes. The retained fix uses
`pos - buffered_bits / 8`, which is equivalent for valid reader states without
overflowing the byte cursor. The regression compares 474 virtual cursor/bit-count
states against u128 arithmetic, including usize limits. It fails on the original
release code and passes with the fix. This models positions that can affect
32-bit streams above 512 MiB; no giant input allocation or actual 32-bit run is
claimed. `experiments/consumed-before.{patch,json,log}` retains the failing
command, exit 101 and reconstructible source.

`tests/deflate_prefix.rs` tests all 3,091 positive independent Huffman fixtures
with fourteen suffix lengths, exact prefix consumption, strict trailing-data
rejection, output limits, three streaming fragment shapes and 24,728 final
body/trailer truncations. Another 740 literal streams cover fixed/stored/fixed
transitions at all tested alignments, empty stored blocks, one-bit dynamic end
codes and zero/nonzero unused padding bits. Sixteen suffix lengths and four
patterns exercise buffered lookahead beyond the Adler trailer. Together the
two tests check 88,414 prefix/suffix/padding placements over 3,831 independent
streams, with 11,493 streaming fragment-shape checks. Inputs are constructed
independently of the native encoder.

The final source passes 123 normal release tests and 123 standalone offline
tests, with fifteen optional checks ignored in each. Separate C audits pass
19,645 default-policy Huffman, 65,652 strict-window header/history and 19,645
compatibility-policy Huffman cases. Strict main/fuzz all-target Clippy and
scoped rustfmt pass. The standalone copy also builds the decoder-only example
offline and checks all sixty independent static fixtures across six operations
(360 result rows). Its one-iteration timings are a correctness smoke, excluded
from comparative performance claims. Final production, test, harness, C binary
and copied-fixture hashes are retained in `prefix-validation.json` with
`prefix-validation.log` and `prefix-independent.log`.

Eight buffered inflate performance trials were rejected. Their patches all
reconstruct the recorded source from `39e258611`; twenty-one measurement groups
match those snapshots exactly, and both final interleaved summaries recompute
from the raw TSV. `BENCHMARKS.md` documents the negative results. Only consumed
byte arithmetic and its regression test change in production source. The
original 32-bit refill, symbol loop and streaming inflater remain selected.
Decoder performance is still incomplete; no speedup is claimed here.

The raw zlib fuzzer now independently validates each accepted exact prefix,
checks strict rejection of trailing bytes and confirms that appending a sentinel
cannot alter the decoded bytes or consumption count. Fresh Linux ASan campaigns
pass 10,000 `huffman`, 20,000 `zlib`, 20,000 `inflate_stream`, 30,000 `png_stream`
and 30,000 `decode` executions. Fuzzing seconds / peak RSS are respectively
132 / 410 MiB, 18 / 382, 16 / 398, 30 / 511 and 7 / 482. Each binary defines
the ASan runtime entry; final sources match the initial snapshots. Fresh raw
corpora have 649, 810 and 161 files; each PNG corpus has 27,867 files / 6,368,086
bytes. Exact commands, source/corpus/binary hashes and full logs are retained in
`fuzz/reports/prefix-source.json` and `prefix-*-linux.log`. These 110,000 successful
decoder executions bring historical campaigns to 1,110,000 across eleven targets,
including earlier encoder runs. No new encoder campaign or encoder change was
added. Whole-decoder native-only feedback review, remaining field audits and
competitive decoder performance remain open.

## Native-crate fuzz feedback isolation (2026-10-27)

`scripts/fuzz_native.py` uses a compiler wrapper to retain SanitizerCoverage only
in `deadlib_image_png` and `deadlib_image`. Harnesses and dependencies retain ASan
and other compiler options. This separates reference/helper code from mutation
feedback without removing memory checks. The compiled-artifact audit finds
16,550 counter bytes in PNG and 45 in shared primitives, with no counter or PC
table sections in fourteen dependency archives and four harness objects. Compiler
invocation records independently confirm removed feedback flags. Native-crate
std/alloc monomorphizations can still contribute feedback; this is a crate
boundary, not source-only feedback.

Fresh campaigns pass 50,000 `png_stream` and 20,000 `inflate_stream` executions,
in 71 / 12 fuzzing seconds and 524 / 398 MiB peak RSS. The first corpus starts
with 27,867 files / 6,368,086 bytes; the raw corpus has 939 files / 716,066 bytes,
including existing zlib fixtures no larger than 64 KiB. Both binaries define
the ASan runtime entry. These 70,000 successful decoder executions bring
historical campaigns to 1,180,000. No encoder target or production source changes
are added. `fuzz/reports/native-feedback-source.json` retains commands, source,
corpus, binary and archive hashes, compiler arguments and section audits.

The initial audit expected Cargo's older `release/deps` layout and failed after
both campaigns passed. The corrected audit locates artifacts under this
toolchain's per-package build directories and verifies the same retained build.
Fuzzing was not restarted or counted again. The exact original runner is retained
as `fuzz/reports/native-feedback-runner-as-run.py`; the manifest distinguishes
its source from the corrected auditor. The reproducible archive helper creates
the same initial corpus digests with stable ZIP metadata.

A separate fixed replay maps twenty native PNG source files plus shared
primitives. The generated constant registry has no runtime coverage mapping.
Reference libraries, harnesses and stdlib source are excluded by explicit file
operands. Whole-package totals include unused encoding utilities and are not
decoder-only percentages. Focused decoder-only file coverage is:

| File | Lines | Instrumented branch outcomes |
| --- | ---: | ---: |
| `src/stream/decode.rs` | 541/555 (97.48%) | 209/228 (91.67%) |
| `src/stream/read.rs` | 112/120 (93.33%) | 14/20 (70.00%) |
| `src/stream/rows.rs` | 169/171 (98.83%) | 47/50 (94.00%) |
| `src/zlib/inflate.rs` | 254/265 (95.85%) | 59/62 (95.16%) |

The 35 explicit zero-count annotations in these files identify resource-limit
branches, API accessors/error repetition, reader interruptions/invalid byte
counts, internal invariants and draining prefetched bytes before stored blocks.
They guide targeted corpus/API work and reachability review; they are not a
reason to alter normative behavior or a proof that every missed line is reachable.
`coverage-reports/native-feedback-*` retains source/corpus/profile/binary hashes,
commands, per-file summaries and the focused gap list. Replay is excluded from
fuzz totals. Remaining field audits, corpus gaps and competitive decoder
performance remain open.

## Decoder field, resource and reader boundaries (2026-10-27)

`tests/decoder_boundaries.rs` independently frames 120 PNG/APNG files: all fifteen
native formats, plain/Adam7, static images, both default-image roles and legal
fcTL-before-acTL ordering. Native samples, frame storage, metadata and resource
thresholds come from literal constructors and image geometry, without using the
production encoder. Across both animation policies, 4,680 zero/below/exact/above
budget probes produce 2,400 successes and 2,280 resource failures. Three reader
fragments and three push fragments plus buffered decoding make 32,760 API checks.
The budgets include cumulative native samples, per-image filtered inflation,
retained/expanded metadata, all chunks and animation frames. Resource errors
remain fatal under recovery.

IHDR checks enumerate 960 dimension pairs across native formats, all 65,536
color/depth byte pairs, and 11,520 compression/filter/interlace bytes. Each is
checked through buffered/reader/push parsing under both animation policies,
totalling 468,096 API checks. Legal header declarations encounter a tiny output
budget before large allocation; invalid fields fail as invalid data. These are
header-field tests, not claims that each mutated file has matching image data.
The [Third Edition integer and IHDR rules](https://www.w3.org/TR/2025/REC-png-3-20250624/)
define the tested format classes; other chunk-field audits remain separate.

Another 80,760 checks inject interruptions, short reads, IO failures and invalid
returned byte counts at chunk boundaries and the final EOF probe. The new
`decoder_limits` target varies all resource budgets, animation/palette policy,
CRC repair, fragmentation and reader failures using a separate 20-byte control
header. Its 6,376 structured seeds include two 1x4096 highly compressed files,
plain and Adam7. The harness compares buffered/reader/push acceptance and native
samples, checks fused errors, counters and bounded image-pipeline capacity.

The tall files exposed an overly strict input-length-only iteration guard in the
new test harness. The decoders already accepted them. The corrected guard also
accounts for the output budget because rows can be emitted without consuming
more compressed input. The failing test (exit 101), exact pre-fix helper and
fixture hashes are retained in `fuzz/reports/boundaries-harness-before.*`. The
corrected contract passes all 6,376 seeds. This is a harness correction, not a
production decoder fix.

The 740 independent mixed/one-bit DEFLATE fixtures are now shared by tests and
seed generation. All match flate2; existing 88,414 suffix/padding checks and
24,728 truncations remain. Valid, suffixed and truncated forms add 2,220 seeds
to each raw corpus. The native raw-stream fuzzer now varies four input fragment
shapes and checks input/output counters, memory and terminal errors.

An initial isolated-feedback ASan stage passes 30,000 resource-target and 20,000
raw-stream executions (9 / 16 fuzzing seconds, 447 / 424 MiB peak RSS). After the
tall-image harness correction, another 30,000 resource executions pass in
13 seconds / 445 MiB. Both stages' exact sources are retained; the first helper
snapshots are explicitly distinguished from current source. Every native-only
artifact audit passes: fourteen dependency archives and the selected harness
objects have no feedback counters or PC tables. All binaries define ASan's
runtime entry. These 80,000 successful development/final executions bring the
historical total to 1,260,000. Expected failing regression tests and fixed
coverage replay are excluded. No encoder target runs in this continuation.

The final release and standalone offline suites each pass 127 tests, with fifteen
optional checks ignored. Strict main/fuzz Clippy and scoped rustfmt pass. All
21 production files retain `41366d843`/`fae2e5bd2` bytes. Commands and source hashes
are in `boundaries-validation.json`/`.log` and `boundaries-independent.log`.

A fixed union replay combines the earlier broad PNG corpus with the corrected
resource corpus and new raw-stream corpus. Explicit source operands map twenty
PNG files and shared primitives; package totals still include untouched encoding
utilities and are not decoder-only percentages. The focused four-file list drops
from 35 to twelve zero-count annotations. `stream/decode.rs` reaches 99.82% lines,
`stream/read.rs` 95.00%, `stream/rows.rs` 99.42% and `zlib/inflate.rs` 98.49%.
The source/profile/corpus hashes and reviewed remainder are in
`coverage-reports/boundaries-*`. Full decoder field auditing and competitive
performance remain open.

## Buffered checksum accumulation (2026-10-27)

`src/checksum.rs` adds sixteen independent Adler-32 lanes for buffered inflation.
Short blocks and tails remain scalar. Lane reductions preserve the existing
5,552-byte accumulator bound, including maximum valid starting state; independent
u64 modular arithmetic checks varied bytes, all short tail lengths, fragmentation
and block/modulus edges. CRC logic is unchanged. There are no new heap allocations,
unsafe blocks, CPU intrinsics or production dependencies.

The shared-update trial is rejected because it slows photo reader decoding by
3.64% relative to Rust png. The selected buffered-only method leaves incremental
`Adler32::update` exactly as in `ded016feb`. Four-position interleaved runs favor
buffered PNG decode by 1.91% on 24 photos and 5.04% on 60 independent format files
relative to Rust png; buffered inflation improves 3.26% / 26.53% relative to
flate2. Streaming reference-relative controls are 1–2% lower. The shared host and
code-generation effects remain uncontrolled, and no streaming improvement is
claimed. Source, binary, corpus, size and capacity invariants are verified for all
fourteen development/selected benchmark groups. `BENCHMARKS.md` retains the exact
selection, rates, limits and reproduction commands.

Linux machine-code inspection confirms packed widening and eight packed u32
addition instructions in buffered Adler-32. The post-timing cycle profile assigns
2.09% to that checksum and 61.58% to buffered inflation; Huffman decoding remains
the largest identified inner operation. Sampling shares are diagnostic, not
isolated timings. Raw profiles remain in the recorded temporary location.

Both release and independently copied offline suites pass 128 tests, with fifteen
optional checks ignored in each default run. Three explicit default/strict-window/
compatibility C oracle audits, strict main/fuzz Clippy and scoped rustfmt pass.
All 875 copied source/fixture/example/benchmark files match the verified worktree.
`adler-validation.*` and `adler-independent.log` retain commands and source hashes.

Fresh ASan campaigns pass 50,000 PNG-stream, 20,000 raw-stream and 30,000 resource
inputs, in 68 / 16 / 13 fuzzing seconds and 542 / 434 / 484 MiB peak RSS. Native
PNG/shared archives contain feedback counters; fourteen dependency archives and
six harness objects contain none. All three binaries define ASan's runtime entry.
These 100,000 successful executions bring historical campaigns to 1,360,000.
No encoder target runs. `fuzz/reports/adler-*` retains exact sources, binaries,
initial corpus digests, compiler arguments and object audits.

Both experiment patches reconstruct their recorded sources from `ded016feb`.
`experiments/adler-selection.json` links the selected source and complete evidence.
Only the checksum source changes in production; the other twenty PNG files and
shared primitives are unchanged. Decoder field audits, remaining corpus gaps and
competitive decoder performance remain open.

## Scalar chunk field audit (2026-10-27)

`tests/scalar_fields.rs` exercises 207,348 independently framed one-pixel files
across fifteen native formats and both interlace flags. Both animation policies,
buffered decoding, one-/seven-byte readers and one-/31-byte push input produce
2,073,480 API/policy checks. The independent framing uses literal fields, bitwise
CRC and stored zlib; it calls no production encoder. Native samples, raw and typed
metadata, palettes, transparency, explicit RGBA16 expansion and fused errors are
checked against literal known answers.

| Field | Accepted cases | Invalid cases |
| --- | ---: | ---: |
| PLTE | 2,604 | 20,556 |
| bKGD | 20,050 | 3,706 |
| cHRM | 1,950 | 1,530 |
| gAMA | 240 | 300 |
| hIST | 32 | 5,392 |
| pHYs | 570 | 8,100 |
| sBIT | 716 | 17,394 |
| sRGB | 150 | 7,680 |
| tIME | 71,206 | 33,060 |
| tRNS | 11,858 | 254 |
| Total | 109,376 | 97,972 |

The matrix includes exact-layout length neighborhoods; all PLTE lengths 0–771;
palette/depth/alpha length relationships; all sBIT channel bytes, sRGB intents,
pHYs units and timestamp field bytes; all 65,536 years; individual 31-bit integer
edges; and background/transparency word-byte positions and unused-bit masking.
It is not every 32-bit Cartesian product or a substitute for larger-image,
APNG-control, text/profile or calendar-interpretation evidence.

The new test-only C `--scalar-metadata` mode exports integer getters without
sample/color transforms. Its 440-case run has 408 exact matches and 32 hIST drops.
The pinned libpng source marks hIST as preceding PLTE; the specification requires
PLTE first. The test checks that precise drop and warning separately, and will
accept exact hIST values from a future fixed reference. The retained wire,
source/binary hashes and diagnostic are in `experiments/scalar-hist-reference.json`;
the fixture was replayed against the same binary during finalization. No native
behavior was changed to reproduce the reference drop.

Strict main/fuzz Clippy and scoped formatting pass. The normal release and
standalone offline suites each pass 129 tests, with sixteen optional checks
ignored by default. Explicit scalar getter, existing C native-sample and zlib
batch checks pass. All compiled sources/test inputs and 876 copied files match;
the oracle README was updated after the copy and is identified as documentation
in `scalar-fields-validation.json`. Commands and logs are retained alongside it.

The seed generator adds 6,494 representative files. Focused export plus the
immutable earlier PNG ZIP produces 34,361 files / 7,204,344 bytes. Separate full
generation produces a byte-for-byte identical ZIP. Source, generator binary,
archive, command and log hashes are in `fuzz/reports/scalar-fields-*`.

Fresh native-feedback ASan passes 50,000 PNG-stream executions in 70 fuzzing
seconds / 528 MiB peak RSS. Object audits verify fourteen dependency archives
and two harness objects have no feedback counters or PC tables. Both native
archives retain feedback; the executable defines ASan's runtime entry. The
binary and all twenty-one production PNG files match the checksum milestone.
These runs bring historical sanitizer executions to 1,410,000. No encoder target
runs. `scalar-fields-audit.json` links exact counts, per-field totals and evidence.

Remaining animation/control-field audits, corpus gaps and competitive decoder
performance remain open.

## Literal values and Huffman lookup trials (2026-10-27)

`tests/primary_pairs.rs` adds 65,668 independently assembled dynamic streams:
all 65,536 ordered byte pairs and 132 fixtures covering short complete trees,
unequal code widths, zero/one final padding and canonical chains through the
15-bit maximum. Construction uses the test-only bit writer, canonical sorting
and scalar checksum, without the production encoder or table builder.

Flate2 and C zlib return exact two-byte samples for every stream. Native
buffered decoding checks valid EOF and seven-byte suffix consumption, output
limits zero/one and four trailer truncations. A separate native inflater uses
one-byte input and output fragments. This is an exhaustive two-byte value
matrix, not all symbol permutations or all DEFLATE block sequences.

Both buffered paired-literal optimizations are rejected after repeated
reference-relative timings. The smaller table lowers photo PNG throughput by
4.71% relative to Rust png; the wider table by 1.99%. All-format results also
fail to establish a benefit. `BENCHMARKS.md` records rates and control drift;
`experiments/primary-pairs-selection.json` links exact source reconstructions
and timing invariants. All twenty-one production files remain unchanged from
`23a46c8ed`, including the existing buffered Adler optimization.

Strict main/fuzz Clippy and scoped rustfmt pass. Normal release and independently
copied offline suites each pass 130 tests, with seventeen optional checks ignored
by default. The explicit C audit matches all 65,668 streams. All compiled sources,
test inputs and 878 copied files match; the only copy difference removes a final
empty line from the oracle README. `primary-pairs-validation.json` records that
documentation normalization separately. `primary-pairs-audit.json` links the
exact counts and finalized evidence.

The seed generator adds 644 representative streams to each raw decoder target.
Focused export merged with the earlier immutable seed ZIP is byte-for-byte
identical to fresh full generation: 6,667 files / 1,254,614 bytes. The archive,
generator binary, source and reproduction command hashes are retained in
`fuzz/reports/primary-pairs-seed-build.json` and `primary-pairs-seeds.json`.

A fresh ASan campaign passes 20,000 raw-stream decoder executions in thirteen
fuzzing seconds / 465 MiB peak RSS. Native image archives retain feedback;
fourteen dependency archives and two harness objects have no feedback counters
or PC tables. The decoder binary matches the prior checksum milestone and
defines ASan's runtime entry. `fuzz/reports/primary-pairs-source.json` and the
raw log retain the audit. Historical successful campaigns total 1,430,000,
including earlier encoder campaigns; no encoder target runs in this milestone.

Decoder field/control audits, remaining corpus gaps and competitive decoder
performance remain open.

## APNG control-field audit (2026-10-27)

`tests/animation_fields.rs` checks 80,460 independently framed files across all
fifteen native formats, both interlace layouts and both default-image roles.
Both animation policies run through buffered decoding, one-/seven-byte readers
and one-/31-byte push input, producing 804,600 API/policy checks. There are
5,280 valid files, 74,700 animation faults and 480 fatal resource cases.

The matrix covers acTL lengths 0-12; fcTL lengths 0-30 in both frame positions;
fdAT prefix lengths 0-3; individual zero, canvas, byte/word, 31-bit and high-bit
integer edges; declared/actual frame counts and the frame limit boundary;
plays; sequences; all 256 disposal/blend bytes in both frames; and one/two/seven
legal empty fdAT chunks with contiguous sequence numbers. Valid rectangle
changes carry independently generated samples and stored zlib, so geometry
checks do not depend on an unrelated frame-data-length error. The small canvas
is 3 by 2; larger positive geometries and native-precision composition retain
their existing evidence. This is not every 32-bit Cartesian product.

A further 131,072 RGB8/nondefault-first-frame files check every unsigned value
of both delay words through buffered decoding and fragmented push input.
Exact raw fields, denominator-zero timing and native samples are verified.
Together the suites cover 211,532 files and 1,066,744 API/policy checks. Direct
events check native sample positions, frame controls, image ends, complete input
consumption and fused errors. Recovery emits one diagnostic, discards subsequent
animation events, and completes the static image.

The explicit Rust png differential audit matches raw controls and native
samples for all 136,352 valid files. Invalid animation outcomes and fatal resource
limits are independent known answers. The C oracle is static-only and provides
no APNG-control getters. `animation-fields-validation.*` and
`animation-fields-audit.json` retain exact counts, commands and source evidence.

Strict main/fuzz Clippy and scoped rustfmt pass. The full release suite passes
132 tests, with eighteen optional checks ignored. A fresh complete standalone
copy passes both new APNG tests offline, with its optional Rust comparison
ignored there. All 881 copied source, fixture, example and benchmark files match.
The unchanged preexisting test inputs retain their linked full offline result
of 130 passed / seventeen ignored; this milestone reruns them in release mode.
`verify_independent.ps1 -Test animation_fields` selects the focused standalone
target; its default still runs the full package suite and dependency tree.

The generator adds 10,740 representative files per PNG decoder target.
Focused export plus the immutable scalar-field ZIP exactly equals full fresh
generation: 45,101 files / 10,776,415 bytes. The seed/archive/source/binary hashes
and reproduction commands are in `fuzz/reports/animation-fields-*`.

Fresh native-feedback ASan passes 50,000 PNG-stream executions in 45 fuzzing
seconds / 525 MiB peak RSS. Both native archives retain feedback; fourteen
dependency archives and two harness objects have no feedback counters or PC
tables. The binary defines ASan's runtime entry and matches the prior checksum
milestone. The exact logs, source/binary hashes and object audit are retained.
Historical successful campaigns total 1,480,000, including earlier encoder runs;
only the PNG-stream decoder target runs in this milestone.

The initialized output-slice performance trial is rejected after repeated
reference-relative comparisons. `BENCHMARKS.md` records the trial and control
drift; its source checkpoint and reconstruction are retained. All twenty-one
production PNG files remain unchanged from `861a15e00`. Remaining corpus gaps,
extension scope checks and competitive decoder performance stay open.

## Streaming literals and language extension scope (2026-10-27)

`src/zlib/inflate.rs` keeps literal runs inside the symbol state and directly
updates the existing history ring. The pending Literal state remains for a
value decoded with no output room. Counters, limits, checksum batching, bit
refill, table construction and the fixed allocation retain their existing rules.
No unsafe code or production dependencies are introduced. The other twenty
production PNG files and shared primitives retain their baseline bytes.

Four-position comparisons favor PNG reader decode by 5.81% on photos and 4.74%
on the independent format corpus relative to Rust png. Streaming inflation
improves 8.29% / 11.53% relative to flate2. Buffered controls and shared-host
limits are documented in `BENCHMARKS.md`. The exact selected source, reconstructed
patch and timing invariants are in `experiments/stream-literals-selection.json`.

`tests/stream_literals.rs` independently builds 172 fixed/dynamic streams with
literal prefixes around byte/output/history boundaries through 65,537 bytes,
then overlapping and 32 KiB-distance references. Expected samples are expanded
one byte at a time, independently of native history/copy code. Alternating empty
output calls and 1/1, 7/31, whole/257 input/output shapes check suspension,
working memory, untouched output tails, counters, fused errors, suffix consumption,
limits and four trailer truncations. Flate2 and the explicit C zlib batch agree
on all 172 exact raw outputs.

The iTXt extension scope audit follows PNG 11.3.3.4 and RFC 5646 2.2.6(4)/2.2.9.
The RFC distinguishes generic well-formedness/registration from validity for a
specific extension, and does not require processors to validate extension
contents. The codec preserves those contents without interpreting locale or
transformation settings; it does not certify extension-specific validity.
Normative references are [PNG Third Edition](https://www.w3.org/TR/2025/REC-png-3-20250624/#11iTXt)
and [RFC 5646](https://www.rfc-editor.org/rfc/rfc5646#section-2.2.6).

`tests/language_extensions.rs` checks every singleton byte, both-case ordered
pairs of all 35 nonprivate singleton characters, extension/private payload
lengths and literal examples. Its 2,758 tags comprise 48 typed, 2,434 unknown-
registration and 276 malformed cases. Both compression flags and three positions
produce 16,548 PNG files and 165,480 API/policy checks through buffered decoding,
one-/seven-byte readers and one-/31-byte push input under both animation policies.
All 14,892 non-malformed files match Rust png's raw language/text fields and native
pixels; unknown registration retains the complete original chunk as raw metadata.
Malformed generic syntax and duplicate singletons use independent known errors.
No encoder feature work is part of this audit.

Strict main/fuzz Clippy and scoped rustfmt pass. The full release suite passes
134 tests, with nineteen optional checks ignored. The new streaming target also
passes in a fresh complete offline package copy, with its optional C check
ignored there. All 885 copied sources, fixtures, examples and benchmark files
match. The normal dependency tree contains only the shared image package.
Commands, exact sources and C batch identities are in `stream-literals-validation.*`;
`stream-literals-audit.json` links the finalized evidence.

The generator adds 3,066 representative PNGs and 130 raw literal streams per
target. `fuzz/reports/stream-literals-*` retain full/focused generation, immutable
input archives, exact source and binary identities, corpus digests and fresh
native-feedback sanitizer campaigns. This milestone's temporary-storage
relocation is documented in `fuzz/reports/generated-seeds-storage-20261027.json`.

Full fresh generation matches the focused/merged ZIP byte for byte after inode
recovery. Initial PNG/raw corpora contain 48,167 / 6,927 files and 11,230,779 /
4,009,252 bytes. Fresh ASan passes 50,000 PNG-stream executions in 47 fuzzing
seconds / 527 MiB peak RSS and 20,000 raw-stream executions in 18 seconds /
492 MiB. Both binaries define ASan's runtime entry. Feedback remains in the two
native archives (16,710 PNG / 45 shared counter bytes); fourteen dependency
archives and four harness objects have no feedback counters or PC tables.
These 70,000 successful executions bring historical campaigns to 1,550,000,
including older encoder campaigns. No encoder target runs here.

The post-validation reader profile in `profiling/reader-stream-literals.*` has
no lost samples. Requested 499 Hz sampling is capped initially at 250 Hz by the
host; the warning is retained. Direct inflater-loop instructions account for
28.15% of samples and Huffman decoding 16.43%, with further refill/checksum
costs. These are diagnostic shares rather than isolated timings or a speedup
measurement. Comparative benchmarks finish before final validation and profiling.
Remaining corpus gaps and competitive decoder performance stay open.

## Streaming match-header dispatch fusion (2026-10-27)

`src/zlib/inflate.rs` decodes complete match headers inside the symbol loop.
Each incomplete field retains its previous resumable state; completed length/
distance fields avoid three outer dispatches. Shared copy validation preserves
window/history checks before output limits. The copy routine, literal loop,
checksums, refill, tables, struct layout and fixed history allocation retain
their existing behavior. The other twenty PNG production files, all test/helper/
tool inputs and shared image primitives match `79ccce5be`.

Four-position measurements favor photo reader decoding by 6.88% relative to
Rust png and photo streaming inflation by 8.31% relative to flate2. Buffered
photo controls are -0.35% / +0.86%. Format measurements are inconclusive because
unchanged controls improve more than changed paths. `BENCHMARKS.md` records
all rates, controls and shared-host limits. The reconstructed source and eight
verified timing groups are in `experiments/stream-matches-selection.json`.

Existing Huffman fixtures cover every legal match length/distance at all eight
bit offsets, fixed/dynamic trees and both final-padding values, through multiple
fragment shapes. Literal/history, prefix, header/window, PNG/APNG reader and
resource suites remain in the full regression run. The explicit C zlib batch
checks 19,645 fixtures: 3,091 successful outputs and 16,554 error records, including
68 documented published-HDIST-range definitions rejected by default zlib policy.
No new fixtures mirror the changed state dispatch.

Strict main/fuzz Clippy and scoped rustfmt pass. The complete release suite
passes 134 tests, with nineteen optional checks ignored. A fresh complete
offline copy passes the six Huffman tests, with its C check ignored there;
the C batch is run explicitly from the verified working tree. All 885 copied
sources, fixtures, examples and benchmark files match. The production dependency
tree contains only the shared image package.

Fresh sanitizer campaigns use the prior immutable ZIP with fresh mutable
corpus copies: 48,167 PNG inputs / 11,230,779 bytes and 6,927 raw inputs /
4,009,252 bytes. `fuzz/reports/stream-matches-seeds.json` links exact producer
provenance and unchanged test inputs. `stream-matches-validation.*` and
`stream-matches-audit.json` retain the current source and validation evidence.

Fresh ASan passes 50,000 PNG-stream executions in 43 fuzzing seconds / 511 MiB
peak RSS and 20,000 raw-stream executions in 20 seconds / 426 MiB. Native PNG/
shared archives retain 16,793 / 45 feedback-counter bytes; fourteen dependency
archives and four harness objects have no counters or PC tables. Both decoder
binaries define ASan's runtime entry. Historical successful campaigns reach
1,620,000, including earlier encoder campaigns; both current targets are decoders.

The [published PNG errata](https://www.w3.org/2025/06/REC-PNG-20250624-errata)
still contains the mDCV example correction already
covered by `tests/hdr.rs`; this review identifies no additional format change.
Remaining corpus gaps, the final requirements audit and competitive decoder
performance remain open.

## Streaming complete-match copies and fuzz count recovery (2026-10-27)

Only `src/zlib/inflate.rs` changes in production from `788659fb4`. Complete
matches copy directly inside the symbol loop; partial-output matches retain
the existing Copy state. The common inlined copy helper retains ring wrapping,
overlap expansion, history updates and validation order. State layout,
allocations, checksum batching, Huffman tables and bit refill are unchanged.
Existing independently assembled fixtures cover every legal length/distance,
eight bit offsets, fixed/dynamic trees, final padding and fragmented input/output.
No new fixtures duplicate the changed dispatch.

Repeated photo timing supports +6.31% reader decoding relative to Rust png and
+9.76% streaming inflation relative to flate2. Unchanged buffered controls move
-0.02% / +1.46%. Format measurements remain small and mixed; no independent
format speedup is claimed. `BENCHMARKS.md` retains absolute/reference-relative
rates, host limitations and the baseline source-guard recovery.
`experiments/stream-copy-selection.json` reconstructs the exact candidate and
verifies all eight timing groups and both combined summaries.

Strict main/fuzz Clippy and scoped rustfmt pass. The full release suite passes
134 tests, with nineteen optional checks ignored. The explicit C zlib Huffman
batch checks 19,645 fixtures: 3,091 successful outputs and 16,554 error records,
including 68 published-HDIST-range definitions rejected by default C policy.
A fresh complete offline source copy passes six Huffman tests, with its external
C check ignored there. All 885 copied sources, fixtures, examples and benchmark
files match. The production dependency tree contains only shared image primitives.
All recorded regression, oracle, seed-generator and harness inputs retain their
baseline bytes; the separately documented fuzz-runner correction is below.

Fresh mutable corpora reuse the verified immutable ZIP: 48,167 PNG seeds and
6,927 raw seeds. ASan succeeds for 50,852 PNG-stream executions in 53 fuzzing
seconds / 557 MiB peak RSS and 20,000 raw-stream executions in 36 seconds /
443 MiB. Native PNG/shared archives retain 16,768 / 45 feedback-counter bytes;
fourteen dependency archives and four harness objects contain no feedback
counters or PC tables. Both binaries define the ASan runtime entry. This adds
70,852 successful decoder executions, bringing historical all-target campaigns
to 1,690,852; historical totals include earlier encoder campaigns.

The PNG process exited zero with 50,852 completed executions against a 50,000
request. The runner's exact-count assertion then failed. `scripts/fuzz_native.py`
now accepts completed counts at or above the requested threshold, verifies the
completion/statistics counts agree and records requested and observed values
separately. Recovery reuses that completed PNG campaign and runs only the missing
raw-stream campaign. The original helper and pre-recovery manifest remain in
`fuzz/reports/stream-copy-fuzz-runner-before.py` and
`fuzz/reports/stream-copy-before-count-recovery.json`; the current source manifest
maps the archived helper to its as-run hash. The corrected helper passes Python
compilation, and recovery checks both observed logs and the original binary.

`stream-copy-validation.*`, `stream-copy-audit.json` and
`fuzz/reports/stream-copy-{source,seeds}.json` retain exact sources, commands,
corpus/binary/log hashes, independent-copy checks and feedback isolation audits.
Earlier source-coverage snapshots retain their original source bindings; no new
whole-decoder coverage percentage follows from these campaigns. Remaining
corpus gaps, the final requirements audit and competitive decoder performance
stay open. Encoder functionality, conformance and speed remain outside scope.

## Rejected Huffman lookahead split and corpus relocation (2026-10-27)

Two source-checkpointed candidates separate complete nine-bit primary lookups
from partial input. The second also forces only the primary decode function
inline. The first passes 29 library/Huffman/primary-pair/prefix/fragmented-stream
and PNG-reader tests, with three external C checks ignored. Its exact candidate
source and normalized log hashes are in `huffman-lookahead-focused.{json,log}`.
The second differs only in its inline annotation; its PNG, reader and raw outputs
agree with references on all 84 benchmark files. No full second-candidate suite
or new sanitizer executions are claimed.

`BENCHMARKS.md` records the ten timing groups and four final comparisons. The
first split regresses buffered photo decode/inflation; forced primary inlining
recovers much of that loss but still produces mixed reference-relative results.
Both candidates are rejected. All twenty-one production files, shared primitives
and recorded regression inputs retain the selected `76d754d2a` bytes. The prior
134-test release suite and 70,852-execution decoder ASan validation are reused
with that exact-source check; they are not counted again. The reconstruction,
timing/source/binary checks and validation links are retained in
`experiments/huffman-lookahead-selection.json`.

The /tmp tmpfs had only 27,539 free inodes. The completed match-header campaign's
mutable corpus is relocated from
`/tmp/deadlib-image-stream-matches-asan-20261027-a/corpus` into
`target/png-fuzz-corpus-stream-matches-20261027.zip`. All 55,463 files / 15,741,565
source bytes are verified against the original before removing that temporary
corpus, and the retained archive is reopened to reproduce the same ordered
UTF-8 filename/NUL/content digest. Binaries, manifests, logs, compiler invocations
and object audits remain at their original locations. The archive and previous
campaign evidence hashes, exact source path and post-verification removal are
recorded in `fuzz/reports/huffman-lookahead-corpus-storage.json`. The immutable
seed ZIP used by later campaigns is unchanged.

This milestone adds no production change and no fuzz executions. Remaining
corpus gaps, the final completion audit and competitive decoder performance
stay open. Encoder requirements remain excluded.

## Buffered profile and marker-reservoir experiments (2026-10-27)

Two fresh selected-source cycle profiles cover 24 photos and eighty corpus
passes, with setup/native/reference validation included in the recording.
Buffered decode captures 5,936 samples and raw inflation 4,016; neither loses
samples. The kernel caps the requested 499 Hz at 250 Hz. All 23 PNG/shared/
entry-point source hashes, corpus bytes, Linux executable and raw profile hashes
are verified. `profiling/{decode,inflate}-b19a1d1b3.json` retains the exact binding.
The failed `perf annotate` attempt is recorded separately from the successful
sampled-address/static-disassembly mapping. Attribution remains diagnostic and
does not prove kernel latency or a portable speedup.

Three reconstructible decoder-only source variants test marker-bit availability
checking, one-shift arithmetic refill and forced primary inlining. The two bit
algorithms each pass 29 focused tests with three optional external checks ignored.
The arithmetic variant also passes strict main all-target Clippy. The third
changes only its inline annotation; all 84 benchmark corpus outputs agree with
references. The experimental private consumed-byte unit preserves the original
virtual-count 0-64 arithmetic oracle while checking representable marker states
0-63 separately. Original production/unit bytes are restored after rejection.
No full candidate regression suite or fresh sanitizer campaign is claimed.

`BENCHMARKS.md` records the fourteen timing groups / six final summaries and
inlining evidence. All variants fail to establish a consistent decoder benefit.
`experiments/sentinel-selection.json` reconstructs every source hash, verifies
saved executables/manifests and recomputes the comparisons. All 21 production
files, shared primitives and recorded regression inputs match the selected
baseline; all 885 copied source/fixture/example/benchmark files match the prior
tested independent workspace. Existing full release and decoder ASan validation
are reused for these identical restored bytes, not repeated or counted again.
Remaining corpus gaps, the completion audit and competitive decoder performance
remain open. No encoder requirement is added.
