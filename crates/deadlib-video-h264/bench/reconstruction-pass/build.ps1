param([string]$Stage, [string]$Source = 'C:\GitHub\deadsync\crates\deadlib-video-h264\src', [switch]$ReplayOnly)
$ErrorActionPreference = 'Stop'
$d = $PSScriptRoot
foreach ($file in 'cabac.rs','cabac_tables.rs') {
    Copy-Item "$Source/$file" "$d/$file" -Force
}
$cabac = [IO.File]::ReadAllText("$d/cabac.rs").Replace('//!','//')
[IO.File]::WriteAllText("$d/cabac.rs", $cabac, [Text.UTF8Encoding]::new($false))
rustc --edition 2021 -C opt-level=3 "$d/replay.rs" -o "$d/$Stage-replay.exe"
if ($LASTEXITCODE) { exit $LASTEXITCODE }
if (Test-Path "$Source/residual.rs") {
    Copy-Item "$Source/residual.rs" "$d/residual.rs" -Force
    py "$d/chroma_extract.py" "$Source"
    if ($LASTEXITCODE) { exit $LASTEXITCODE }
    rustc --edition 2021 -C opt-level=3 "$d/transform_replay.rs" -o "$d/$Stage-transform.exe"
    if ($LASTEXITCODE) { exit $LASTEXITCODE }
}
if ($ReplayOnly) { return }
rustc --edition 2021 --crate-name deadlib_video_h264 --crate-type rlib -C opt-level=3 -C codegen-units=16 --cfg 'feature=\"std\"' --cfg 'feature=\"surface\"' "$Source/lib.rs" -o "$d/lib$Stage.rlib"
if ($LASTEXITCODE) { exit $LASTEXITCODE }
foreach ($kind in 'cycles','alloc') {
    $extra = @()
    if ($kind -eq 'alloc') { $extra = @('--cfg','allocations') }
    rustc --edition 2021 -C opt-level=3 --extern "codec=$d/lib$Stage.rlib" @extra "$d/decode.rs" -o "$d/$Stage-decode-$kind.exe"
    if ($LASTEXITCODE) { exit $LASTEXITCODE }
}
