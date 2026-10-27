# Test-only libogg process oracle

The helper links unmodified libogg and emits page/packet records. Native Rust
remains dependency-free. Optional oracle tests fail if invoked without the
required executable/corpus; ordinary tests explicitly mark them ignored.

From the repository root (PowerShell):

```powershell
git clone --depth 1 --branch v1.3.6 https://github.com/xiph/ogg.git target/deadlib-audio-reference/ogg
$referenceRoot = (Resolve-Path target/deadlib-audio-reference).Path
cmake -S crates/deadlib-audio/decode/ogg/tests/oracle -B target/deadlib-audio-reference/ogg-oracle-build "-DREFERENCE_ROOT=$referenceRoot" -A x64
cmake --build target/deadlib-audio-reference/ogg-oracle-build --config Release --target deadlib_ogg_oracle --parallel 4
$env:DEADLIB_LIBOGG_ORACLE = (Resolve-Path target/deadlib-audio-reference/ogg-oracle-build/Release/deadlib_ogg_oracle.exe).Path
$env:DEADLIB_OGG_CORPUS = (Resolve-Path assets).Path
python crates/deadlib-audio/decode/scripts/fetch_corpus.py --group ffmpeg
$env:DEADLIB_OGG_EXTERNAL_CORPUS = "$pwd/target/deadlib-audio-reference/corpus"
cargo test -p deadlib-audio-decode-ogg --release --test libogg -- --ignored --nocapture
cargo test -p deadlib-audio-decode-ogg --release --test seeking -- --include-ignored --nocapture
```

Use the platform's executable path and omit `-A x64` for other generators.
The v1.3.6 reference tag is recorded with the run evidence in `COVERAGE.md`.
The corpus test checks every Ogg file recursively under the selected directory.
The separate historical test selects three hash-pinned files, including valid
Ogg framing with an invalid Vorbis endpoint. See
[historical validation](../../../vorbis/HISTORICAL.md); the container comparison
does not treat codec timestamps as a container restriction.

`capture` uses the same unmodified libogg page/packet APIs as `decode`, allowing
the intentional initial packet hole before the first returned packet after a
mid-stream seek. Later holes remain errors. Rust tests compare every generated
page-boundary capture and beginning/middle/end captures of the local corpus.
The production PacketCapture validates sequence/continuation strictly after
its initial page and never delegates packet assembly to this helper.

`sync-ops` accepts reset, append, `ogg_sync_pageseek` and `ogg_sync_pageout`
operations. It reports exact signed results, unread bytes, hole state and full
returned page bytes. Parsing and synchronization use unmodified libogg.
`sync-read-ops` additionally exercises the pinned vorbisfile boundary/seek
contract with a memory source, configurable read size, read/seek failures and
exact source/logical positions and callback counts. This test-only driver links
only libogg. The adapted transport driver's BSD license is in `VORBIS-COPYING`.

The synchronization tests use the same `DEADLIB_LIBOGG_ORACLE` executable.
Set `DEADLIB_OGG_SYNC_INPUTS` to a new directory to retain every binary request
and response. The four-batch offline golden needs no C installation. Its
regeneration test is manual; normal validation excludes regeneration.
