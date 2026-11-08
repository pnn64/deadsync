# Exact FLAC decoder parity

The decoder preserves signed, right-aligned integer PCM. Independent comparisons
use exact samples, rates, bit depths, channel counts, frame lengths and sample
positions. No float conversion, sample tolerance, resampling or channel remixing
is involved. Native FLAC, independent raw frames and Ogg FLAC share the same
frame decoder. Whole sequential reads verify a supplied STREAMINFO MD5; partial
seeks retain frame/page checksums and explicitly report partial integrity.
Performance changes rerun these gates; current source-regression records are
linked from [PERFORMANCE.md](PERFORMANCE.md).

## Current evidence, 2026-10-27

| Gate | Result |
| --- | --- |
| Official valid corpus, libFLAC 1.5.0 | All 75 files exact, including two headerless streams |
| Official native file seeks | 1,533 borrowed-input and 1,533 file-backed positions exact |
| Official Ogg remuxes | All 73 native-source files exact with libFLAC; 1,533 indexed seeks exact |
| Official valid corpus, FFmpeg 9.0.2 | 74 exact; one named reference rejection below |
| Official Ogg remuxes, FFmpeg 9.0.2 | 72 exact; same named reference rejection |
| Explicit independent wire families | All 1,279 cases exact with native decoding, libFLAC and FFmpeg |
| Accepted raw-frame fuzz replay | All 2,162 distinct normalized frames exact with libFLAC and FFmpeg |
| Generated native cases | 696 libFLAC-encoded inputs decode exactly; native-generated files also match libFLAC |
| Generated Ogg cases | 696 exact bidirectional libFLAC cases; Symphonia's Ogg path matches all 696 |
| Fresh decoder ASan campaigns | 173,174 executions, all four targets exit zero |

The official corpus also covers changing rates/channels/depths, pre-2007 variable
numbering, every subframe type, maximum block and Rice partition orders, and
metadata extremes. All 69 available source MD5s verify. The four changing-property
files have no MD5; two headerless files have no STREAMINFO. Per-file evidence is
in [native-corpus.tsv](validation/native-corpus.tsv),
[native-seeking.tsv](validation/native-seeking.tsv),
[ogg-corpus.tsv](validation/ogg-corpus.tsv) and
[ffmpeg-corpus.tsv](validation/ffmpeg-corpus.tsv).

The explicit wire families are assembled directly from fields rather than
relying on an encoder to choose a mode. They contain:

- 957 verbatim cases: every depth 4..32, assignments 0..10, and no/one/maximum
  wasted bits, with signed extremes and exact stereo decorrelation.
- 60 fixed-predictor cases: orders 0..4, both residual methods, partition orders
  0..2 and escaped/unescaped residuals.
- 256 LPC cases: every order 1..32, both residual methods, coefficient precisions
  1 and 15, and shifts 0 and 15.
- Six wide LPC cases: 32-bit stereo, all three decorrelation modes, 32-tap
  15-bit coefficients, shifts 0 and 15, and full 33-bit side-channel extremes.

Every wire file carries an independently accumulated expected-PCM MD5. Native
sequential decoding verifies it; libFLAC verifies it independently. The C oracle
comparisons also check every emitted sample and its declared properties.
See [wire-parity.tsv](validation/wire-parity.tsv).

The raw-frame fuzz replay examines the retained fresh campaign corpus, decodes
accepted frames and deduplicates exact normalized bytes. It replaces only the
coded starting number with zero, recomputes header/frame CRCs and wraps each raw
frame in metadata describing its actual properties and PCM MD5. Subframe syntax
and bytes remain unchanged. All 2,162 resulting files pass both independent
decoders exactly; see [fuzz-frame-parity.tsv](validation/fuzz-frame-parity.tsv).
This gate excludes non-audio sample-rate-zero data because FFmpeg's audio demux
API cannot represent it. Native tests cover rate zero separately. It supplements
the full-stream checks rather than proving a mutated source's metadata/sequence
validity. Its [source corpus hashes](validation/fuzz-frame-source.json) are recorded separately.

## Reference differences

FFmpeg 9.0.2 rejects `uncommon/04 - changing bitdepth.flac` with
`switching bps mid-stream is not supported`, for native and remuxed Ogg input.
The accepted prefix is checked exactly and the rejection is pinned by filename
and diagnostic. Native decoding of that entire file matches libFLAC exactly.
No additional FFmpeg rejection or PCM mismatch is accepted by the gate.

Symphonia 0.6.1 has three named official-corpus differences: changed channel
assignment rejection, a 32-bit mid-side PCM mismatch, and a 768 kHz rejection.
Its generated native matrix has two separately pinned 32-bit stereo differences.
The full native PCM remains exact with libFLAC; the official 32-bit file is also
exact with FFmpeg. Details and first differing samples are in
[COVERAGE.md](COVERAGE.md) and [ENCODING.md](ENCODING.md). A checked-arithmetic
Symphonia LPC panic and the reference-only release-arithmetic policy for fuzzing
are documented in [fuzz/README.md](fuzz/README.md).

