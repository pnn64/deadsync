param([string]$Stage)
$ErrorActionPreference = 'Stop'
$d = 'target/h264-perf-20261128'
$src = 'crates/deadlib-video-h264/src'
Copy-Item "$src/bitstream.rs" "$d/bitstream.rs" -Force
rustc --edition 2021 --crate-name deadlib_video_h264 --crate-type rlib -C opt-level=3 -C codegen-units=16 --cfg 'feature=\"std\"' --cfg 'feature=\"surface\"' "$src/lib.rs" -o "$d/lib$Stage.rlib"
if ($LASTEXITCODE) { exit $LASTEXITCODE }
foreach ($kind in 'cycles','alloc') {
    $extra = @()
    if ($kind -eq 'alloc') { $extra = @('--cfg','allocations') }
    rustc --edition 2021 -C opt-level=3 --extern "codec=$d/lib$Stage.rlib" @extra "$d/bench.rs" -o "$d/$Stage-$kind.exe"
    if ($LASTEXITCODE) { exit $LASTEXITCODE }
}
