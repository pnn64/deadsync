# PNG Third Edition clause audit

This matrix tracks the implementation against the [24 June 2025 Recommendation](https://www.w3.org/TR/2025/REC-png-3-20250624/)
and its [published errata](https://www.w3.org/2025/06/REC-PNG-20250624-errata).
It is an audit in progress, not a certificate of complete conformance.
`Covered` identifies current code and automated evidence; it does not mean every
possible input was enumerated. `Open` identifies missing implementation or
insufficient evidence. `Application` identifies an explicit caller responsibility
of this native-sample codec, rather than an implicit image transformation.

All test paths below are relative to the PNG package. `COVERAGE.md` records exact
campaign counts, external tool versions, isolated builds and remaining milestones.

## Representation, framing and compression

| Clauses | Implementation and evidence | Audit |
| --- | --- | --- |
| 1–3: scope and terminology | Two independent Cargo packages; native `Image`, `Samples`, color type and precision; production dependency tree in `independent-build.log` | Covered for package architecture; no application integration |
| 4.1–4.6, 6.1: static images and native representation | Five color types, all 15 legal depth combinations; `tests/static_png.rs`, `tests/pngsuite.rs`, external C oracle | Covered for native samples; arbitrary non-PNG reference formats are outside `Image`'s current contract |
| 4.4.1–4.4.5, 6.2: alpha/indexing/merging/scaling | Native channels and indices remain exact; `to_rgba16` is explicit; `tests/compositing.rs`, `tests/palette.rs`, `tests/metadata_fields.rs` | Covered for PNG sample storage and explicit expansion; source-image quantization/scaling is caller-controlled |
| 4.7: additional information and color precedence | Raw placement plus typed metadata; supported color selection with bounded ICC validation; `tests/metadata.rs`, `tests/profiles.rs` | Covered for wire values and selection; color transforms are Application |
| 4.8, 5.1–5.3: signature and chunks | Literal signature, checked lengths, CRCs and core grammar in both parsers; `tests/chunks.rs` independently checks all 256 byte values in each name position, empty/unaligned payloads and 31-bit length boundaries | Covered for these framing equivalence classes; broader field/length mutations remain Open |
| 5.4: chunk property bits | `tests/chunks.rs`: all 16 property-bit combinations at three positions, both empty/binary data, and every noncanonical case variant of all 25 standard names | Covered for literal dispatch, all property bits and future reserved-bit handling; C reference differences are recorded in `chunks-validation.log` |
| 5.5: CRC | Native CRC-32 for chunks and fdAT sequence/data; known answers, bulk tails/splits, corrupt-stream tests | Covered |
| 5.6: ordering | All 720 permutations of IHDR/PLTE/tRNS/two distinct IDAT parts/IEND; all 16 unknown-name flags at every compressed-byte split before/between/after IDAT; shared ancillary validator | Covered for this core grammar and named metadata/APNG cases; broader ancillary/APNG ordering mutations remain Open |
| 5.7–5.8: extensions and private fields | Public raw ancillary API enforces alphabetic names, ancillary/reserved bits; unknown critical chunks fail; encoder emits only supported IHDR methods | Covered for current extension API; no new public chunk definitions |
| 7.1: integers and byte order | Big-endian native u16 samples; 31-bit PNG u32 fields and checked dimensions/lengths; format and malformed suites | Covered; full field-boundary matrix remains Open |
| 7.2: scanlines | Native sub-byte packing/unpacking, per-row padding, 8/16-bit samples; 3,240 format/geometry/filter/interlace/compression cases | Covered; nonzero unused input bits are not image samples |
| 7.3, 9.1–9.4: filters | Native None/Sub/Up/Average/Paeth plus adaptive encoder selection; byte-wise wrapping arithmetic and pass resets | Covered by reference sample matrix and PNGSuite |
| 8.1: Adam7 | All seven pass origins/steps, empty passes, plain/stream writer and row-reader paths | Covered by geometry matrix, PNGSuite and APNG subframes |
| 10.1, RFC 1950: zlib | All 65,536 CMF/FLG pairs, advertised-window boundaries, Adler-32, limits and prefix consumption; `tests/zlib_headers.rs` | Covered by buffered/fragmented raw and PNG tests, all level hints, 92 independent window fixtures and C zlib default/strict builds; broader malformed DEFLATE classes are listed below |
| 10.1, RFC 1951: DEFLATE decode | Stored/fixed/dynamic trees; 19,645 independently assembled Huffman fixtures, 1,525 complete code-length histograms, all repeat counts, reserved symbols, code widths 1–15 and legal length/distance values; `tests/huffman.rs` | Covered for these equivalence classes through buffered/fragmented raw and PNG paths, default/compatibility C zlib, 3,091 libdeflate positive checks and structured ASan fuzzing; arbitrary symbol permutations and native-only fuzz coverage review remain Open |
| 10.2: image datastream | One zlib stream across arbitrary IDAT/fdAT splits; exact filtered/native output lengths; prefix tolerance for unused final data | Covered by one-byte payloads, truncations and streaming properties |
| 10.3: compressed metadata | Independent native zlib streams and cumulative raw/inflated metadata limits; metadata/text/profile tests and structured profile fuzzer | Covered; no preset dictionaries or concatenated metadata streams |

## Chunk requirements

| Clause / chunk | Implementation and evidence | Audit |
| --- | --- | --- |
| 11.2.1 IHDR | Required first, singleton, nonzero 31-bit dimensions, legal color/depth, method 0, interlace 0/1 | Covered by native/reference format matrix and malformed headers |
| 11.2.2 PLTE | Presence by color type, 1–256 RGB entries, depth capacity and placement; native index retention | Covered; out-of-range index recovery is listed below |
| 11.2.3 IDAT | Consecutive chunks, arbitrary/empty payload boundaries, single concatenated zlib stream, ignored unused final compressed bytes | Covered by buffered/reader/row/encoder tests |
| 11.2.4 IEND | Empty, required last; whole-file APIs reject trailing bytes, push parser reports prefix completion | Covered by truncation/grammar/prefix tests |
| 11.3.1.1 tRNS | Color-specific lengths, PLTE dependency, missing palette alpha = opaque, unused bits masked, both bytes of 16-bit keys compared | Covered by palette/field/compositing tests; encoder zeroes unused bits |
| 11.3.2.1 cHRM | Exact fixed-point integers, size, 31-bit range, singleton/order and sRGB compatibility | Covered for wire values; physical color interpretation is Application |
| 11.3.2.2 gAMA | Exact nonzero fixed-point value, range, singleton/order and sRGB compatibility | Covered for wire values; gamma conversion is Application |
| 11.3.2.3 iCCP | Names/method, bounded native inflate, RGB/GRAY match, common ICC header/tag framing and version-aware v4.4 table checks | Covered by 960 profile wire cases, eight pinned independent profiles, C oracle and structured mutations; transform/tag semantics remain opaque |
| 11.3.2.4 sBIT | Exact channel count and nonzero depth bounded by native/palette precision | Covered by metadata suite; original significant-bit recovery is available from unchanged native samples |
| 11.3.2.5 sRGB | Four intents, singleton/order; accompanying gAMA/cHRM must use Table 17 values when encoding | Covered by all-format/intent and incompatible-fallback tests; decoder follows higher-precedence description |
| 11.3.2.6 cICP | H.273 (07/2024) primaries/transfer assignments, matrix 0, full-range flag 0/1, singleton/order and highest selection priority | Covered by all 131,072 primaries/transfer/range combinations and all matrix/flag bytes; reserved values retained as unknown when decoding and rejected when encoding |
| 11.3.2.7 mDCV | Exact 16/32-bit fixed-point fields, length/range, cICP dependency and placement | Covered for wire structure; broader semantic/boundary fixtures and tone mapping remain Open / Application |
| 11.3.2.8 cLLI | Exact MaxCLL/MaxFALL values, 31-bit range and placement | Covered for wire structure; broader boundary fixtures and rendering policy remain Open / Application |
| 11.3.3.1 keywords | Printable Latin-1 ranges, 1–79 bytes, no leading/trailing/consecutive spaces | Covered by metadata tests; exhaustive keyword-byte/length matrix remains Open |
| 11.3.3.2 tEXt | Keyword and non-null Latin-1 bytes; raw placement/retention and typed serialization | Covered by PNGSuite, metadata and reference tests |
| 11.3.3.3 zTXt | Method 0 and independent bounded zlib stream; non-null Latin-1 text | Covered by compression/expansion/method tests |
| 11.3.3.4 iTXt | Flags, ignored decoder method when uncompressed, strict encoder method, UTF-8 BOM/replacement, raw retention and BCP47 grammar/registration | Covered by all 256 method values, UTF-8 known answers and 9,902 IANA records/codes; extension-specific semantics need final scope audit |
| 11.3.4.1 bKGD | Color-specific size, indexed bounds, unused-bit masking, encoder normalization and placement | Covered by all nonindexed formats and indexed metadata tests; compositing against suggested background is Application |
| 11.3.4.2 hIST | One frequency per PLTE entry, placement and nonzero frequencies for used encoded indices, including frames | Covered by field tests; suggested-quantization semantics for truecolor are Application |
| 11.3.4.3 pHYs | Two exact 31-bit values, known unit flag, singleton/order | Covered for wire structure; display aspect correction is Application |
| 11.3.4.4 sPLT | Names/depth/entry shape/frequency order; repeated names rejected | Covered by 8/16-bit metadata tests; broader entry-count/frequency mutations remain Open |
| 11.3.4.5 eXIf | Both TIFF byte orders and first-directory framing; unsupported data retained as unknown; encoder rejects unsupported containers | Covered by endian/truncation/offset tests and exact C/Rust payload comparisons; remaining Exif tags/directories/thumbnails are opaque |
| 11.3.5.1 tIME | Exact fields, documented month/day/time ranges, leap second, singleton and legal placement | Covered by metadata tests; calendar/display/local-time policy is Application |
| 11.3.6.1 acTL | Before IDAT, singleton, nonzero frame count, plays/frame limits and actual count agreement | Covered by APNG tests; larger malformed-ordering matrix remains Open |
| 11.3.6.2 fcTL | Contiguous sequence numbers, legal rectangles, default-image restrictions, timing, blend and disposal enums | Covered by 60 format/default-role/interlace combinations and known-answer compositor tests; broader external rendered-frame oracle remains Open |
| 11.3.6.3 fdAT | Sequence prefix, preceding frame control, arbitrary splits/empty frame-data fields, shared image format and independent frame zlib | Covered by native/reference APNG matrix, independent 20-frame fixture and streamed one-byte data |

## Encoders, decoders, viewers and editors

| Clauses | Implementation and evidence | Audit |
| --- | --- | --- |
| 12.1–12.2: color/gamma | Caller supplies stored samples and color metadata; integer metadata stays exact; precedence exposed | Application for selecting/converting color spaces; no implicit CMS |
| 12.3–12.4: alpha and source precision | Straight native alpha retained; encoder accepts legal PNG sample formats and validates ranges | Covered for native PNG contract; assess arbitrary reference-depth scaling against the encoder conformance boundary before final sign-off |
| 12.5: suggested palettes | PLTE/sPLT/hIST serialization and usage checks; no automatic quantization | Covered for supplied metadata; palette generation is Application |
| 12.6–12.7: interlace/filter choice | Plain/Adam7 and all filters selectable; adaptive score shared by both writers | Covered; filter optimization/benchmarking remains Open |
| 12.8: compression | Native stored/fixed-Huffman LZ77, bounded 16-candidate chains, per-block coding choice and cross-block streaming history | Format compatibility covered; repeated local measurements improve ratio/throughput, but broader competitive performance remains Open |
| 12.9: text | UTF-8 serialization, keyword/null checks, native metadata compression, raw/typed APIs | Covered for current format rules; line wrapping/display conventions are Application |
| 12.10: chunking/extensions | Chunk size control, bounded writer chunks, recognized ancillary placement, private-name/property validation | Covered; full ordering mutation matrix remains Open |
| 13.1: errors/recovery | Explicit error enum, fused stream errors, opaque-black indexed recovery, unsupported profile/language ancillary retention | Covered for named recovery cases; APNG fallback-image recovery on animation errors remains Open |
| 13.2–13.3: checking/security | Mandatory CRCs/checksums, grammar/format checks, checked arithmetic, input/output/chunk/frame/metadata limits; eight sanitizer targets | Covered for tested cases; longer campaigns and native-only coverage review remain Open |
| 13.4: privacy | Exact hidden color samples, Exif and text are retained; encoder does not claim redaction or metadata scrubbing | Application; native preservation is intentional |
| 13.5: unknown chunks | Independent property-bit/name-byte/case matrices through buffered, one-/seven-byte reader and direct event paths; literal raw names/data/positions and fused critical errors | Covered for these equivalence classes; reserved-bit ancillary acceptance follows 13.5 despite libpng 1.6.50 rejecting these names |
| 13.6: pixel dimensions | pHYs values available without changing pixel storage | Application for presentation scaling |
| 13.7: text presentation | Typed Unicode decode and raw bytes available | Covered for decoding; escaping controls, line breaks and UI display are Application |
| 13.8–13.9: decompression/filtering | Fully resumable zlib and row reconstruction with checksum-verified image boundaries | Covered by fragmentation, truncation, mixed-block and row-memory tests |
| 13.10: progressive display | Adam7 row pass/origin/step exposed; `ImageEnd` separates provisional rows from verified images | Covered for codec events; progressive filling/painting is Application |
| 13.11–13.12: truecolor/scaling | Native precision retained; explicit full-range RGBA16 expansion | Covered for native values and known expansion cases; display quantization is Application |
| 13.13–13.14: color/gamma rendering | Supported metadata selection is available; source sample space remains explicit | Application; no automatic ICC, HDR or gamma conversion |
| 13.15–13.17: backgrounds/alpha/palettes | bKGD/hIST/sPLT retained; explicit APNG SOURCE/OVER and disposal renderer | Covered for known-answer straight RGBA16 rendering; external rendered-frame differential corpus remains Open |
| 14.1–14.3: editor copy/order rules | Both encoders tested for all 16 property combinations at three positions; unknown unsafe chunks omitted, repeated safe chunks retain binary payload/order/placement, known unsafe metadata follows its own rules | Covered for reencoding, including prewrite rejection of all nonalphabetic name bytes and reserved-bit names; arbitrary edit operations are not provided |
| 15.3.1: datastream conditions | Signature, core grammar, per-chunk fields/relationships, known order constraints and encoder validation | Partial until all open field/order audit items are resolved |
| 15.3.2: encoder conditions | Both native writers agree with independent readers for supported native formats; invalid supplied fields rejected before writer output | Partial: remaining metadata audit and reference-image input boundary require sign-off |
| 15.3.3: decoder conditions | All standardized critical methods/formats, unknown property/case matrices, both transparency bytes and arbitrary data splitting | Partial: broader malformed equivalence classes and completion audit remain |
| 15.3.4: editor conditions | Native reference-image samples preserved; safe-to-copy handling implemented | Partial for reencoding API; no general editing UI or color-managed viewer is claimed |

## Completion evidence still required

The user also requested competitive performance, comprehensive differential and
fuzz evidence, independent reuse and no DeadSync integration. Package isolation
and the production dependency boundary are verified by the retained offline
build. Performance is explicitly incomplete in `BENCHMARKS.md`; the current
fixed-Huffman encoder's ratio and pipeline speed require substantial work.
The later Huffman refill/prefix optimization improves measured photo decoding,
but the native 78.98 MiB/s still trails Rust png's 202.58 on that corpus.
Resolve the open rows above, obtain broader external APNG rendering comparisons,
extend structured malformed/coverage campaigns, and repeat the requirement audit
against the resulting implementation and measurements before claiming completion.
