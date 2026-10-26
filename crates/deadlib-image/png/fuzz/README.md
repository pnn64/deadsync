# PNG and zlib fuzz targets

This is an isolated Cargo workspace. It never adds libFuzzer, png, flate2, or any
other codec to the production dependency graph. All targets bound decompression,
metadata, frame counts and input sizes before exercising untrusted inputs.

On Linux with nightly Rust, rust-src, a C++ compiler, and cargo-fuzz:

```sh
cargo run -p deadlib-image-png --example seed_fuzz
cd crates/deadlib-image/png
cargo +nightly fuzz run decode -- -runs=10000 -seed=782347 -max_len=65536 -timeout=5 -rss_limit_mb=1024 -print_final_stats=1
cargo +nightly fuzz run zlib -- -runs=20000 -seed=782347 -max_len=65536 -timeout=5 -rss_limit_mb=1024 -print_final_stats=1
cargo +nightly fuzz run png_stream -- -runs=20000 -seed=782347 -max_len=65536 -timeout=5 -rss_limit_mb=1024 -print_final_stats=1
cargo +nightly fuzz run inflate_stream -- -runs=20000 -seed=782347 -max_len=65536 -timeout=5 -rss_limit_mb=1024 -print_final_stats=1
cargo +nightly fuzz run encode_stream -- -runs=20000 -seed=782347 -max_len=256 -timeout=5 -rss_limit_mb=1024 -print_final_stats=1
cargo +nightly fuzz run profiles -- -runs=20000 -seed=782347 -max_len=4096 -timeout=5 -rss_limit_mb=1024 -print_final_stats=1
cargo +nightly fuzz run deflate_stream -- -runs=20000 -seed=782347 -max_len=131072 -timeout=5 -rss_limit_mb=1024 -print_final_stats=1
cargo +nightly fuzz run huffman -- -runs=20000 -seed=782347 -max_len=64 -timeout=10 -rss_limit_mb=1024 -print_final_stats=1
```

Seed generation copies the checked-in independent PNGSuite and APNG fixtures and
extracts their IDAT zlib streams. Working corpora, crash artifacts and builds are
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
sample bug. `zlib` compares every native-accepted stream with bounded flate2
decompression. Integration tests separately check generated valid streams and
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
native-accepted stream also checks flate2 output when that decoder accepts it.
Default miniz/zlib reject unused HDIST 31/32 definitions permitted by the published
RFC. The deterministic suite explicitly checks that policy, and a C compatibility
build independently verifies all such positive fixtures. A reference rejection
of an arbitrary mutation is therefore not treated as a native conformance verdict.
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
