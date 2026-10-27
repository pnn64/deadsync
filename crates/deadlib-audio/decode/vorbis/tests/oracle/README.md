# Test-only libvorbis oracle

This process links unmodified libvorbis/libogg and checks raw headers, codebooks,
forward/inverse MDCT, packet PCM and full Ogg PCM via libvorbisfile. Its codebook mode uses libvorbis's private APIs to
unpack VQ values and encode all used Huffman codewords. The Rust production
graph never links either reference implementation.

From the repository root (PowerShell):

```powershell
git clone --depth 1 --branch v1.3.6 https://github.com/xiph/ogg.git target/deadlib-audio-reference/ogg
git clone --depth 1 --branch v1.3.7 https://github.com/xiph/vorbis.git target/deadlib-audio-reference/vorbis
$referenceRoot = (Resolve-Path target/deadlib-audio-reference).Path
cmake -S crates/deadlib-audio/decode/vorbis/tests/oracle -B target/deadlib-audio-reference/vorbis-oracle-build "-DREFERENCE_ROOT=$referenceRoot" -A x64
cmake --build target/deadlib-audio-reference/vorbis-oracle-build --config Release --target deadlib_vorbis_oracle --parallel 4
$env:DEADLIB_LIBVORBIS_ORACLE = (Resolve-Path target/deadlib-audio-reference/vorbis-oracle-build/Release/deadlib_vorbis_oracle.exe).Path
$env:DEADLIB_VORBIS_CORPUS = (Resolve-Path assets).Path
cargo test -p deadlib-audio-decode-vorbis --release --test libvorbis -- --ignored --nocapture
cargo test -p deadlib-audio-decode-vorbis --release --test headers -- --ignored --nocapture
$env:DEADLIB_VORBIS_REPORT = "$pwd/crates/deadlib-audio/decode/vorbis/validation/raw-pcm.tsv"
cargo test -p deadlib-audio-decode-vorbis --release --test pcm -- --ignored --skip historical_floor0 --skip regenerate_surround_pcm_golden --nocapture
$env:DEADLIB_VORBIS_MAPPING_REPORT = "$pwd/crates/deadlib-audio/decode/vorbis/validation/ogg-pcm.tsv"
cargo test -p deadlib-audio-decode-vorbis --release --test ogg_mapping -- --include-ignored --nocapture
$env:DEADLIB_VORBIS_SEEK_REPORT = "$pwd/crates/deadlib-audio/decode/vorbis/validation/seeks.tsv"
cargo test -p deadlib-audio-decode-vorbis --release --test seeking -- --include-ignored --skip historical_floor0 --nocapture
```

Skip clones already present. Use platform executable paths and omit `-A x64`
for other generators. Pinned source commits are in `COVERAGE.md`.

The corpus helpers require single-stream Ogg Vorbis files. Generated mapping
tests separately exercise multiplexing natively and mixed-rate chains against
libvorbisfile. This is a test-harness limitation, not a production restriction.
The `pcm` mode receives length-delimited raw packets, assigns unknown granules
and clears EOS, then emits unclipped, untrimmed planar PCM. It verifies raw
overlap frame counts independently, including the zero-frame priming packet.
It does not verify the Ogg Vorbis initial/final sample mapping. `mdct` tests the
reference inverse transform directly, independently of headers or packet decode.

`pcm-cases` reads the same three length-prefixed headers, then a u64 case count.
Each case starts with a u64 packet count followed by length-prefixed raw packets.
It restarts synthesis between cases and resets packet numbering to three.
Output starts with channels (u32) and case count (u64), then the existing `P`/`S`
records for each packet, grouped by case. The prefix/recovery suite checks this
mode against fresh-process `pcm` runs byte for byte; see [PARITY.md](../../PARITY.md).
`mdctforward` receives N time samples instead of N/2 coefficients and emits the
N/2 coefficients from `mdct_forward`, with the same length-prefixed protocol.

`oggpcm` reads an actual Ogg file, reports each link's serial, channels, rate and
`ov_pcm_total`, and emits planar `ov_read_float` records tagged by link. The Rust
test checks both decoded and declared reference lengths before comparison.
After querying metadata, the oracle explicitly seeks to PCM zero, which also
avoids libvorbisfile skipping the first short link at its raw-open position.
Zero-total files have no PCM seek target and produce no sample records. This
mode verifies final counts, trimming and mixed-rate chains separately from raw
synthesis. Symphonia's format-reader path checks timestamps, durations and PCM.

`oggpcmraw` has the same output protocol but establishes its initial position
with `ov_raw_seek(0)`. The native encoder matrix uses this path. A named
regression shows `ov_pcm_seek(0)` losing the first 4,096 frames of a legal
eight-channel/N8192 native stream whose 65,810-byte priming packet spans pages;
raw-start decoding returns all 16,401 declared frames. The original `oggpcm`
mode and existing decoder corpus tests are unchanged. See [ENCODING.md](../../ENCODING.md).

