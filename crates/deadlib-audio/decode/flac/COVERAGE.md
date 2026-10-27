# Coverage and reproducible validation

## Native decoder milestone, 2026-10-27

Implemented against RFC 9639 sections 8 and 9, with pre-2007 variable blocking
compatibility from appendix B.1. Native, raw frame and borrowed-input seek APIs
are available. The current production graph uses only the sibling Ogg container. Symphonia
and the separately compiled C oracle are test-only.

Local gate: `cargo test -p deadlib-audio-decode-flac`, 13 integration tests and
one documentation example. `cargo clippy -p deadlib-audio-decode-flac --all-targets
-- -D warnings` is also required. Toolchain: rustc 1.99.1, cargo 1.99.1,
Windows x86_64 MSVC. Rust package declares MSRV 1.97; a separate MSRV run has not
yet been performed.

The local vectors exercise every sample depth 4..32, channel assignment 0..10,
all rate/block/depth header codes, coded-number boundaries through 36 bits,
constant/verbatim/fixed orders 0..4/LPC orders 1..32, Rice methods 0/1, extreme
residuals, escaped zero-width partitions, wasted bits, 33-bit side signals,
checksums, metadata and stream truncations. Streaming tests use every short-read
size for two known-PCM streams; every sample position is sought forwards and
backwards. Invalid digest, short frame, count and byte-budget cases are checked.
This matrix does not replace the outstanding fuzz campaign or encoder tests.

## External references

The current three-reference decoder parity gate, explicit wire cases and
accepted fuzz-frame differential replay are documented in [PARITY.md](PARITY.md).

- [Official CELLAR FLAC corpus](https://github.com/ietf-wg-cellar/flac-test-files),
  commit `aa7b0c6cf32994c106ae517a08134c28a96ff5b2`, CC0. Corpus stays in ignored
  `target/deadlib-flac-reference/conformance`; no large audio payloads are vendored.
- [libFLAC 1.5.0](https://github.com/xiph/flac/releases/tag/1.5.0), source commit
  `1507800de4b70e21be71f38caa0d9079d0bc6e45`. The C oracle links the unmodified
  source library and emits frame properties followed by planar right-aligned
  signed i32 values. Its callback uses libFLAC's public decoder API only.
- Symphonia 0.6.1, exact-pinned dev dependency. Raw encoded frames are passed
  to its decoder using STREAMINFO as extra data, and its left-aligned i32 output
  is shifted to the original bit depth before exact comparison. This checks the
  reference codec, not its container demuxer.

All 75 valid corpus files match libFLAC sample for sample, including changing
audio properties, old-format variable blocks, maximum block/partition orders,
unusual depths/rates and metadata extremes. For 69, the original STREAMINFO MD5
also verifies. Four changing-property streams have no MD5; two are headerless.
The 73 native streams are each sought to 21 positions, totaling 1,533; returned
PCM and exact start positions are checked against libFLAC's linear output.
The combined metadata vector uses an explicit 128 MiB budget.

All 11 faulty files terminate without a panic: eight are rejected; three decode
and verify their MD5 while disregarding incorrect advisory frame size, channel
count or depth in STREAMINFO. Actual frame properties always govern buffer sizes.
This behavior is explicit in the test assertions; RFC 9639 leaves handling of
malformed files unspecified.

Symphonia matches 72 files exactly. Three named differences are asserted:

| File | Symphonia 0.6.1 result | Native evidence |
| --- | --- | --- |
| uncommon/02 increasing channels | Rejects changed channel assignment | Exact libFLAC PCM |
| uncommon/05 32-bit audio | First mismatch: channel 0, sample 16384, mid-side; expected 1340429, reference 1446772 | Exact libFLAC PCM and verified source MD5 |
| uncommon/06 768 kHz | Rejects sample rate | Exact libFLAC PCM and verified source MD5 |

No tolerance is applied to native PCM. A changed or additional reference
difference fails the gate. Per-file results are in
[validation/native-corpus.tsv](validation/native-corpus.tsv).
Input sizes, SHA-256 hashes and tool versions are recorded in
[validation/native-corpus-manifest.json](validation/native-corpus-manifest.json).

## Reproduce

From the repository root in PowerShell (skip clones that already exist):

```powershell
git clone https://github.com/ietf-wg-cellar/flac-test-files.git target/deadlib-flac-reference/conformance
git -C target/deadlib-flac-reference/conformance checkout aa7b0c6cf32994c106ae517a08134c28a96ff5b2
git clone --branch 1.5.0 https://github.com/xiph/flac.git target/deadlib-flac-reference/flac
git -C target/deadlib-flac-reference/flac checkout 1507800de4b70e21be71f38caa0d9079d0bc6e45
$reference = (Resolve-Path target/deadlib-flac-reference/flac).Path
cmake -S crates/deadlib-audio/decode/flac/tests/oracle -B target/deadlib-flac-reference/oracle-build "-DREFERENCE_ROOT=$reference" -A x64
cmake --build target/deadlib-flac-reference/oracle-build --config Release --target deadlib_flac_oracle --parallel 4
$env:DEADLIB_FLAC_CORPUS = (Resolve-Path target/deadlib-flac-reference/conformance).Path
$env:DEADLIB_FLAC_ORACLE = (Resolve-Path target/deadlib-flac-reference/oracle-build/Release/deadlib_flac_oracle.exe).Path
$env:DEADLIB_FLAC_REPORT = "$pwd/crates/deadlib-audio/decode/flac/validation/native-corpus.tsv"
$env:DEADLIB_FLAC_SEEK_REPORT = "$pwd/crates/deadlib-audio/decode/flac/validation/native-seeking.tsv"
cargo test -p deadlib-audio-decode-flac --release --test conformance -- --ignored --nocapture
```

On other platforms omit `-A x64` and use the generator's executable path.
The corpus tests are marked ignored because they require external artifacts;
when explicitly run, missing artifacts are failures, never silent skips.

## Remaining scope

Native encoding is covered in [ENCODING.md](ENCODING.md) and Ogg mapping,
streaming, encoding and indexed file seeking in [OGG.md](OGG.md).
Native file-backed seeking and validated SEEKTABLE acceleration are covered in
[SEEKING.md](SEEKING.md), including another 1,533 exact official-corpus seeks.
Recorded ASan/libFuzzer campaigns are described in [fuzz/README.md](fuzz/README.md).
Decode benchmarks and measured optimizations are recorded in [PERFORMANCE.md](PERFORMANCE.md).
Subframes decode into reusable i32 PCM planes with checked i64 arithmetic;
only declared 33-bit stereo side planes use separate i64 storage. The
borrowed-input decoder's initial forward seek
scans/decompresses frames up to its target, and the lazy index retains one entry
per visited frame. Whole-stream MD5 is unavailable after a nonzero seek until a
new sequential pass from sample zero completes.
