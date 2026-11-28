param(
    [string]$Previous = 'C:\GitHub\h264-perf-external-20261128',
    [string]$Destination = 'C:\GitHub\h264-perf-cabac-20261128'
)
$ErrorActionPreference = 'Stop'
$repo = (Resolve-Path (Join-Path $PSScriptRoot '..\..\..\..')).Path
$d = [IO.Path]::GetFullPath($Destination)
$previous = (Resolve-Path -LiteralPath $Previous).Path
if ($d.Equals($repo, [StringComparison]::OrdinalIgnoreCase) -or $d.StartsWith($repo + '\', [StringComparison]::OrdinalIgnoreCase) -or $d.Equals($previous, [StringComparison]::OrdinalIgnoreCase)) {
    throw 'Use a separate external directory for reconstruction samples and traces.'
}
New-Item -ItemType Directory -Force $d | Out-Null
foreach ($file in 'build.ps1','decode.rs','replay.rs','transform_replay.rs','trace.py','transform_trace.py','chroma_trace.py','chroma_extract.py','trace.ps1','measure.py','summary.py','verify.py','report.py') {
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot $file) -Destination "$d/$file" -Force
}
foreach ($clip in 'srpg9','srpg10','bunny-excerpt','nature-excerpt','envivio-excerpt','interlaced-excerpt','cavlc','bunny-cavlc','nature-cavlc','africa-excerpt') {
    foreach ($ext in 'h264','avcc') {
        Copy-Item -LiteralPath "$previous/$clip.$ext" -Destination "$d/$clip.$ext" -Force
    }
}
ffmpeg -nostdin -v error -y -ss 12 -i "$previous/bunny.mp4" -map 0:v:0 -vf scale=640:360 -frames:v 60 -c:v libx264 -threads 1 -preset fast -profile:v high -x264-params cqm=jvt:8x8dct=1 -f h264 "$d/bunny-custom.h264"
if ($LASTEXITCODE) { throw 'Custom-matrix transcode failed' }
py -c "import re,pathlib,struct,sys; p=pathlib.Path(sys.argv[1]); p.with_suffix('.avcc').write_bytes(b''.join(struct.pack('>I',len(n))+n for n in re.split(rb'\x00\x00\x00?\x01',p.read_bytes())[1:] if n))" "$d/bunny-custom.h264"
if ($LASTEXITCODE) { throw 'Custom-matrix AVCC conversion failed' }
