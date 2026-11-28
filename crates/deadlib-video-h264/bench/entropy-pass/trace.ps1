param([string]$Crate = 'C:\GitHub\deadsync\crates\deadlib-video-h264')
$ErrorActionPreference = 'Stop'
$d = $PSScriptRoot
py "$d/trace_prepare.py" $Crate
if ($LASTEXITCODE) { exit $LASTEXITCODE }
rustc --edition 2021 --crate-name deadlib_video_h264 --crate-type rlib -C opt-level=3 -C codegen-units=16 --cfg 'feature=\"std\"' --cfg 'feature=\"surface\"' "$d/trace-crate/src/lib.rs" -o "$d/libtrace.rlib"
if ($LASTEXITCODE) { exit $LASTEXITCODE }
rustc --edition 2021 -C opt-level=3 --extern "codec=$d/libtrace.rlib" "$d/decode.rs" -o "$d/trace-cycles.exe"
if ($LASTEXITCODE) { exit $LASTEXITCODE }
try {
    foreach ($clip in 'cavlc','bunny-cavlc','nature-cavlc','envivio-excerpt','interlaced-excerpt','nature-excerpt','africa-excerpt','bunny-excerpt') {
        $env:H264_TRACE_FILE = "$d/$clip.trace"
        & "$d/trace-cycles.exe" 42 surface-annex "$d/$clip.h264" 1
        if ($LASTEXITCODE) { throw "Trace capture failed: $clip" }
    }
} finally {
    Remove-Item Env:H264_TRACE_FILE -ErrorAction SilentlyContinue
}
