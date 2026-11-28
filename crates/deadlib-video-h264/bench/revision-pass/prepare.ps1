$ErrorActionPreference = 'Stop'
$repo = (Resolve-Path (Join-Path $PSScriptRoot '..\..\..\..')).Path
Push-Location $repo
try {
    $d = 'target/h264-perf-20261128'
    New-Item -ItemType Directory -Force -Path $d | Out-Null
    foreach ($name in 'bench.rs','build.ps1','measure.py','summary.py','verify.py','report.py') {
        Copy-Item -LiteralPath (Join-Path $PSScriptRoot $name) -Destination "$d/$name"
    }
    foreach ($clip in 'srpg9','srpg10') {
        ffmpeg -nostdin -v error -y -i "assets/graphics/visual_styles/$clip/background_video.mp4" -map 0:v:0 -frames:v 45 -c:v copy -bsf:v h264_mp4toannexb -f h264 "$d/$clip.h264"
        if ($LASTEXITCODE) { throw "Extracting $clip failed" }
    }
    ffmpeg -nostdin -v error -y -i assets/graphics/visual_styles/srpg10/background_video.mp4 -map 0:v:0 -vf scale=640:360 -frames:v 90 -c:v libx264 -threads 1 -preset fast -profile:v baseline -x264-params cabac=0:bframes=0 -f h264 "$d/cavlc.h264"
    if ($LASTEXITCODE) { throw 'CAVLC transcode failed' }
    py -c "import re,pathlib,struct; d=pathlib.Path('target/h264-perf-20261128'); [(p.with_suffix('.avcc').write_bytes(b''.join(struct.pack('>I',len(n))+n for n in re.split(rb'\x00\x00\x00?\x01',p.read_bytes())[1:] if n))) for p in d.glob('*.h264')]"
    if ($LASTEXITCODE) { throw 'AVCC conversion failed' }
} finally {
    Pop-Location
}
