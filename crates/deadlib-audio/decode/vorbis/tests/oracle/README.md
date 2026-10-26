# Test-only libvorbis oracle

This process links unmodified libvorbis/libogg and checks raw headers, codebooks,
inverse MDCT and packet PCM. Its codebook mode uses libvorbis's private APIs to
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
```

Skip clones already present. Use platform executable paths and omit `-A x64`
for other generators. Pinned source commits are in `COVERAGE.md`.

The corpus helpers currently require single-stream Ogg Vorbis files; this is a
test-harness limitation, not evidence for multiplexed/chained Vorbis decoding.
The `pcm` mode receives length-delimited raw packets, assigns unknown granules
and clears EOS, then emits unclipped, untrimmed planar PCM. It verifies raw
overlap frame counts independently, including the zero-frame priming packet.
It does not verify the Ogg Vorbis initial/final sample mapping. `mdct` tests the
reference inverse transform directly, independently of headers or packet decode.

For the external historical floor-zero fixture (never redistributed in this
repository):

```powershell
$env:DEADLIB_VORBIS_FLOOR0 = "$pwd/target/deadlib-audio-reference/corpus/floor0/vorbis_floor_type_0.ogg"
Invoke-WebRequest 'https://samples.ffmpeg.org/A-codecs/vorbis/floor_type_0/vorbis_floor_type_0.ogg' -OutFile $env:DEADLIB_VORBIS_FLOOR0
cargo test -p deadlib-audio-decode-vorbis --release --test pcm historical_floor0 -- --ignored --nocapture
```

Create the destination directory before download. Expected SHA-256:
`1c97545f93cc2afe473066d523f15bb04edc68fe7474bc98408bda452e462ea0`.
This file has known nonconforming container granules. Its raw test deliberately
uses the independent Rust Ogg development dependency to extract codec packets,
without relaxing the native container's strict validation. See COVERAGE.md for
the separate floor-zero numerical criteria and unresolved legacy container work.
