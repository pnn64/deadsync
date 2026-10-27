# PNG and zlib fuzz targets

This is an isolated Cargo workspace. It never adds libFuzzer, png, flate2, or any
other codec to the production dependency graph. All targets bound decompression,
metadata, frame counts and input sizes before exercising untrusted inputs.

On Linux with nightly Rust, rust-src, Clang/C++ compilers, and cargo-fuzz:

```sh
cargo run -p deadlib-image-png --example seed_fuzz
cd crates/deadlib-image/png
cargo +nightly fuzz run decode -- -runs=10000 -seed=782347 -max_len=65536 -timeout=5 -rss_limit_mb=1024 -print_final_stats=1
CC=clang CFLAGS="-fsanitize=address" cargo +nightly fuzz run zlib -- -runs=20000 -seed=782347 -max_len=262144 -timeout=10 -rss_limit_mb=1024 -print_final_stats=1
cargo +nightly fuzz run png_stream -- -runs=20000 -seed=782347 -max_len=65536 -timeout=5 -rss_limit_mb=1024 -print_final_stats=1
cargo +nightly fuzz run inflate_stream -- -runs=20000 -seed=782347 -max_len=65536 -timeout=5 -rss_limit_mb=1024 -print_final_stats=1
cargo +nightly fuzz run encode_stream -- -runs=20000 -seed=782347 -max_len=256 -timeout=5 -rss_limit_mb=1024 -print_final_stats=1
cargo +nightly fuzz run profiles -- -runs=20000 -seed=782347 -max_len=4096 -timeout=5 -rss_limit_mb=1024 -print_final_stats=1
cargo +nightly fuzz run deflate_stream -- -runs=20000 -seed=782347 -max_len=131072 -timeout=5 -rss_limit_mb=1024 -print_final_stats=1
CC=clang CFLAGS="-fsanitize=address" cargo +nightly fuzz run huffman -- -runs=20000 -seed=782347 -max_len=64 -timeout=10 -rss_limit_mb=1024 -print_final_stats=1
```

Seed generation copies the checked-in independent PNGSuite and APNG fixtures and
extracts their IDAT zlib streams. It also adds 649 independent Huffman wire
fixtures to the raw zlib corpus, including all 68 extended-distance positives.
Both PNG corpora additionally receive 1,893 independent chunk-grammar files:
all property flags, case variants of the 25 standard names and every permutation
of a six-chunk indexed image. Seed filenames encode the literal case-sensitive
names as hexadecimal so they remain distinct on Windows filesystems.
Another 3,710 independent files cover text/profile/palette name bytes, spacing
and length boundaries across seven named-metadata wire forms. Current fresh PNG
corpora contain 5,779 files each; `reports/named-source.json` records their exact
hashes and the corresponding 40,000-execution ASan campaigns.
Working corpora, crash artifacts and builds are
ignored. Committed fixtures and the seed generator reproduce the initial corpus;
do not reuse a previously mutated corpus when reproducing an exact smoke run.
An optional argument to `seed_fuzz` creates a fresh corpus root and refuses an
existing destination. For example, from the repository root:

```sh
cargo run -p deadlib-image-png --release --example seed_fuzz -- target/png-fuzz-fresh
```

Pass the desired subdirectory, such as `target/png-fuzz-fresh/deflate_stream`,
after the target name in `cargo fuzz run` (using a path relative to its working
directory). This keeps each campaign's initial corpus separate from mutations.

`decode` checks for native parser failures and compares every native sample when
the independent Rust png decoder also accepts the image. Reference ancillary
policies differ, so a reference rejection does not automatically mean a native
sample bug. `zlib` compares every native-accepted stream with bounded libdeflate
decompression, including published HDIST 31/32 definitions that default flate2
rejects. It caps compressed input at 256 KiB and output at 1 MiB. The reference
is confined to this isolated fuzz workspace; normal PNG dependencies are unchanged.
Integration tests separately check generated valid streams and
native output rejected for truncation/checksum/limit errors. Coverage counters
include the harness and reference codecs, not just the native implementation.

`png_stream` repairs complete chunk CRCs to explore deeper grammar and DEFLATE
paths. It compares whole-file acceptance with the buffered implementation and
every emitted default-image / APNG-frame row sample, under varying input splits.
It selects both opaque-black recovery and strict indexed-color policies.
`inflate_stream` compares fragmented decoding, prefix byte counts and fused
errors against buffered native zlib decoding. Independent reference parity is
provided by the other targets and integration/oracle tests.

