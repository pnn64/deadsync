# Native libpng oracle (test only)

The C helper emits packed native samples without color, gamma, alpha, or depth
transformation. It retains libpng's error/setjmp boundary inside a separate
process. CMake and a C compiler are needed only for this optional external check.

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