These reference defects are not compatibility modes. The native decoder follows
RFC 9639 and exact integer reconstruction. Error concealment, arbitrary corrupt
stream recovery and matching every implementation's malformed-input decisions
are separate from valid-stream PCM parity. Native streaming is strict;
`FrameDecoder::find_frame` provides explicit CRC-validated headerless recovery.
The official malformed corpus safely terminates, with eight errors and three
advisory-metadata cases that decode and verify their MD5.

## FFmpeg oracle

The test-only C program uses libavformat's FLAC/Ogg demuxers and libavcodec's
native FLAC decoder. It reads AVFrames directly, handles packed/planar S16/S32,
and shifts left-aligned samples back to the original precision. It emits the
same rate/channels/depth/block/planar-i32 protocol as the libFLAC oracle. It uses
one decoder thread, CRC checks and strict frame errors. Initial invalid-data
packets without STREAMINFO can be skipped before the first decoded frame, matching
the libFLAC oracle's initial LOST_SYNC recovery for the official garbage-prefixed
headerless vector. Errors after decoding starts remain failures.

Pinned reference: FFmpeg 9.0.2, source tag commit
`946fcce07b6dcd0331c8cc609192aeff5e1924f8`, from the
[official source release](https://ffmpeg.org/releases/ffmpeg-9.0.2.tar.xz).
Archive SHA-256:
`8c3850283eb25fa026482078a04051e0be17347b09ef81a0849bec15a96e002e`.
The minimal build enables only the required decoder/parser/demuxers/file protocol,
with external dependencies disabled. No reference codec enters production.
Tool and source hashes are in [parity-manifest.json](validation/parity-manifest.json).

## Reproduce

Prepare the official corpus and libFLAC oracle as described in COVERAGE.md, with
Ogg support as described in OGG.md. On Linux with GCC/make/curl/tar, build a fresh
FFmpeg oracle (the output directory must not exist):

```sh
bash crates/deadlib-audio/decode/flac/tests/ffmpeg/build.sh /tmp/flac-ffmpeg-parity
```

For the recorded Windows/WSL setup, from the repository root:

```powershell
wsl -d Debian -- bash /mnt/c/GitHub/deadsync/crates/deadlib-audio/decode/flac/tests/ffmpeg/build.sh /tmp/flac-ffmpeg-parity
$env:DEADLIB_FLAC_FFMPEG_WSL = 'Debian'
$env:DEADLIB_FLAC_FFMPEG_ORACLE = '/tmp/flac-ffmpeg-parity/ffmpeg-flac-oracle'
$env:DEADLIB_FLAC_ORACLE = (Resolve-Path target/deadlib-flac-reference/oracle-build/Release/deadlib_flac_oracle.exe).Path
$env:DEADLIB_FLAC_CORPUS = (Resolve-Path target/deadlib-flac-reference/conformance).Path
$env:DEADLIB_FLAC_GENERATED = "$pwd/target/deadlib-flac-reference/generated"
$env:DEADLIB_FLAC_FFMPEG_REPORT = "$pwd/crates/deadlib-audio/decode/flac/validation/ffmpeg-corpus.tsv"
$env:DEADLIB_FLAC_WIRE_REPORT = "$pwd/crates/deadlib-audio/decode/flac/validation/wire-parity.tsv"
# Use the actual retained raw-frame campaign directory from fuzz/run.py.
$env:DEADLIB_FLAC_RAW_FUZZ_CORPUS = "$pwd/target/deadlib-flac-reference/fuzz-decoder-parity-20261027/corpus/frame"
$env:DEADLIB_FLAC_FUZZ_FRAME_REPORT = "$pwd/crates/deadlib-audio/decode/flac/validation/fuzz-frame-parity.tsv"
cargo test -p deadlib-audio-decode-flac --target-dir target/deadlib-flac --release --test decoder_parity -- --include-ignored --test-threads=1
```

For native Linux execution omit `DEADLIB_FLAC_FFMPEG_WSL` and set the oracle
executable path directly. The test fails when prerequisites are missing. The
mutated corpus is retained under ignored scratch storage; regenerate a fresh
campaign using the fuzz runner and preserve its hashes. A fresh time-bounded
campaign need not produce the identical mutated corpus or replay count.

Local gates are `cargo test -p deadlib-audio-decode-flac`, `cargo fmt -p
deadlib-audio-decode-flac -- --check` and Clippy with `--all-targets -- -D warnings`.
Current evidence establishes exact parity for these standards and differential
gates. It does not claim exhaustive proof for every possible bitstream,
universal corrupt-file recovery, or measured performance parity.
