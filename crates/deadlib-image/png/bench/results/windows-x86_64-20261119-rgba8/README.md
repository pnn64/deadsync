# PNG decoder experiment archive, 2026-11-19

This directory records the 116-input Windows x86-64 PNG comparison: 60 generated format cases, 24 pinned Kodak photos, and 32 distinct bundled assets. Equivalent RGBA8 output is the principal consumer comparison; the raw-format runs remain separate controls.

The final performance verdict is maintained in the parent `bench/RESULTS.md`. This archive preserves measurements and their provenance without turning corpus averages into a claim about every file.

## Populate only after final timing finishes

From the repository root:

```powershell
python crates/deadlib-image/png/bench/results/windows-x86_64-20261119-rgba8/archive_results.py --check
python crates/deadlib-image/png/bench/results/windows-x86_64-20261119-rgba8/archive_results.py --write
python crates/deadlib-image/png/bench/results/windows-x86_64-20261119-rgba8/archive_results.py --verify
```

`--check` reports missing or incomplete evidence and exits 2 until ready. `--write` requires the five final timing/allocation TSVs, `final-hashes.json`, `final-summary.json`, all three completed timing-process records, baseline and experiment pairs, and validation logs. It never runs a decoder, compiler, benchmark, test, or download.

The write step checks frozen source and executable hashes, verifies all 116 input hashes against the asset selection and previous pinned corpus, confirms full-table coverage and final-summary hashes, then copies evidence. Run it after the timing processes finish because hashing and copying can disturb measurements.

No executable or PNG payload is copied. Every task-local text artifact is retained under `raw/`, including `inventory.py`, `freeze-final.py`, `analyze-final.py`, `run-final.py`, patches, TSVs, summaries, process records, validation logs, and source snapshots. Original bytes and byte encodings are preserved. Patches and any other text with Git whitespace errors are stored as deterministic gzip; `manifest.json` maps logical filenames to stored paths, storage format, and both original and stored hashes. Initial failures remain clearly historical; they are not discarded.

## Evidence map

| File | Purpose |
| --- | --- |
| `manifest.json` | Archive inventory and SHA256 of every retained payload |
| `experiment-states.json` | Candidate decisions, paired files, source-state descriptions, and known lineage limits |
| `corpus.json` | All 116 benchmark paths, input hashes, dimensions, formats, and original asset paths |
| `raw/assets-manifest.json` | Asset selection rules and byte-identical copy mapping |
| `raw/final-hashes.json` | Hardware, toolchain, build settings, frozen sources, and executable hashes |
| `baseline-source-hashes.json` | Original production/harness Git blob hashes at b04e37657 |
| `compatibility-fix.patch.gz` | Separate correctness commit 3fc2deedb |
| `final-performance.patch.gz` | Final production/test diff after the compatibility commit |
| `final-harness.patch.gz` | Reproducible harness and standalone-workspace changes from the original baseline |
| `validation.json` | Totals parsed from the final log and separately attributed checks |
| `raw/final-summary.json` | Coordinating agent's aggregate and per-file final analysis |

The three final process JSON files record compiler-process checks before and after each measurement process. They preserve the exact commands, sampling durations, exit status, and start/finish times; the boundary checks alone do not claim continuous process monitoring.

## Reconstruct the benchmark inputs

Use the repository revision identified by this archive so the original bundled asset paths still have the recorded bytes. Run these commands from the repository root. The format generator requires a fresh output directory; the photo fetcher verifies existing files and never replaces a mismatch. The original format corpus used Python zlib 1.3.1, as recorded in `raw/formats-generation-manifest.json`; only regeneration that matches all pinned hashes reproduces these inputs.

```powershell
$pngArchive = 'crates/deadlib-image/png/bench/results/windows-x86_64-20261119-rgba8'
python "$pngArchive/raw/generate_decode_corpus.py" --output target/png-decode-corpus-20261117
python "$pngArchive/raw/fetch_corpus.py" --manifest "$pngArchive/raw/kodak.json" --output target/png-kodak-20261117
```

