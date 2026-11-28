param(
    [ValidateSet('cabac','transform','chroma')][string]$Kind,
    [string]$Source,
    [string]$Cases,
    [int]$Cpu = 42
)
$ErrorActionPreference = 'Stop'
$d = $PSScriptRoot
$src = (Resolve-Path -LiteralPath $Source).Path
$script, $dir = switch ($Kind) {
    'cabac' { 'trace.py', 'trace-src' }
    'transform' { 'transform_trace.py', 'transform-src' }
    'chroma' { 'chroma_trace.py', 'chroma-src' }
}
Copy-Item -LiteralPath (Join-Path (Split-Path $src -Parent) 'README.md') -Destination "$d/README.md" -Force
py "$d/$script" $src
if ($LASTEXITCODE) { exit $LASTEXITCODE }
rustc --edition 2021 --crate-name deadlib_video_h264 --crate-type rlib -C opt-level=3 -C codegen-units=16 --cfg 'feature=\"std\"' --cfg 'feature=\"surface\"' "$d/$dir/lib.rs" -o "$d/lib$Kind-trace.rlib"
if ($LASTEXITCODE) { exit $LASTEXITCODE }
rustc --edition 2021 -C opt-level=3 --extern "codec=$d/lib$Kind-trace.rlib" "$d/decode.rs" -o "$d/$Kind-trace.exe"
if ($LASTEXITCODE) { exit $LASTEXITCODE }
$saved = $env:H264_TRACE_PREFIX
try {
    foreach ($clip in $Cases.Split(',')) {
        $env:H264_TRACE_PREFIX = "$d/$clip"
        & "$d/$Kind-trace.exe" $Cpu surface-annex "$d/$clip.h264" 1
        if ($LASTEXITCODE) { throw "Trace failed: $clip" }
    }
} finally {
    $env:H264_TRACE_PREFIX = $saved
}
