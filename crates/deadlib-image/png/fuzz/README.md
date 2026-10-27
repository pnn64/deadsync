# PNG and zlib fuzz targets

`compositor` checks bounded native animations, literal independently framed APNG
bytes, exact expansion and full-canvas composition against a u128 rational model.
The reference uses full-canvas snapshots and immediate disposal. Native
composition uses saved rectangles and deferred disposal. All formats, both
default-image roles, plain/Adam7, partial rectangles, all disposal/blend modes,
alpha/transparent-key/palette recovery, replay/reset and exact snapshot budgets
vary. This target calls no external reference codec. Inputs are 16–4,096 bytes;
generated images are at most 9x7 with eight frames and two palette entries.
The initial 480 structured seeds include recoverable invalid palette indices;
240 valid wire fixtures also extend each PNG corpus.

```sh
cargo +nightly fuzz run compositor -- -runs=20000 -seed=928671 -max_len=4096 -timeout=10 -rss_limit_mb=1024 -print_final_stats=1
```

`reference_samples` exercises the explicit 1–16-bit reference constructor with
uniform/mixed channel precision, both input containers, invalid depth/value/
dimension/count classes, alpha endpoints and high-bit recovery. Its independent
u64 rational model checks stored samples, target depth and sBIT; both writers
then roundtrip through native buffered/reader paths and independent Rust png,
including packed grayscale, Adam7, every filter and Stored/Fast compression.
The generator supplies 640 structured 12-byte seeds plus 256 valid converted
PNG files in each PNG corpus. Input is capped at 4,096 bytes; generated images
are at most 17x17 pixels. Invalid large dimensions fail before allocation.

