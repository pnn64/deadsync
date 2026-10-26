# Test-only libvorbis oracle

This process links unmodified libvorbis/libogg and checks raw headers and
codebooks. Its codebook mode deliberately uses libvorbis's private APIs to
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
```

Skip clones already present. Use platform executable paths and omit `-A x64`
for other generators. Pinned source commits are in `COVERAGE.md`.

The corpus helpers currently require single-stream Ogg Vorbis files; this is a
test-harness limitation, not evidence for multiplexed/chained Vorbis decoding.
No PCM parity is claimed by these header/codebook oracles.