`encode_stream` derives legal native images and APNG rectangles from shrinkable
byte inputs, covering all 15 formats, filters, interlace, both compression modes,
transparency and text. It checks the stream writer's native roundtrip through
both decoders, bounded capacity, byte counts, and independent Rust png frame
samples. The seed generator provides 1,080 deterministic structured inputs.
An initial harness assumed distinct metadata slots without PLTE; the corrected
canonical placement and a fixed-input replay are documented in `reports/`.

## Structured profile target

`profiles` mutates ICC headers, tag-table records, version fields, Exif byte-order
signatures and first-directory bounds, or consumes arbitrary payloads. It uses
independent flate2 compression and repaired chunk CRCs so mutations reach the
native metadata validators. It compares buffered and fragmented-reader acceptance,
exact raw metadata/native samples, encoder validation before writing, supported
profile roundtrips, and Rust png's retained ICC/Exif bytes. The deterministic
generator provides 1,800 eight-byte seeds across all 15 PNG formats, eight
independent profiles, both compression modes, invalid fields and resource limits.

The harness type-checks on Windows, but the instrumented MSVC build encountered
unresolved `__start___sancov_*` / `__stop___sancov_*` linker symbols on this host.
The smoke campaigns therefore use Debian WSL and Linux nightly. If the checkout
is on a Windows filesystem, select a Linux build directory with
`--target-dir /tmp/deadlib-image-fuzz-target` and install cargo-fuzz on Linux's
filesystem. Campaign logs and exact tool versions are recorded in `reports/`.

Smoke campaigns are a regression check, not exhaustive validation. Longer runs,
additional structured generators, independent APNG compositing comparisons,
and coverage review remain required.

## Structured Huffman target

`huffman` selects among 19,645 independently assembled RFC 1951 streams. Its
64-byte input controls payload bit flips, truncation, suffixes, output limits and
input/output fragmentation. The shared fixture generator includes malformed
trees/counts/repeats/reserved symbols and valid long canonical codes, cross-table
repeats, overlapping matches, all legal length/distance values and bit offsets.
The seed generator emits 649 deterministic twelve-byte selectors spanning every
family. It does not store the generated compressed streams in the fuzz input.

Unmodified fixtures have independent exact-output/acceptance expectations.
Mutated streams check buffered/fragmented acceptance, samples, prefix consumption,
fused errors, progress, output tail guards and bounded inflater memory. Every
native-accepted stream checks libdeflate output, with flate2 as an additional
comparison when that decoder accepts it.
Default miniz/zlib reject unused HDIST 31/32 definitions permitted by the published
RFC. The deterministic suite explicitly checks that policy, and a C compatibility
build independently verifies all such positive fixtures. A reference rejection
of an arbitrary mutation is therefore not treated as a native conformance verdict;
libdeflate now supplies the unconditional positive check.
See `../huffman-conformance.json` and `../tests/oracle/README.md` for the two C
configurations and their different literal-count behavior. Longer campaigns,
arbitrary symbol permutations and native-only coverage analysis remain open.

The initial harness timed out after 697 executions because it reset and scanned
4 KiB of sentinel space for every one-byte input fragment of a large valid
stream. The stack resolved to the harness tail assertion. The corrected target
resets only previously written bytes and checks both ends of the unused tail
plus the byte beyond the supplied slice; deterministic tests still check every
untouched byte. It also caps mutated output by the fixture's expected size or
4 KiB. `reports/huffman-initial-timeout.log`, the exact thirteen-byte
`huffman-timeout-input.bin`, the corrected replay log, and a reverse patch in
`../experiments/huffman-harness-timeout.patch` preserve the diagnosis. The initial
697 executions are not included in successful campaign totals. Fixture creation
runs once per process and contributes to fixed-input replay time.

The corrected fresh campaign passes 20,000 executions in 857 seconds with a
394 MiB peak RSS, using a ten-second per-input timeout. Together with earlier
successful campaigns this records 220,000 executions across eight targets.
`reports/huffman-source.json` preserves options, source/binary hashes and the
separate preliminary timeout. Native-only integration-test coverage is recorded
under `../coverage-reports/`; it is not a native fuzz coverage measurement.

## Selected inflation follow-up

The wider Huffman refill and canonical-prefix decoder passes fresh 5,000-run
campaigns for `huffman`, `png_stream`, `zlib` and `profiles`. Logs are named
`reports/inflate-<target>-linux.log`; `reports/inflate-source.json` binds all four
binaries to exact production/harness source, options, initial corpus hashes and
defined AddressSanitizer initialization symbols. They take 118 / 15 / 6 / 65
seconds and peak at 396 / 193 / 128 / 368 MiB respectively. These add 20,000
successful executions, bringing historical campaigns to 240,000 across eight
targets. Their counters include references; the native integration-test coverage
snapshot under `../coverage-reports/` remains a different measurement.