`oggseek input.ogg requests.bin` opens libvorbisfile once, then performs repeated
`ov_pcm_seek`/`ov_pcm_tell`/`ov_read_float` operations without reopening. Requests
contain a little-endian u64 count followed by u64 global retained-frame positions
and u64 requested output lengths. Output starts with link metadata as in `oggpcm`,
then the actual PCM position and planar records for each request. Rust checks
exact positions/counts and justified PCM error bounds. Positive broadcast-offset
position disagreement is kept in a named regression, not hidden by tolerances.
The Symphonia seek test explicitly requests a maximum block of pre-roll, resets
the codec, and discards by packet timestamp, as required for sample precision
with its packet-level `FormatReader` seek API.

For the external historical floor-zero fixture (never redistributed in this
repository):

```powershell
$env:DEADLIB_VORBIS_FLOOR0 = "$pwd/target/deadlib-audio-reference/corpus/floor0/vorbis_floor_type_0.ogg"
Invoke-WebRequest 'https://samples.ffmpeg.org/A-codecs/vorbis/floor_type_0/vorbis_floor_type_0.ogg' -OutFile $env:DEADLIB_VORBIS_FLOOR0
cargo test -p deadlib-audio-decode-vorbis --release --test pcm historical_floor0 -- --ignored --nocapture
cargo test -p deadlib-audio-decode-vorbis --release --test seeking historical_floor0 -- --ignored --nocapture
```

Create the destination directory before download. Expected SHA-256:
`1c97545f93cc2afe473066d523f15bb04edc68fe7474bc98408bda452e462ea0`.
This file has known nonconforming container granules. Its raw test deliberately
uses the independent Rust Ogg development dependency to extract codec packets,
without relaxing the native container's strict validation. See COVERAGE.md for
the separate floor-zero numerical criteria. [LEGACY.md](../../LEGACY.md) covers
the explicit native container option and separate indexed endpoint policy.
The seek test repacketizes unchanged floor-zero codec packets with strict native
granules; it validates floor-zero pre-roll without accepting the legacy container.

## Pinned upstream coded test grid

The `deadlib_vorbis_upstream_vectors` target compiles the pinned upstream
`test/test.c`, `test/util.c`, and `test/write_read.c` unchanged. A force-included
test shim replaces only successful-fixture removal, retaining the generated
files. Upstream's own encode/decode amplitude checks still run. This is upstream
coded test material, not a claim of an exhaustive formal conformance suite.

```powershell
crates/deadlib-audio/decode/scripts/generate_upstream_vectors.ps1
# Set this to the fresh directory printed by that command:
$env:DEADLIB_VORBIS_UPSTREAM_CORPUS = 'target/deadlib-audio-reference/corpus/upstream-<id>'
$env:DEADLIB_VORBIS_UPSTREAM_REPORT = "$pwd/crates/deadlib-audio/decode/vorbis/validation/upstream.tsv"
cargo test -p deadlib-audio-decode-vorbis --release --test upstream -- --ignored --nocapture
```

The script verifies both reference commits and unmodified tracked sources,
builds the target, requires 528 outputs, and retains `generation.log`,
`assets.sha256`, and `provenance.json`. Each run uses a new directory. The grid
covers 1..=8 channels, six rates (16/22.05/32/44.1/48/96 kHz), and eleven quality
settings from -0.5 through 9.5. Every file has 2,048 source frames. Native tests
compare full-file PCM/counts to libvorbisfile, five exact native seeks per file,
metadata, and 17-byte incremental input. The checked-in evidence records this
Windows build's hashes; floating-point encoders on other platforms may produce
different valid bitstreams and must be validated by the numerical tests.

External historical and ITGmania song files have a separate hash/size-pinned
manifest in `validation/corpus.json`. Fetch/verify them with Python 3.11+:

```powershell
python crates/deadlib-audio/decode/scripts/fetch_corpus.py
# Or select a group: --group itgmania / --group floor0 / --group legacy / --group ffmpeg
```

Media stays under ignored `target/deadlib-audio-reference/corpus`; it is never
redistributed in the crate. The manifest pins the upstream ITGmania Git commit
and paired SSC charts, distinguishing shipped songs from theme sounds. The
native diagnostic example reports parameters, counts and strict failures:

```powershell
cargo run -p deadlib-audio-decode-vorbis --release --example inspect -- path/to/song.ogg
```

To validate the original historical and shipped-song containers with explicit
options, set the oracle variable above, fetch the manifest, then run:

```powershell
$env:DEADLIB_VORBIS_EXTERNAL_CORPUS = "$pwd/target/deadlib-audio-reference/corpus"
$env:DEADLIB_VORBIS_LEGACY_REPORT = "$pwd/crates/deadlib-audio/decode/vorbis/validation/legacy.tsv"
cargo test -p deadlib-audio-decode-vorbis --release --test legacy -- --include-ignored --nocapture
```

The diagnostic accepts `--allow-zero-span-granules` and
`--trim-to-final-granule` separately. The second uses indexed decoding because a
forward-only reader cannot retract PCM emitted before a decreasing EOS granule.
Reports keep reference declared/emitted counts separate for the malformed song.

The additional `--test historical` suite uses the `ffmpeg` group with the same
external-corpus and oracle variables. It checks strict floor-zero and six-channel
aoTuV files against both independent decoders, streaming/reset/seek PCM bits,
and five named rejection cases. Set `DEADLIB_VORBIS_HISTORICAL_REPORT` to retain
its PCM table. See [HISTORICAL.md](../../HISTORICAL.md) for commands and results.
