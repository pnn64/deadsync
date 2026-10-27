# Native libpng oracle (test only)

The C helper emits packed native samples without color, gamma, alpha, or depth
transformation. It retains libpng's error/setjmp boundary inside a separate
process. CMake and a C compiler are needed only for this optional external check.

`--reference-samples FILE.png` adds an independent significant-bit recovery
mode. Its output is the usual width/height/PNG-depth/color header (four u32 BE
words), followed by the original sBIT byte count (u32 BE), those channel-depth
bytes, and scanlines transformed by libpng's `png_set_shift`. Recovered values
remain packed at the PNG storage depth; row padding is not sample data. The
reference-sample test checks 576 files through both ordinary native decoding
and this recovery mode, including mixed channel precision and alpha. The
ordinary mode continues to apply no sample transformations.

From the workspace root (PowerShell; choose any scratch reference directory):

```powershell
git clone --depth 1 --branch v1.6.50 https://github.com/pnggroup/libpng.git target/deadlib-image-reference/libpng
git clone --depth 1 --branch v1.3.1 https://github.com/madler/zlib.git target/deadlib-image-reference/zlib
$referenceRoot = (Resolve-Path target/deadlib-image-reference).Path
cmake -S crates/deadlib-image/png/tests/oracle -B target/deadlib-image-reference/oracle-build "-DREFERENCE_ROOT=$referenceRoot" -A x64
cmake --build target/deadlib-image-reference/oracle-build --config Release --target deadlib_png_oracle --parallel 4
$env:DEADLIB_LIBPNG_ORACLE = (Resolve-Path target/deadlib-image-reference/oracle-build/Release/deadlib_png_oracle.exe).Path
cargo test -p deadlib-image-png --release --test libpng libpng_native_sample_conformance -- --ignored
cargo test -p deadlib-image-png --release --test profiles -- --ignored
cargo test -p deadlib-image-png --release --test zlib_headers c_zlib_header_and_history_oracle -- --ignored --nocapture
```

Omit `-A x64` and use the platform's executable path for other CMake generators.
Pinned source commits: libpng `2b978915d82377df13fcbb1fb56660195ded868a`, zlib
`51b7f2abdade71cd9bb0e7a373ef2610ec6f9daf`. Reference source stays outside
the production crate and is never linked to it. The test fails if an oracle is
missing; normal `cargo test` explicitly reports it as ignored.

`--metadata FILE.png` emits ICC length and Exif length (two big-endian u32s),
followed by the exact uncompressed profile and Exif bytes retained by libpng.
The profiles test checks 480 encodings across eight pinned independent ICC v2/v4
profiles, all 15 native PNG formats, both interlace settings, both native writers
and both Exif byte orders. This supplements the existing 1,061 native-sample
comparisons; it does not introduce any C linkage into the production library.

