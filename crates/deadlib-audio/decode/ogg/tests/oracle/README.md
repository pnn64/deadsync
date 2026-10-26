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
cargo test -p deadlib-audio-decode-ogg --release --test libogg -- --ignored --nocapture
```

Use the platform's executable path and omit `-A x64` for other generators.
The v1.3.6 reference tag is recorded with the run evidence in `COVERAGE.md`.
The corpus test checks every Ogg file recursively under the selected directory.
