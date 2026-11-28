param([string]$Destination = 'C:\GitHub\h264-perf-external-20261128')
$ErrorActionPreference = 'Stop'
$repo = (Resolve-Path (Join-Path $PSScriptRoot '..\..\..\..')).Path
$d = [IO.Path]::GetFullPath($Destination)
if ($d.Equals($repo, [StringComparison]::OrdinalIgnoreCase) -or $d.StartsWith($repo + '\', [StringComparison]::OrdinalIgnoreCase)) {
    throw 'The downloaded media and benchmark traces must live outside the repository.'
}
New-Item -ItemType Directory -Force -Path $d | Out-Null
foreach ($name in 'bench.rs','decode.rs','build.ps1','measure.py','summary.py','verify.py','trace_prepare.py','trace.ps1','report.py') {
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot $name) -Destination "$d/$name"
}
function Check-Exit($message) {
    if ($LASTEXITCODE -ne 0) { throw $message }
}
$urls = @{
    'bunny.mp4'='https://raw.githubusercontent.com/chthomos/video-media-samples/master/big-buck-bunny-1080p-30sec.mp4'
    'nature.h264'='https://samples.ffmpeg.org/V-codecs/h264/nature_704x576_25Hz_1500kbits.h264'
    'africa.mov'='https://samples.ffmpeg.org/V-codecs/h264/bbc-africa_m720p.mov'
    'interlaced.mp4'='https://samples.ffmpeg.org/V-codecs/h264/interlaced_crop.mp4'
    'envivio.mp4'='https://samples.ffmpeg.org/V-codecs/h264/envivio-h264.mp4'
}
foreach ($name in $urls.Keys) {
    if (!(Test-Path -LiteralPath "$d/$name")) {
        curl.exe -fL --connect-timeout 10 --max-time 120 --silent --show-error -o "$d/$name" $urls[$name]
        Check-Exit "Download failed: $name"
    }
}
foreach ($sample in @(@('bunny.mp4','bunny',36),@('africa.mov','africa',45),@('nature.h264','nature',60),@('envivio.mp4','envivio',60),@('interlaced.mp4','interlaced',60))) {
    ffmpeg -nostdin -v error -y -i "$d/$($sample[0])" -map 0:v:0 -frames:v $sample[2] -c:v copy -bsf:v h264_mp4toannexb -f h264 "$d/$($sample[1])-excerpt.h264"
    Check-Exit "Excerpt extraction failed: $($sample[0])"
}
foreach ($clip in 'srpg9','srpg10') {
    ffmpeg -nostdin -v error -y -i "$repo/assets/graphics/visual_styles/$clip/background_video.mp4" -map 0:v:0 -frames:v 45 -c:v copy -bsf:v h264_mp4toannexb -f h264 "$d/$clip.h264"
    Check-Exit "Excerpt extraction failed: $clip"
}
ffmpeg -nostdin -v error -y -i "$repo/assets/graphics/visual_styles/srpg10/background_video.mp4" -map 0:v:0 -vf scale=640:360 -frames:v 90 -c:v libx264 -threads 1 -preset fast -profile:v baseline -x264-params cabac=0:bframes=0 -f h264 "$d/cavlc.h264"
Check-Exit 'Game CAVLC transcode failed'
ffmpeg -nostdin -v error -y -ss 12 -i "$d/bunny.mp4" -map 0:v:0 -vf scale=640:360 -frames:v 90 -c:v libx264 -threads 1 -preset fast -profile:v baseline -x264-params cabac=0:bframes=0 -f h264 "$d/bunny-cavlc.h264"
Check-Exit 'Bunny CAVLC transcode failed'
ffmpeg -nostdin -v error -y -i "$d/nature.h264" -map 0:v:0 -frames:v 60 -c:v libx264 -threads 1 -preset fast -profile:v baseline -x264-params cabac=0:bframes=0 -f h264 "$d/nature-cavlc.h264"
Check-Exit 'Nature CAVLC transcode failed'
py -c "import re,pathlib,struct,sys; d=pathlib.Path(sys.argv[1]); [(p.with_suffix('.avcc').write_bytes(b''.join(struct.pack('>I',len(n))+n for n in re.split(rb'\x00\x00\x00?\x01',p.read_bytes())[1:] if n))) for p in d.glob('*.h264') if p.name!='nature.h264']" $d
Check-Exit 'AVCC conversion failed'
