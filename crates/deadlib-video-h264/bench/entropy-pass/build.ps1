param([string]$Stage, [string]$Source = 'C:\GitHub\deadsync\crates\deadlib-video-h264\src')
$ErrorActionPreference = 'Stop'
$d = $PSScriptRoot
foreach ($file in 'bitstream.rs','cavlc.rs','once.rs') {
    Copy-Item "$Source/$file" "$d/$file" -Force
}
rustc --edition 2021 --crate-name deadlib_video_h264 --crate-type rlib -C opt-level=3 -C codegen-units=16 --cfg 'feature=\"std\"' --cfg 'feature=\"surface\"' "$Source/lib.rs" -o "$d/lib$Stage.rlib"
if ($LASTEXITCODE) { exit $LASTEXITCODE }
foreach ($kind in 'cycles','alloc') {
    $extra = @()
    if ($kind -eq 'alloc') { $extra = @('--cfg','allocations') }
    rustc --edition 2021 -C opt-level=3 --cfg 'feature=\"std\"' --extern "codec=$d/lib$Stage.rlib" @extra "$d/bench.rs" -o "$d/$Stage-$kind.exe"
    if ($LASTEXITCODE) { exit $LASTEXITCODE }
    rustc --edition 2021 -C opt-level=3 --extern "codec=$d/lib$Stage.rlib" @extra "$d/decode.rs" -o "$d/$Stage-decode-$kind.exe"
    if ($LASTEXITCODE) { exit $LASTEXITCODE }
}