```sh
cargo +nightly fuzz run reference_samples -- -runs=20000 -seed=928671 -max_len=4096 -timeout=10 -rss_limit_mb=1024 -print_final_stats=1
```

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
cargo +nightly fuzz run text_body -- -runs=20000 -seed=782347 -max_len=4097 -timeout=10 -rss_limit_mb=1024 -print_final_stats=1
```

Seed generation copies the checked-in independent PNGSuite and APNG fixtures and
extracts their IDAT zlib streams. It also adds 649 independent Huffman wire
fixtures to the raw zlib corpus, including all 68 extended-distance positives.
Both PNG corpora additionally receive 1,893 independent chunk-grammar files:
all property flags, case variants of the 25 standard names and every permutation
of a six-chunk indexed image. Seed filenames encode the literal case-sensitive
names as hexadecimal so they remain distinct on Windows filesystems.
Another 3,710 independent files cover text/profile/palette name bytes, spacing
and length boundaries across seven named-metadata wire forms. That milestone's PNG
corpora contain 5,779 files each; `reports/named-source.json` records their exact
hashes and the corresponding 40,000-execution ASan campaigns.
Text-body seeds add all 256 byte values in both tEXt/zTXt to the PNG corpora,
bringing that milestone's fresh counts to 6,291 files each. The dedicated `text_body`
target has 4,304 initial inputs: flags/single-byte values plus 79-/80-/4096-byte
plain, graphic, late-control and embedded-NUL bodies.

The compositor follow-up passes 20,000 structured compositor, 30,000 PNG-stream
and 30,000 buffered PNG ASan inputs. Fuzzing seconds / peak RSS are 68 / 430 MiB,
33 / 500 and 7 / 466. `reports/compositor-source.json` retains source/corpus/binary
hashes, environment and commands; full target logs are alongside it. The final
compositor corpus is separately replayed for scoped native source coverage in
`../coverage-reports/compositor-*`; its fixed-input executions are excluded from
fuzz totals. The 80,000 successful ASan inputs bring historical campaigns across
all eleven targets to 1,000,000, including earlier encoder runs. Whole-decoder
feedback coverage and longer campaigns remain open.

## Exact zlib prefixes and consumed-byte overflow

`zlib` now accepts a prefix result, validates that exact stream independently
with libdeflate, checks strict trailing-data rejection, and verifies stable output
and consumption after appending a sentinel. Its 256 KiB input and 1 MiB output
limits remain unchanged. The deterministic prefix suite separately checks
88,414 suffix/padding placements and 24,728 body/trailer truncations; a virtual
cursor regression covers the overflow-safe consumed-byte calculation.

Fresh ASan campaigns pass 10,000 `huffman`, 20,000 `zlib`, 20,000 `inflate_stream`,
30,000 `png_stream` and 30,000 `decode` executions. Initial corpus counts are
649, 810, 161, 27,867 and 27,867; the last two each total 6,368,086 bytes.
Fuzzing seconds / peak RSS are 132 / 410 MiB, 18 / 382, 16 / 398, 30 / 511 and
7 / 482 respectively. `reports/prefix-source.json` and per-target logs retain
commands, source/corpus/binary hashes, defined ASan runtime entries and matching
final sources. These 110,000 decoder executions bring historical successful
campaigns to 1,110,000 across eleven targets, including earlier encoder runs.
No encoder target is used in this continuation. Feedback still includes
reference code; this is not a whole-decoder native-only coverage claim.

## Feedback restricted to native crates

`scripts/fuzz_native.py` now supports a separate Linux ASan campaign whose
feedback is restricted to `deadlib_image_png` and `deadlib_image`. Its
[Cargo compiler wrapper](https://doc.rust-lang.org/cargo/reference/environment-variables.html)
removes only SanitizerCoverage flags from harnesses and other crates. ASan and
other compiler options remain. The runner records each invocation and inspects
compiled archive and harness sections rather than relying on the flags alone.
Native-crate std/alloc monomorphizations remain in feedback; this is not
source-only instrumentation. Existing differential targets remain available.

The first run passes 50,000 PNG-stream and 20,000 raw-stream inputs. Native PNG
and shared primitives have 16,550 / 45 counter bytes; all fourteen dependency
archives and four harness objects have no feedback counter or PC table sections.
Fuzzing seconds / peak RSS are 71 / 524 MiB and 12 / 398. Initial corpus counts
are 27,867 / 939, including bounded zlib fixtures in the raw-stream corpus.
`reports/native-feedback-source.json` retains source/binary/corpus hashes,
compiler arguments, object audits and the correction for Cargo's artifact layout.
That milestone brings the historical successful total to 1,180,000 across eleven targets,
including previous encoder campaigns. No encoder target runs here.

The companion `scripts/coverage_fuzz_native.py` replays these final corpora and
maps explicit native source files only. Its fixed replay is excluded from fuzz
totals. Reports and reviewed gaps are in `../coverage-reports/README.md`.
Create fresh seeds and transfer them as one ZIP to avoid repeated cross-filesystem
small-file reads:

```powershell
cargo run -p deadlib-image-png --example seed_fuzz -- target/png-native-seeds-fresh
python crates/deadlib-image/png/fuzz/scripts/archive_native_seeds.py --corpus-root target/png-native-seeds-fresh --output target/png-native-seeds-fresh.zip
```

Run on Linux with fresh output and target directories:

```sh
python3 crates/deadlib-image/png/scripts/fuzz_native.py --cargo-fuzz /path/to/cargo-fuzz --seed-archive target/png-native-seeds-fresh.zip --target-dir /tmp/png-native-fuzz-fresh --output-dir /tmp/png-native-campaign-fresh
python3 crates/deadlib-image/png/scripts/coverage_fuzz_native.py --campaign-output /tmp/png-native-campaign-fresh --cargo-fuzz /path/to/cargo-fuzz --target-dir /tmp/png-native-coverage-build-fresh --output-dir /tmp/png-native-coverage-fresh
```

The coverage runner now copies the fuzz project and its shared test helpers into
the fresh output directory, records their hashes, and uses `--fuzz-dir` for
isolated coverage storage. Earlier per-target `fuzz/coverage` evidence remains
available. The runner refuses existing output/build directories and never deletes
old evidence.
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

## Decoder resource and reader target (2026-10-27)

`decoder_limits` is the twelfth target. Its shared implementation lives in
`tests/support/resource.rs`, so normal and standalone tests execute the same
contract. It compares buffered, collecting-reader and push acceptance, exact
native samples and metadata, error fusing, byte counters and image-pipeline
capacity. The callback used for optional CRC repair is independent of the native
codec. The PNG body follows a 20-byte control header; total input is at most
65,536 bytes.

| Header bytes | Meaning |
| --- | --- |
| 0..4 | Big-endian u32 input budget, capped at 1 MiB |
| 4..8 | Big-endian u32 output budget, capped at 8 MiB |
| 8..12 | Big-endian u32 metadata budget, capped at 64 KiB |
| 12..14 | Big-endian u16 chunk budget, capped at 4,096 |
| 14..16 | Big-endian u16 frame budget, capped at 256 |
| 16 | Bit 0: default-image recovery; bit 1: strict palette; bits 2..3: reader mode; bit 4: repair CRCs |
| 17 | Reader/push fragment size: 1 + byte modulo 32 |
| 18..20 | Big-endian u16 reader fault offset, capped at the PNG body length |

Reader modes select ordinary reads, alternating interruptions, an IO error or an
invalid returned byte count. Zero budgets are retained. Independent constructors
produce 6,376 structured seeds across all fifteen formats, plain/Adam7, static
and animation roles, limit boundaries and reader modes. Two highly compressed
1x4096 zero images expose a harness iteration-bound defect: an input-only bound
could exhaust while emitting valid rows without consuming more input. Native
buffered/reader/push decoding already passed. The corrected harness accounts for
the output budget; its failing regression, exact old helper snapshots and hashes
are retained in `reports/boundaries-harness-before.*` and
`reports/boundaries-{resource,budgets}-before.rs`. This was a harness correction.

The shared 740 independent mixed/one-bit DEFLATE fixtures add 2,220 valid,
suffixed and truncated seeds to each raw corpus. The archive combines raw-stream
and bounded zlib seeds into 5,379 initial `inflate_stream` inputs. That target
now varies one-, seven-, 31-byte and whole-input fragments, and checks native
counters, working capacity and terminal error behavior.

The initial ASan stage passes 30,000 resource and 20,000 raw-stream executions
(9 / 16 fuzzing seconds, 447 / 424 MiB peak RSS). After the tall-image regression
and harness correction, another 30,000 resource executions pass in 13 seconds /
445 MiB. `reports/boundaries-initial-source.json` maps the initial helper hashes
to retained snapshots; `reports/boundaries-source.json` records the corrected
source. Both stages verify native-crate feedback isolation from object sections
and retain ASan. All production PNG source files are unchanged. These 80,000
successful executions bring the historical total to 1,260,000 across twelve
targets. The expected failing regression and fixed coverage replay are excluded.
Encoder targets are outside this decoder continuation.

To reproduce with new destinations, generate seeds as above, then select the
resource/raw targets when archiving and running:

```sh
python crates/deadlib-image/png/fuzz/scripts/archive_native_seeds.py --corpus-root target/png-native-seeds-fresh --output target/png-resource-seeds-fresh.zip --targets decoder_limits inflate_stream
python3 crates/deadlib-image/png/scripts/fuzz_native.py --cargo-fuzz /path/to/cargo-fuzz --seed-archive target/png-resource-seeds-fresh.zip --target-dir /tmp/png-resource-fuzz-fresh --output-dir /tmp/png-resource-campaign-fresh --targets decoder_limits inflate_stream
python3 crates/deadlib-image/png/scripts/coverage_fuzz_native.py --campaign-output /tmp/png-resource-campaign-fresh --cargo-fuzz /path/to/cargo-fuzz --target-dir /tmp/png-resource-coverage-build-fresh --output-dir /tmp/png-resource-coverage-fresh
```

The two selection options default to the earlier `png_stream inflate_stream`
pair. Selected run counts are 50,000 PNG-stream, 20,000 raw-stream and 30,000
resource executions. Fixed replay counts are documented separately under
`../coverage-reports/`.

## Buffered checksum follow-up (2026-10-27)

The selected buffered Adler-32 lanes pass fresh native-feedback ASan runs of
50,000 `png_stream`, 20,000 `inflate_stream` and 30,000 `decoder_limits` inputs.
Initial corpus counts are 27,867 / 5,379 / 6,376, with exact literal-filename
digests in `reports/adler-seeds.json`. Fuzzing seconds / peak RSS are
68 / 542 MiB, 16 / 434 MiB and 13 / 484 MiB.

`reports/adler-source.json` retains source, binary and corpus hashes, compiler
invocations and object-section audits. PNG/shared feedback counters occupy
16,634 / 45 bytes; fourteen dependency archives and six selected harness objects
have no counters or PC tables. All three binaries define ASan's runtime entry.
The source check covers the selected buffered-only method; incremental checksum
updates retain their original scalar implementation. No encoder target runs.
These 100,000 successful executions bring the historical total to 1,360,000
across twelve targets. Compiler SIMD and comparative decoder timings are
documented in `../BENCHMARKS.md`; ASan runs are correctness evidence, not timings.

## Scalar metadata fields (2026-10-27)

`tests/support/scalar_fields.rs` adds 6,494 representative valid/invalid PNG
inputs from the 207,348-case field audit. Normal seed generation includes them
in both PNG corpora. A focused mode exports only these additions to a fresh root:

```sh
cargo run -p deadlib-image-png --release --example seed_fuzz -- --scalar-fields-only /tmp/png-scalar-seeds-fresh
python3 crates/deadlib-image/png/fuzz/scripts/archive_native_seeds.py --corpus-root /tmp/png-scalar-seeds-fresh --output /tmp/png-scalar-seeds-fresh.zip --targets png_stream --include-archive /path/to/retained-base-seeds.zip
```

The archive helper imports selected targets from earlier immutable ZIPs, rejects
duplicate names, and records input archive hashes. Literal UTF-8 filename sorting
and fixed ZIP metadata remain deterministic. This avoids reopening thousands of
cross-filesystem seed files. A separate full seed-generation run produces the
identical combined PNG ZIP; `reports/scalar-fields-reproduce.*` retains the proof.

The combined PNG corpus starts at 34,361 files / 7,204,344 bytes. Its native-only
ASan campaign passes 50,000 executions in 70 fuzzing seconds / 528 MiB peak RSS.
Object audits verify fourteen dependency archives and two harness objects have
no feedback counters/PC tables; both native archives retain feedback. The binary
defines ASan's runtime entry. `reports/scalar-fields-*` retains generator/source/
binary/corpus hashes, commands and logs. Production PNG code and the selected
PNG-stream binary are unchanged from the checksum milestone. These runs bring
the historical total to 1,410,000 across twelve targets. No encoder target runs.

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

`png_stream` checks each input in three configurations: repaired CRCs with
strict animation handling, repaired CRCs with APNG recovery, and original CRCs
with recovery. It compares whole-file acceptance and native row samples under
varying input splits. Valid animation results must match the strict buffered
decoder; recovered static images must match independent buffered decoding after
removing only APNG chunks. Previously emitted animation rows are discarded on
recovery. Indexed policy uses input-length parity, allowing both policies with
an intact PNG signature. The APNG fixtures add 3,060 valid/fault files per PNG
corpus; that milestone's fresh PNG corpora contain 9,351 files each.
The fresh APNG recovery campaigns complete 20,000 inputs each on `png_stream`,
`decode` and `encode_stream` under Linux ASan. The three configurations within
`png_stream` count as one input execution. Exact source/corpus/binary hashes,
commands and statistics are in `reports/apng-recovery-source.json` and its three
target logs. These smoke runs do not establish native-only coverage.

HDR seeds add another 8,534 independent files per PNG corpus: every byte value
at all 32 mDCV/cLLI payload-byte positions, 60 integer boundaries, 30 format/
interlace cases, 132 length cases and 120 chunk-order permutations. Current
fresh PNG corpora at that milestone contain 17,885 files / 3,551,398 bytes each. Native metadata
fixtures use independent stored-DEFLATE and CRC construction; the HDR integration
suite independently compares exact C and Rust fixed-point metadata. The fresh
30,000-input campaigns on each PNG target are retained in
`reports/hdr-source.json` and its two target logs.

The fcTL-before-acTL fix adds 1,560 independent valid/fault prefix files and all
42 pinned WPT APNG/static-reference PNGs to each PNG corpus. Current fresh
corpora contain 19,487 files / 4,090,790 bytes each, maximum 63,435 bytes. Both
PNG targets now bound native output at 8 MiB and animation count at 256 so the
128-frame WPT case reaches its frame data. The Rust png reference is bounded at
16 MiB, with an 8 MiB output-buffer cap. Full commands, hashes and two fresh
30,000-input ASan campaigns are retained in `reports/apng-order-source.json`.

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


## Structured Latin-1 text-body target

`text_body` consumes a flag byte plus at most 4,096 body bytes. Flags select
plain/compressed text, placement and Stored/Fast construction. Independent
stored-zlib/CRC fixtures verify non-null decoder recovery separately from strict
encoder repertoire. It checks typed construction, raw-metadata prewrite
validation, both writer outputs, exact metadata/pixels and Rust png values.
Invalid encoder inputs must leave the destination empty. The initial 4,304
files cover all byte/flag choices, empty text, 79-/80-/4096-byte graphic bodies,
late controls and embedded NUL. `reports/text-body-source.json` records the
20,000-run target campaign plus 20,000 fresh decode and streaming runs with the
512 added PNG-body seeds. These 60,000 successful ASan executions are included
in the historical total; the expected failing pre-fix regression is not.

## Reference sample precision campaign

Fresh Linux ASan runs pass 20,000 `reference_samples`, 30,000 `encode_stream`,
30,000 `png_stream` and 30,000 `decode` inputs. Fuzzing seconds / peak RSS are
48 / 404 MiB, 128 / 417, 44 / 654 and 6 / 464 respectively. All four binaries
define the ASan runtime entry and final source hashes match their initial
snapshot. `reports/reference-samples-source.json` records commands, fresh corpus
digests, source hashes and instrumented binary hashes; full per-target logs are
retained alongside it. The PNG corpora each begin with 19,743 files, including
256 new reference-converted images. These 110,000 successful executions bring
the historical count to 860,000 across ten targets. This remains smoke evidence;
native-only coverage and longer campaigns remain open.

## Decoder ancillary ordering follow-up

The decoder-focused continuation adds 7,884 independent Table 7 PNG fixtures to
each decoder corpus: all 2,052 single placements across native formats/optional
palettes and 5,832 ordered-pair placements for RGB8 with and without PLTE. Valid
and malformed orderings are both retained. Each fresh corpus has 27,627 files /
5,758,804 bytes. Fresh ASan runs pass 30,000 `png_stream` inputs in 30 fuzzing
seconds / 523 MiB peak RSS and 30,000 `decode` inputs in 6 seconds / 470 MiB.
Both binaries define the ASan runtime entry and final source hashes match the
initial snapshot. `reports/ancillary-order-source.json` and per-target logs
retain exact commands, source/corpus/binary hashes and diagnostics.

The 60,000 successful decoder executions bring historical campaigns across all
ten targets to 920,000, including prior encoder runs. Decoder conformance,
precision, APNG rendering and decoder performance are the clarified deliverable;
encoder features and speed are outside its completion requirements. Native-only
feedback coverage and longer campaigns remain open.

## Independent literal-value decoder follow-up

`seed_fuzz --primary-pairs-only FRESH_ROOT` exports 644 independently assembled
streams to each of `zlib` and `inflate_stream`. The normal full generator also
includes them. Representative diagonal/complement byte pairs, complete trees,
unequal widths, zero/one padding and canonical chains through fifteen bits
supplement the exhaustive 65,668-case regular/C regression suite.

`reports/primary-pairs-seed-build.json` records focused and full generation.
The focused archive merged with the earlier immutable raw seeds exactly equals
the full-generator archive: 6,667 files / 1,254,614 bytes, with literal UTF-8
filename ordering. The fresh `inflate_stream` ASan campaign passes 20,000
executions in thirteen fuzzing seconds / 465 MiB peak RSS. Both native image
archives retain feedback; fourteen dependencies and two harness objects have
no feedback counters or PC tables. The binary matches the checksum milestone.
`reports/primary-pairs-source.json` and `primary-pairs-inflate_stream.log` retain
commands, exact sources/binary/corpus hashes and object audits.

These are decoder checks. The two paired-literal runtime trials are rejected
and all production files remain unchanged. Historical successful sanitizer
executions reach 1,430,000 across twelve targets, including older encoder runs;
this milestone runs only the raw-stream decoder target.

## APNG control-field seeds

`seed_fuzz --animation-fields-only FRESH_ROOT` exports 10,740 independent files
to each PNG decoder target; normal full generation includes the same files.
The seeds sample control lengths, zero/31-bit/high-bit integers, frame-count
limits, geometry, sequences, byte enums, timing words and empty fdAT chunks
across all sixty format/interlace/default-role configurations. They supplement
the 211,532-file normal control/delay audit and Rust png known-value comparison.

`reports/animation-fields-seed-build.json` records full and focused generation.
The focused archive merged with the immutable scalar-field PNG archive exactly
equals the full-generator archive: 45,101 files / 10,776,415 bytes, ordered by
literal UTF-8 filename bytes. `reports/animation-fields-source.json` and the raw
PNG-stream log retain the fresh ASan campaign, exact sources/binary/corpus hashes
and native/dependency/harness feedback audits. Production decoder files are
unchanged; no encoder target is part of this milestone.
