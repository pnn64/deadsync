# PNG Third Edition clause audit

This matrix tracks the implementation against the [24 June 2025 Recommendation](https://www.w3.org/TR/2025/REC-png-3-20250624/)
and its [published errata](https://www.w3.org/2025/06/REC-PNG-20250624-errata).
It is an audit in progress, not a certificate of complete conformance.
`Covered` identifies current code and automated evidence; it does not mean every
possible input was enumerated. `Open` identifies missing implementation or
insufficient evidence. `Application` identifies an explicit caller responsibility
of this native-sample codec, rather than an implicit image transformation.

The user clarified on 2026-10-27 that decoding is the deliverable. Completion
therefore covers native PNG/APNG decoding and rendering, standards behavior,
differential/fuzz validation, independent reuse and measured decoder performance.
Existing encoding utilities and their historical evidence remain documented;
encoder feature completeness, compression ratio and encoder speed are outside
the current completion requirements.

All test paths below are relative to the PNG package. `COVERAGE.md` records exact
campaign counts, external tool versions, isolated builds and remaining milestones.

## Representation, framing and compression

| Clauses | Implementation and evidence | Audit |
| --- | --- | --- |
| 1–3: scope and terminology | Two independent Cargo packages; native `Image`, `Samples`, color type and precision; production dependency tree in `independent-build.log` | Covered for package architecture; no application integration |
| 4.1–4.6, 6.1: static images and native representation | Five color types, all 15 legal depth combinations; native/reference image APIs; `tests/static_png.rs`, `tests/pngsuite.rs`, `tests/reference_samples.rs`, external C oracle | Covered for native samples and all 1–16-bit reference RGB/RGBA channel-depth combinations; arbitrary source-to-reference transformations remain outside the specification's interface scope (4.2) |
| 4.4.1–4.4.5, 6.2: alpha/indexing/merging/scaling | Native channels/indices remain exact; explicit full-range expansion and reference-depth conversion with sBIT | Covered for storage/scaling: 69,904 depth declarations, exhaustive uniform source values, both writers and independent C sBIT recovery. Optional indexing, RGB merging and alpha reductions are not implicitly applied |
| 4.7: additional information and color precedence | Raw placement plus typed metadata; supported color selection with bounded ICC validation; `tests/metadata.rs`, `tests/profiles.rs` | Covered for wire values and selection; color transforms are Application |
| 4.8, 5.1–5.3: signature and chunks | Literal signature, checked lengths, CRCs and core grammar in both parsers; `tests/chunks.rs` independently checks all 256 byte values in each name position, empty/unaligned payloads and 31-bit length boundaries | Covered for these framing equivalence classes; broader field/length mutations remain Open |
| 5.4: chunk property bits | `tests/chunks.rs`: all 16 property-bit combinations at three positions, both empty/binary data, and every noncanonical case variant of all 25 standard names | Covered for literal dispatch, all property bits and future reserved-bit handling; C reference differences are recorded in `chunks-validation.log` |
| 5.5: CRC | Native CRC-32 for chunks and fdAT sequence/data; known answers, bulk tails/splits, corrupt-stream tests | Covered |
| 5.6: ordering | All 720 core and 120 APNG permutations; `tests/ancillary_order.rs` checks all 18 non-animation ancillary types across all native formats, optional palettes, critical-boundary placements, ordered pairs, duplicates and 720 six-type permutations per context | Covered for the Table 7 representative matrix, every small-stream IDAT split and direct event error termination; arbitrary payload mutations remain separate field/fuzz evidence |
| 5.7–5.8: extensions and private fields | Public raw ancillary API enforces alphabetic names, ancillary/reserved bits; unknown critical chunks fail; encoder emits only supported IHDR methods | Covered for current extension API; no new public chunk definitions |
| 7.1: integers and byte order | Big-endian native u16 samples; checked dimensions/lengths; `tests/decoder_boundaries.rs`, `tests/scalar_fields.rs`, `tests/animation_fields.rs` | Covered for IHDR, scalar metadata and APNG zero/31-bit/high-bit boundaries, all timestamp years, every delay-word value and native word-byte positions; large integer Cartesian products are not enumerated |
| 7.2: scanlines | Native sub-byte packing/unpacking, per-row padding, 8/16-bit samples; 3,240 format/geometry/filter/interlace/compression cases | Covered; nonzero unused input bits are not image samples |
| 7.3, 9.1–9.4: filters | Native None/Sub/Up/Average/Paeth plus adaptive encoder selection; byte-wise wrapping arithmetic and pass resets | Covered by reference sample matrix and PNGSuite |
| 8.1: Adam7 | All seven pass origins/steps, empty passes, plain/stream writer and row-reader paths | Covered by geometry matrix, PNGSuite and APNG subframes |
| 10.1, RFC 1950: zlib | All 65,536 CMF/FLG pairs, advertised-window boundaries, Adler-32, limits and prefix consumption; `tests/zlib_headers.rs`, `tests/deflate_prefix.rs` | Covered by buffered/fragmented raw and PNG tests, all level hints, 92 independent window fixtures, C zlib default/strict builds and 88,414 prefix/suffix cases; consumed-byte arithmetic is checked at virtual usize boundaries without a large input allocation |
| 10.1, RFC 1951: DEFLATE decode | Stored/fixed/dynamic trees; 19,645 independently assembled Huffman fixtures, 1,525 complete code-length histograms, all repeat counts, reserved symbols, code widths 1–15 and legal length/distance values; `tests/huffman.rs`. `tests/primary_pairs.rs` adds every ordered two-byte value and 132 width/padding fixtures | Covered for these equivalence classes through buffered/fragmented raw and PNG paths, default/compatibility C zlib, 3,091 libdeflate positive checks and structured ASan fuzzing. The 65,668 additional literal streams match flate2/C zlib with exact native EOF/prefix consumption and limits; broader symbol permutations and targeted native fuzz coverage gaps remain Open |
| 10.2: image datastream | One zlib stream across arbitrary IDAT/fdAT splits; exact filtered/native output lengths; prefix tolerance for unused final data | Covered by one-byte payloads, truncations and streaming properties |
| 10.3: compressed metadata | Independent native zlib streams and cumulative raw/inflated metadata limits; metadata/text/profile tests and structured profile fuzzer | Covered; no preset dictionaries or concatenated metadata streams |

## Chunk requirements

| Clause / chunk | Implementation and evidence | Audit |
| --- | --- | --- |
| 11.2.1 IHDR | Required first, singleton, nonzero 31-bit dimensions, legal color/depth, method 0, interlace 0/1 | Covered by framing/format tests plus all 65,536 color/depth pairs, all 11,520 native-format/method bytes and 960 dimension pairs through three APIs and both animation policies, without giant allocations |
| 11.2.2 PLTE | Presence by color type, 1–256 RGB entries, depth capacity and placement; native index retention | Covered by grammar tests and all payload lengths 0–771 in thirty format/interlace contexts; index recovery is listed below |
| 11.2.3 IDAT | Consecutive chunks, arbitrary/empty payload boundaries, single concatenated zlib stream, ignored unused final compressed bytes | Covered by buffered/reader/row/encoder tests |
| 11.2.4 IEND | Empty, required last; whole-file APIs reject trailing bytes, push parser reports prefix completion | Covered by truncation/grammar/prefix tests |
| 11.3.1.1 tRNS | Color-specific lengths, PLTE dependency, missing palette alpha = opaque, unused bits masked, both bytes of 16-bit keys compared | Covered by palette/field/compositing tests; encoder zeroes unused bits |
| 11.3.2.1 cHRM | Exact fixed-point integers, size, 31-bit range, singleton/order and sRGB compatibility | Covered for wire values; physical color interpretation is Application |
| 11.3.2.2 gAMA | Exact nonzero fixed-point value, range, singleton/order and sRGB compatibility | Covered for wire values; gamma conversion is Application |
| 11.3.2.3 iCCP | Names/method, bounded native inflate, RGB/GRAY match, common ICC header/tag framing and version-aware v4.4 table checks | Covered by 960 profile wire cases, eight pinned independent profiles, C oracle and structured mutations; transform/tag semantics remain opaque |
| 11.3.2.4 sBIT | Exact channel count and nonzero depth bounded by native/palette precision | Covered by all 256 values at every channel position in all formats/interlace contexts, plus independent C getters and original significant-bit recovery |
| 11.3.2.5 sRGB | Four intents, singleton/order; accompanying gAMA/cHRM must use Table 17 values when encoding | Covered by all-format/intent and incompatible-fallback tests; decoder follows higher-precedence description |
| 11.3.2.6 cICP | H.273 (07/2024) primaries/transfer assignments, matrix 0, full-range flag 0/1, singleton/order and highest selection priority | Covered by all 131,072 primaries/transfer/range combinations and all matrix/flag bytes; reserved values retained as unknown when decoding and rejected when encoding |
| 11.3.2.7 mDCV | Exact 16/32-bit fixed-point fields, length/range, cICP dependency and placement; `tests/hdr.rs` | Covered by independent literal/byte/boundary fixtures, all formats, two writers and exact C/Rust fixed-point comparisons; physical display interpretation and tone mapping are Application |
| 11.3.2.8 cLLI | Exact MaxCLL/MaxFALL values, zero/31-bit boundaries, singleton and placement; `tests/hdr.rs` | Covered by the HDR field/order suite and exact C/Rust comparisons; content measurement and rendering policy are Application |
| 11.3.3.1 keywords | `tests/keywords.rs`: all 256 bytes in single/first/interior/last and 79-byte contexts, lengths 0–256 and every single/double-space position; seven named wire forms | Covered for these name equivalence classes by 19,026 independent files and exact C/Rust values; separate text-body rules are verified below |
| 11.3.3.2 tEXt | Strict encoder graphic Latin-1/LF validation; decoder raw non-null recovery; exact bytes, placement and long text | Covered by all 256 byte values, four contexts, three positions and both writers; old ESC regression fails against the isolated baseline and passes after correction |
| 11.3.3.3 zTXt | Method 0, bounded zlib, same strict body repertoire as tEXt with independent stored-stream fixtures and native Stored/Fast typed construction | Covered by the shared 6,144-file body matrix, C/Rust exact values, expansion/method tests and structured text-body fuzzing |
| 11.3.3.4 iTXt | Flags, ignored decoder method when uncompressed, UTF-8 BOM/replacement, raw retention and BCP47 grammar/registration | Covered by all 256 methods, UTF-8 answers, 9,902 IANA records/codes and 16,548 extension-tag files / 165,480 native checks. RFC 5646 2.2.6(4) does not require extension-specific validation; locale/transformation interpretation is Application, while generic syntax, singleton uniqueness and raw payload preservation are verified |
| 11.3.4.1 bKGD | Color-specific size, indexed bounds, unused-bit masking, encoder normalization and placement | Covered by all nonindexed formats and indexed metadata tests; compositing against suggested background is Application |
| 11.3.4.2 hIST | One frequency per PLTE entry, placement and nonzero frequencies for used encoded indices, including frames | Covered by independent payload-length/palette contexts and prior usage tests; pinned libpng drops 32 legal representative chunks, recorded separately; quantization remains Application |
| 11.3.4.3 pHYs | Two exact 31-bit values, known unit flag, singleton/order | Covered by integer boundaries, all 256 unit values, lengths and C integer getters; display aspect correction is Application |
| 11.3.4.4 sPLT | All 256 depth bytes, entry/remainder/count boundaries through 257 entries, full sample/frequency boundaries, all 15 image formats, case-sensitive unique names | Covered for these equivalence classes, with independent big-endian fields and 158 exact C/policy comparisons; palette generation/selection is Application |
| 11.3.4.5 eXIf | Both TIFF byte orders and first-directory framing; unsupported data retained as unknown; encoder rejects unsupported containers | Covered by endian/truncation/offset tests and exact C/Rust payload comparisons; remaining Exif tags/directories/thumbnails are opaque |
| 11.3.5.1 tIME | Exact fields, documented month/day/time ranges, leap second, singleton and legal placement | Covered by all 65,536 years, all byte values in five remaining fields across all formats/interlace contexts, and C getters; calendar/display/local-time policy is Application |
| 11.3.6.1 acTL | Before IDAT, singleton, nonzero frame count, plays/frame limits and actual count agreement; explicit recovery diagnostic | Covered by prior 3,000 fault cases/120 permutations and the 80,460-file control matrix, including declared/actual count, 31-bit plays and exact/over resource boundaries; fatal resource/core errors remain distinct |
| 11.3.6.2 fcTL | Contiguous sequence numbers, legal rectangles, default-image restrictions, timing, blend/disposal enums; first fcTL may precede acTL | Covered by all byte enum values and integer edges in both frame positions across 60 configurations, all 65,536 values of each delay word, 136,352 exact Rust png control/native-sample comparisons, plus 35 WPT/240 generated animations, 2,121 RGBA16 canvases and all disposal transitions |
| 11.3.6.3 fdAT | Sequence prefix, preceding frame control, arbitrary splits/empty frame-data fields, shared image format and independent frame zlib | Covered by native/reference APNG matrix, independent 20-frame fixture and streamed one-byte data; control audit adds prefix-length/sequence edges and one/two/seven empty chunks with contiguous renumbering |

`tests/scalar_fields.rs` supplements the scalar chunk rows above with 207,348
independently framed one-pixel files: 109,376 accepted wire cases and 97,972
invalid field cases. Both animation policies, buffered decoding, one-/seven-byte
readers and one-/31-byte push input produce 2,073,480 checks. The suite verifies
native rows, raw/typed metadata, palette/transparency values, explicit RGBA16
expansion and terminal errors. It samples integer edges individually rather than
claiming the full Cartesian product of 32-bit fields. Existing suites cover
larger images, APNG geometry, opaque profiles and variable-length text.

The external scalar getter run has 408 exact matches and 32 recorded hIST drops.
[PNG Table 7](https://www.w3.org/TR/2025/REC-png-3-20250624/) requires hIST after
PLTE. The [pinned libpng table](https://github.com/pnggroup/libpng/blob/2b978915d82377df13fcbb1fb56660195ded868a/pngrutil.c#L3077)
instead places it before PLTE, explaining the observed warning/drop. The exact
fixture, binary/source hashes and diagnostics are retained in
`experiments/scalar-hist-reference.json`. This reference difference does not
change native known answers.

## Encoders, decoders, viewers and editors

| Clauses | Implementation and evidence | Audit |
| --- | --- | --- |
| 12.1–12.2: color/gamma | Caller supplies stored samples and color metadata; integer metadata stays exact; precedence exposed | Application for selecting/converting color spaces; no implicit CMS |
| 12.3–12.4: alpha and source precision | `Png::from_reference_samples` accepts per-channel depths 1–16, scales to the smallest legal PNG depth, records sBIT and preserves high bits/alpha endpoints | Covered by exhaustive declaration/value checks, 3,456 static and 1,152 APNG writer files, 576 C native files and 576 independent C unshift recoveries; native `Image` remains the PNG-image representation |
| 12.5: suggested palettes | PLTE/sPLT/hIST serialization and usage checks; no automatic quantization | Covered for supplied metadata; palette generation is Application |
| 12.6–12.7: interlace/filter choice | Plain/Adam7 and all filters selectable; adaptive score shared by both writers | Covered; predictor exhaustively compared over all byte triplets, independent row/score matrix and filter/photo benchmarks retained; broader performance remains Open |
| 12.8: compression | Native stored/fixed-Huffman LZ77, bounded 16-candidate chains, per-block coding choice and cross-block streaming history | Historical encoding utility evidence; compression ratio and encoder performance are outside the clarified decoder completion requirements |
| 12.9: text | UTF-8 serialization, keyword byte/length classes, strict Latin-1 body encoding, native metadata compression and raw/typed APIs | Covered for these wire/encoder rules, including body NBSP and LF, empty/long text and unchanged non-null iTXt controls; line wrapping/display conventions are Application |
| 12.10: chunking/extensions | Chunk size control, bounded writer chunks, recognized ancillary placement, private-name/property validation | Covered; full ordering mutation matrix remains Open |
| 13.1: errors/recovery | Strict default plus `AnimationPolicy::RecoverDefault`, first-error event/diagnostic, discarded animation frames and verified native static image; palette/profile recovery | Covered for the APNG fault/CRC/sequence/data classes, all fragments/truncations tested, fatal core/resource/IO cases and diagnostic-free static reencoding; presentation remains Application |
| 13.2–13.3: checking/security | Mandatory CRCs/checksums, grammar/format checks, checked arithmetic, input/output/chunk/frame/metadata limits; twelve sanitizer targets | Covered for tested cases, including 4,680 independently calculated budget/policy probes and 80,760 reader fault checks. Native-crate feedback isolation and source replay are verified; remaining corpus/field gaps and longer campaigns remain Open |
| 13.4: privacy | Exact hidden color samples, Exif and text are retained; encoder does not claim redaction or metadata scrubbing | Application; native preservation is intentional |
| 13.5: unknown chunks | Independent property-bit/name-byte/case matrices through buffered, one-/seven-byte reader and direct event paths; literal raw names/data/positions and fused critical errors | Covered for these equivalence classes; reserved-bit ancillary acceptance follows 13.5 despite libpng 1.6.50 rejecting these names |
| 13.6: pixel dimensions | pHYs values available without changing pixel storage | Application for presentation scaling |
| 13.7: text presentation | Typed Unicode decode and raw bytes available | Covered for decoding; escaping controls, line breaks and UI display are Application |
| 13.8–13.9: decompression/filtering | Fully resumable zlib and row reconstruction with checksum-verified image boundaries | Covered by fragmentation, truncation, mixed-block and row-memory tests |
| 13.10: progressive display | Adam7 row pass/origin/step exposed; `ImageEnd` separates provisional rows from verified images | Covered for codec events; progressive filling/painting is Application |
| 13.11–13.12: truecolor/scaling | Native precision retained; explicit full-range RGBA16 expansion | Covered for native values and known expansion cases; display quantization is Application |
| 13.13–13.14: color/gamma rendering | Supported metadata selection is available; source sample space remains explicit | Application; no automatic ICC, HDR or gamma conversion |
| 13.15–13.17: backgrounds/alpha/palettes | bKGD/hIST/sPLT retained; explicit APNG SOURCE/OVER and disposal renderer | Covered by 983,040 exact rational alpha pairs, 2,121 WPT/generated libpng/Fraction RGBA16 canvases, all formats, rectangles, disposal transitions, budget boundaries, replay/reset and compositor fuzzing; additional color management remains Application |
| 14.1–14.3: editor copy/order rules | Both encoders tested for all 16 property combinations at three positions; unknown unsafe chunks omitted, repeated safe chunks retain binary payload/order/placement, known unsafe metadata follows its own rules | Covered for reencoding, including prewrite rejection of all nonalphabetic name bytes and reserved-bit names; arbitrary edit operations are not provided |
| 15.3.1: datastream conditions | Signature, core grammar, per-chunk fields/relationships, known order constraints and encoder validation | Partial until all open field/order audit items are resolved |
| 15.3.2: encoder conditions | Both native writers agree with independent readers; reference-depth frontend preserves original high bits and legal-depth native input remains strict | Historical utility evidence; encoder conformance is outside the current decoder completion scope |
| 15.3.3: decoder conditions | All standardized critical methods/formats, unknown property/case matrices, both transparency bytes and arbitrary data splitting | Partial: broader malformed equivalence classes and completion audit remain |
| 15.3.4: editor conditions | Native reference-image samples preserved; safe-to-copy handling implemented | Partial for reencoding API; no general editing UI or color-managed viewer is claimed |

## Completion evidence still required

The clarified decoder deliverable still requires competitive performance,
comprehensive differential and fuzz evidence, independent reuse and no DeadSync
integration. Package isolation and the production dependency boundary are
verified by retained offline builds. Decoder performance remains incomplete in
`BENCHMARKS.md`: the latest streaming-match-copy comparison measures buffered
photo decoding at 93.37 MiB/s against Rust png's 185.41 on the same shared host,
and reader decoding at 91.09 MiB/s. Local streaming gains do not establish
competitive end-to-end performance. Native-precision APNG render answers and scoped native compositor
corpus coverage are now retained. Native-crate feedback isolation and a
whole-package source replay are verified; package totals also contain untouched
encoding utilities and are not decoder-only percentages. Resolve remaining
decoder field audits, review the recorded native decoder gaps,
improve measured decoder performance, and repeat the requirement audit against
the resulting implementation and measurements before claiming completion.