The first staging attempt used Linux `copytree` on the Windows-mounted checkout
and failed while preserving metadata, before any fuzzing started. Fresh Windows
`Copy-Item` directories were compared byte for byte with the untouched seed
generator output before use. No partly staged or previously mutated corpus was
reused. Generate directly into a fresh Linux directory when reproducing there.

## Compatible positive reference

The raw target's old flate2-only assertion failed on the legal regression stream
`reports/reference-hdist31.zlib`: native output is two zero bytes, while miniz
rejects its unused distance definitions. The saved `cafedc93d` harness reproduces
exit 77; the updated ASan harness accepts the exact input. Both replay logs and
source/binary/fixture hashes are retained. Replays are excluded from campaign totals.

The reference now uses pinned `libdeflater` / `libdeflate-sys` 1.26.1, bundling
libdeflate 1.26. Its [header parser](https://github.com/ebiggers/libdeflate/blob/v1.26/lib/decompress_template.h)
supports all 32 distance definitions. Its [tree builder](https://github.com/ebiggers/libdeflate/blob/v1.26/lib/deflate_decompress.c)
also permits some malformed codes, so it serves as a positive-output oracle.
Native and strict C negative assertions remain in the conformance tests.
`fuzz/examples/reference_check.rs` verifies all 3,091 valid Huffman fixtures,
including all 68 extended-distance cases, plus all 32 window/level headers on
Windows and Linux. Native rejects all 16,554 malformed fixtures. A bounded
64-byte reference probe accepts 64 excess-literal-count streams and one unused
code branch; those reference acceptances do not change native validation.

```sh
cargo run --locked --manifest-path crates/deadlib-image/png/fuzz/Cargo.toml --release --example reference_check
```

Fresh campaigns pass 20,000 raw-zlib executions in 18 seconds (378 MiB peak) and
10,000 structured-Huffman executions in 157 seconds (404 MiB). The raw corpus has
810 files / 4,803,389 bytes with a 136,426-byte maximum; the selector corpus has
649 files / 7,788 bytes. Every native-accepted Huffman mutation now has an
independent positive check, including previously unsupported distance definitions.
Clang 19.1.7 builds the C reference with AddressSanitizer too; archive references
to `__asan_report_*` and defined `__asan_init` in both final binaries are verified.

`reports/reference-source.json` records the commands, environment, package and
bundled-source hashes, corpora, instrumented archives and binaries. The normal
tests, Clippy, dependency boundary and isolated build are retained in
`../reference-conformance.log` / `.json`. These 30,000 executions bring successful
historical campaigns to 270,000 across eight targets. They are smoke campaigns;
native-only fuzz coverage and longer runs remain open.

The conformance follow-up used a fresh `fuzz/corpus/png_conformance_20261026`
directory containing only the 175 checked-in PNGSuite files and independent
`basic_f20.png` APNG. Pass that directory after `png_stream` in the command above
to reproduce the initial seeds. The 20,000-run log is
`reports/png-conformance-linux.log`; the corpus directory must be fresh because
libFuzzer adds and reduces files during a campaign.
The IANA follow-up uses the same fresh initial files in
`fuzz/corpus/png_registry_20261026`, with diagnostics in
`reports/png-registry-linux.log`.

## Raw streaming compression target

`deflate_stream` treats the complete fuzz input as raw data, up to 128 KiB. It
compares buffered and fragmented Fast/Stored encodings with independent flate2,
native whole-stream decoding and a separately fragmented native inflater. Input
and output sizes, empty output calls and separate finish calls vary; encoder
capacity must remain constant. This reaches arbitrary byte patterns and multiple
32 KiB history wraps beyond the structured image generator.

`seed_fuzz` prepares 185 initial raw files: 161 valid PNGSuite IDAT inflations
(capped at 128 KiB) plus 24 zero/noise/mixed/repeated patterns around 32 KiB,
64 KiB and 128 KiB boundaries. Start from a fresh corpus when reproducing a run;
libFuzzer mutates and reduces it. The matcher follow-up also reruns
`encode_stream` with a fresh `fuzz/corpus/encode_lz_20261026` copy of its 1,080
deterministic 12-byte seeds. Exact commands are in the retained campaign logs.