`libpng_external_corpus` is a separate ignored test requiring
`DEADLIB_PNG_CORPUS` as well as the C executable. The hash-pinned external RGB8
inputs are fetched into ignored build storage; acquisition and commands are in
[BENCHMARKS.md](../../BENCHMARKS.md#external-rgb8-corpus). Its 24 original images
and eight reencodings each add 216 exact native-sample comparisons, covering
both writers, Stored/Fast compression and plain/Adam7 output.

`--zlib-batch FILE` cross-checks independent raw compression fixtures in one
process. Each input record contains a big-endian u32 compressed length, a
big-endian u32 output capacity, then compressed bytes. Capacities must be 1 byte
through 1 MiB; compressed inputs are capped at 16 MiB. Each result contains the
signed zlib status encoded as a big-endian i32, a big-endian u32 successful output
length, then those output bytes. Failed records have length zero. The driver
also rejects unused compressed bytes, matching the whole-stream zlib API.

The header/history test sends 65,652 records: every CMF/FLG pair, 92 window
boundary/padding/history cases and 24 references before available output. It
also checks 24 malformed PNG wrappers through the normal libpng mode. Successful
outputs are compared byte for byte. Default C zlib accepts over-advertised-window
distances; an explicit strict configuration validates the native rejection policy:

```powershell
$referenceRoot = (Resolve-Path target/deadlib-image-reference).Path
cmake -S crates/deadlib-image/png/tests/oracle -B target/deadlib-image-reference/oracle-strict-build "-DREFERENCE_ROOT=$referenceRoot" -DDEADLIB_ZLIB_STRICT_WINDOW=ON -A x64
cmake --build target/deadlib-image-reference/oracle-strict-build --config Release --target deadlib_png_oracle --parallel 4
$env:DEADLIB_LIBPNG_ORACLE = (Resolve-Path target/deadlib-image-reference/oracle-strict-build/Release/deadlib_png_oracle.exe).Path
$env:DEADLIB_ZLIB_STRICT_WINDOW = '1'
cargo test -p deadlib-image-png --release --test zlib_headers c_zlib_header_and_history_oracle -- --ignored --nocapture
Remove-Item Env:DEADLIB_ZLIB_STRICT_WINDOW
```

The flag enables zlib's existing `INFLATE_STRICT` checks in the reference library;
the production Rust crate has no C dependency. Its source remains unchanged.
Default and strict results, tool versions, active reference features and hashes
are retained in [zlib-conformance.log](../../zlib-conformance.log) and
[zlib-conformance.json](../../zlib-conformance.json).

These fixtures follow [RFC 1950 sections 2.2/2.3](https://www.rfc-editor.org/rfc/rfc1950)
and [RFC 1951 section 3.2.3](https://www.rfc-editor.org/rfc/rfc1951).
The flate2/miniz reference accepts the 24 raw fixtures that refer before output
exists. Native decoding and C zlib reject them; Rust png and libpng reject the
PNG wrappers. Reference acceptance of malformed data is recorded as a policy
difference and does not override the native rejection asserted by the tests.

## Huffman grammar audit

`tests/huffman.rs` checks 19,645 independently constructed compressed streams
against native buffered/fragmented decoding and default C zlib. It explicitly
records reference policy differences. RFC 1951 permits 1–32 dynamic distance
definitions while reserving symbols 30/31 from actual compressed data. Default
zlib rejects definitions beyond 30, including unused or zero-length entries.
The [maintainer's explanation](https://github.com/madler/zlib/issues/82) identifies
that deliberate restriction. Rust png and flate2/miniz reject the 68 positive
fixtures with those definitions too; the tests verify those reference rejections.
Native decoding accepts the unused definitions and rejects consumed reserved
symbols, following the published field limits.

A second C build enables zlib's existing `PKZIP_BUG_WORKAROUND` flag to verify
these positives independently. This flag also disables the 286-literal limit:
it accepts 64 otherwise valid fixtures with 287/288 literal definitions, which the native codec
and default zlib reject. The compatibility build is therefore not their negative
oracle. This distinction is asserted rather than silently skipping disagreement.

```powershell
$ref = (Resolve-Path target/deadlib-image-reference).Path
cmake -S crates/deadlib-image/png/tests/oracle -B "$ref/oracle-pkzip-build" -DREFERENCE_ROOT="$ref" -DDEADLIB_ZLIB_STRICT_WINDOW=ON -DDEADLIB_ZLIB_PKZIP_WORKAROUND=ON
cmake --build "$ref/oracle-pkzip-build" --config Release --target deadlib_png_oracle --parallel 4
$env:DEADLIB_LIBPNG_ORACLE = "$ref/oracle-pkzip-build/Release/deadlib_png_oracle.exe"
$env:DEADLIB_ZLIB_PKZIP_WORKAROUND = '1'
cargo test -p deadlib-image-png --release --test huffman c_zlib_huffman_oracle -- --ignored --nocapture
Remove-Item Env:DEADLIB_ZLIB_PKZIP_WORKAROUND
$env:DEADLIB_LIBPNG_ORACLE = "$ref/oracle-build/Release/deadlib_png_oracle.exe"
cargo test -p deadlib-image-png --release --test huffman c_zlib_huffman_oracle -- --ignored --nocapture
```

The two runs, exact commands and source/binary hashes are retained in
`huffman-conformance.log` and `huffman-conformance.json` in the PNG package.
The generated fixtures span every complete code-length histogram with 2–19
leaves and maximum width seven; they do not enumerate all symbol permutations.
All length values 3–258 and distances 1–32,768 are exercised in fixed/dynamic
blocks at all eight bit offsets and both alignment-padding values. This is an
equivalence-class audit, not enumeration of every possible compressed stream.

## Literal chunk names and core ordering

```powershell
$env:DEADLIB_LIBPNG_ORACLE = (Resolve-Path target/deadlib-image-reference/oracle-build/Release/deadlib_png_oracle.exe).Path
cargo test -p deadlib-image-png --release --test chunks libpng_unknown_chunk_and_core_order_comparison -- --ignored --nocapture
```

This compares 1,893 independently constructed files: all 16 unknown chunk
property combinations at three positions, all noncanonical case variants of the
25 standard chunk names at those positions, and all 720 permutations of a small
indexed image's six chunks (IHDR, PLTE, tRNS, two distinct IDAT parts, IEND).
The native buffered/fragmented readers and exact pixels have separate ordinary
tests. No native encoding, CRC or compression code constructs these inputs.

Pinned libpng 1.6.50 rejects all 312 reserved-bit ancillary fixtures here.
The native decoder accepts these unknown ancillary names, following the
[Third Edition's forward-compatible decoder guidance](https://www.w3.org/TR/2025/REC-png-3-20250624/#13Unknown-chunk-handling).
Both native encoders still require current-version names with the reserved bit
clear. Libpng accepts three malformed core orders (tRNS before PLTE or after
IDAT/IEND); Rust png 0.18.1 accepts 11, including PLTE after IDAT/IEND.
Both native parsers reject all 719 invalid permutations. Reference recovery
does not override those assertions. Counts, commands and source/binary hashes
are retained in `chunks-validation.log` and `chunks-validation.json`.


## Named metadata values

`--named-metadata file.png` emits a version-1 record stream: version and record
count (u32 BE), then each four-byte chunk kind, u32 BE name length, literal name,
u32 BE value length, and value. Text/profile values are decompressed. Suggested
palette values contain one depth byte followed by five u16 BE fields per entry,
including when the original samples used eight-bit encoding. Language and
translated-keyword fields are not part of this protocol.

```powershell
cmake --build target/deadlib-image-reference/oracle-build --config Release --target deadlib_png_oracle --parallel 4
$env:DEADLIB_LIBPNG_ORACLE = (Resolve-Path target/deadlib-image-reference/oracle-build/Release/deadlib_png_oracle.exe).Path
cargo test -p deadlib-image-png --release --test keywords libpng_ -- --ignored --nocapture
```

The named-field test checks 3,710 files covering all single-byte names, every
byte in a 79-byte name's interior, length boundaries and spacing. All 2,702
positive files retain their exact names and text/profile/palette values.
Malformed-name decoder policies are counted separately: native strict metadata
validation and encoder rejection do not use C recovery as a negative oracle.
A further 158 suggested-palette files cover sample/frequency/count boundaries.
Libpng retains 114 valid palettes exactly, ignores two valid empty palettes,
and retains all 42 invalid ascending-frequency palettes; native validation
accepts empty palettes and rejects ascending frequencies. Native interpretation
also checks independent big-endian values across the full normal test matrix.
`keywords-validation.log` and `keywords-validation.json` retain the commands,
counts and hashes, including compatibility checks of the original pixel and
ICC/Exif oracle modes.


## Latin-1 text-body boundary

```powershell
cargo test -p deadlib-image-png --release --test text_body libpng_all_body_bytes_and_native_writer_values -- --ignored --nocapture
```

Using the same version-1 named-metadata mode, this checks all 256 single-byte
bodies in both tEXt and zTXt, plus both native writers for every conforming byte.
All 384 conforming raw values and 768 native-writer values match exactly,
including LF, spaces and NBSP. Libpng retains the 126 non-null malformed control
bodies and changes the two NUL bodies. Native raw decode retains non-null bodies;
native encoders reject the control/NUL inputs before output. Recovery behavior
is recorded separately from encoder conformance. `text-body-validation.log`
and `.json` retain results and hashes; `text-body-before.log` and `.json` show
the final regression test failing against the isolated pre-fix source.


## APNG static recovery

```powershell
cargo test -p deadlib-image-png --release --test apng_recovery libpng_static_fallback_known_answers_and_crc_policy -- --ignored --nocapture
```

The test constructs native known answers independently for all 15 formats,
plain/Adam7 layout and both default-image roles. The unpatched pinned libpng
reads their static images: 60 static-only files, 60 valid animations and 3,000
animation faults. It returns exact native pixels for 2,880 files, including
well-framed animation syntax/data errors that it ignores, and rejects the 240
APNG-CRC cases under the oracle's strict CRC setting. The opt-in native recovery
policy reports those animation faults and completes static/core PNG checks.
No C reference rejection is silently treated as a sample mismatch.
`apng-recovery-validation.log` and `.json` retain the results and hashes.

## Exact HDR metadata

```powershell
cargo test -p deadlib-image-png --release --test hdr libpng_exact_hdr_fixed_values_and_invalid_integer_policy -- --ignored --nocapture
```

`--hdr-metadata` emits big-endian u32 words: protocol version 1, presence bits
(cICP=1, mDCV=2, cLLI=4), followed by present chunks' fields. cICP has four words;
mDCV has eight chromaticities in red/green/blue/white x/y order, then maximum/
minimum luminance; cLLI has MaxCLL/MaxFALL. Chromaticities are the exact libpng
fixed-point getter values scaled by 100,000; luminances use the getter's 10,000
scale. The native test compares its original u16 coordinates multiplied by two,
without floats, and catches white-point relocation or scaling errors.

All 7,728 raw fixtures within PNG's integer range and 60 native writer files
match exactly. For 524 over-range fixtures, libpng ignores the affected HDR
chunk while native decoding rejects the input; presence flags verify this
policy difference. `hdr-oracle-validation.log` retains the results.
