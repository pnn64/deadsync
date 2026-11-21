# PNG encoder measurement evidence

Raw TSVs, logs and provenance JSON are copied byte for byte. No image inputs or executables are included.
`archive-manifest.json` records the SHA256 of every archived artifact except itself.
`input-manifest.json` distinguishes tracked inputs from local-only files and records original input SHA256, dimensions and sizes.
`runs.json` indexes exact historical commands and identifies measured sources through the frozen executable snapshot, which may differ from the working-tree HEAD when the executable was rerun.
`environment.json` is explicitly an archive-time snapshot, not retroactive proof of every run's host state.

`baseline-encode.rs` reconstructs the original five-mode harness by removing only the later Rle benchmark modes. Its raw SHA256 matches the recorded baseline/steps 1-5 source identity `6b3b184913ab8db948ff4e302542016375fb9c7e7bacb679f9ea7750c1e02753`.

## Reproduction

Run these examples from the repository root on Windows. The current committed harness reproduces generated inputs. To reproduce historical measurements exactly, supply the exact source versions identified in each run record and every local-only external PNG matching its input SHA256; hashes alone do not supply missing source or image bytes.

```powershell
$pngEvidence = 'crates/deadlib-image/png/bench/results/2026-11-20-encode'
$pngCorpus = Get-Content -Raw "$pngEvidence/corpus.json" | ConvertFrom-Json
cargo build --release --locked --manifest-path crates/deadlib-image/png/bench/Cargo.toml --bin encode --target-dir target/png-encode-repro
$env:PNG_BENCH_MS = '40'
& target/png-encode-repro/release/encode.exe --generated --sizes $pngCorpus.sizes
# With every external input available at its original path:
$pngInputs = @($pngCorpus.files | ForEach-Object { $_.path })
& target/png-encode-repro/release/encode.exe --generated --sizes $pngCorpus.sizes @pngInputs
# Allocation measurements use a separate instrumented build:
cargo build --release --locked --manifest-path crates/deadlib-image/png/bench/Cargo.toml --bin encode --features allocations --target-dir target/png-encode-repro-alloc
& target/png-encode-repro-alloc/release/encode.exe --generated --sizes $pngCorpus.sizes @pngInputs
# Example analysis, naming matching input/codec sets from this archive:
python crates/deadlib-image/png/bench/compare_encode.py baseline.tsv after.tsv --corpus "$pngEvidence/corpus.json" --output comparison.json
```

The archived `run.py` and `capture.py` retain the original command-capture implementation. Their paths are relative to their original `target/png-encode-opt-20261120` location; restore them there to use those wrappers.
Allocation columns are requested allocation/reallocation bytes, not peak RSS. Timing TSVs and allocation TSVs are separate builds. A zero or truncated validation log is not evidence of a passing check.