Recreate the selected asset copies from the archived selection manifest, checking source hashes first and preserving any existing destination:

```powershell
$pngAssets = Get-Content -Raw "$pngArchive/raw/assets-manifest.json" | ConvertFrom-Json
New-Item -ItemType Directory -Force target/png-perf-20261119/assets | Out-Null
foreach ($pngAsset in $pngAssets.files) {
    $pngSourceHash = (Get-FileHash -LiteralPath $pngAsset.source -Algorithm SHA256 -ErrorAction Stop).Hash.ToLowerInvariant()
    if ($pngSourceHash -ne $pngAsset.sha256) { throw "Asset source differs: $($pngAsset.source)" }
    if (-not (Test-Path -LiteralPath $pngAsset.benchmark_path)) {
        Copy-Item -LiteralPath $pngAsset.source -Destination $pngAsset.benchmark_path -ErrorAction Stop
    }
}
```

Check every resulting input against this archive before benchmarking. This also catches a stale asset destination or different generated compressed bytes:

```powershell
$pngCorpus = Get-Content -Raw "$pngArchive/corpus.json" | ConvertFrom-Json
foreach ($pngInput in $pngCorpus.inputs) {
    $pngFile = Get-Item -LiteralPath $pngInput.benchmark_path -ErrorAction Stop
    $pngHash = (Get-FileHash -LiteralPath $pngInput.benchmark_path -Algorithm SHA256 -ErrorAction Stop).Hash.ToLowerInvariant()
    if ($pngFile.Length -ne $pngInput.bytes -or $pngHash -ne $pngInput.sha256) {
        throw "Corpus input differs: $($pngInput.benchmark_path)"
    }
}
Write-Output "Verified $($pngCorpus.inputs.Count) pinned PNG inputs"
```

The archived fetcher downloads the external photos from the URLs in `raw/kodak.json`; no photo or asset image bytes are redistributed here. The local `.gitattributes` disables text conversion for the archive so Git checkout preserves the hashes of TSVs, logs, scripts, and manifests.

## Read the comparisons with their actual source states

The original baseline is b04e37657 plus the frozen RGBA8 harness. Portable stride 2 was measured first. The fixed-group and one-bit-LUT unpack comparisons both included the temporary fixed-wide-table candidate on each side. Those fixed tables were subsequently removed; the final hybrid unpack remains.

The exact 1-bit expansion used by `packed-unpack-lut-before.tsv` is not independently recorded: it may be the original per-sample control or the preceding arithmetic fixed-group candidate. This pair therefore does not independently isolate the LUT from the arithmetic candidate. The final full-corpus comparison covers the final hybrid against the original baseline.

The packed-length regression retains its raw before/after tables and full source snapshot. That snapshot excludes fixed-wide-table additions, but the historical source-to-executable linkage was not independently reconstructed. This uncertainty is explicit rather than inferred away from timestamps.

Fused-Up, SIMD stride 2, fixed-wide tables, and packed length lookups were measured and rejected. The contiguous chunk CRC patch was prepared but unmeasured. It is not a measured rejection and contributes no claimed speedup.

The stored-prefetch defect is a separate compatibility fix, commit 3fc2deedb. Its generated mixed-block fixture is independently decoded by flate2 in the committed test. The before log reproduces the defect with the original algorithm; the after log passes. The accepted deferred-initialization comparison uses `stored-fixed-before.tsv`; `stored-before.tsv` predates the fix and is historical evidence only.

## Limits

Total cycles represent one decode of each selected input. Geometric means give each per-file ratio equal weight. Both are retained; neither establishes universal per-file superiority. Allocation counts and cumulative requested bytes are distinct metrics, and a lower corpus total does not imply fewer bytes for every image.

Final validation totals are parsed from `final-tests.log`; historical test/fuzz counts from the November 17 archive are not reused. ARM64 evidence is a compile check, not a runtime benchmark. The benchmark preserves complete native metadata/CRC validation while the existing asset adapter stops at its first frame, as documented in the parent benchmark README.
