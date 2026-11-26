param(
    [Parameter(Mandatory = $true)] [string] $ItgmaniaRoot,
    [string] $Results = 'target/h264-bench-run',
    [ValidateRange(0, 63)] [int] $Cpu = 2,
    [ValidateRange(1, 100)] [int] $CycleRuns = 5,
    [ValidateRange(1, 100)] [int] $AllocationRuns = 2,
    [switch] $BuildOnly
)

$ErrorActionPreference = 'Stop'
$repo = (Resolve-Path (Join-Path $PSScriptRoot '..\..\..')).Path
$ff = Join-Path (Resolve-Path -LiteralPath $ItgmaniaRoot).Path 'extern\ffmpeg-w32'
$ffInclude = Join-Path $ff 'include'
$ffLib = Join-Path $ff 'x64'
if (!(Test-Path -LiteralPath (Join-Path $ffLib 'avcodec-62.dll'))) {
    throw "Expected ITGmania's x64 libavcodec 62 under $ffLib."
}

function Assert-Exit([string] $Operation) {
    if ($LASTEXITCODE -ne 0) { throw "$Operation failed with exit code $LASTEXITCODE." }
}

$originalPath = $env:Path
Push-Location $repo
try {
    $out = (New-Item -ItemType Directory -Force -Path $Results).FullName
    $build = Join-Path $out 'build'
    $surfaceBuild = Join-Path $out 'surface-build'
    $object = Join-Path $out 'ffmpeg.obj'
    $cyclesExe = Join-Path $out 'cycles.exe'
    $allocExe = Join-Path $out 'allocations.exe'
    $codec = Join-Path $build 'release\libdeadlib_video_h264.rlib'
    $surfaceCodec = Join-Path $surfaceBuild 'release\libdeadlib_video_h264.rlib'

    cargo build --release --locked --offline --manifest-path crates/deadlib-video-h264/Cargo.toml --target-dir $build
    Assert-Exit 'Building deadlib-video-h264'
    cargo build --release --locked --offline --features surface --manifest-path crates/deadlib-video-h264/Cargo.toml --target-dir $surfaceBuild
    Assert-Exit 'Building deadlib-video-h264 with the surface feature'
    cl.exe /nologo /c /O2 /MT /std:c11 "/I$ffInclude" "/Fo$object" (Join-Path $PSScriptRoot 'ffmpeg.c')
    Assert-Exit 'Compiling the FFmpeg harness'
    $common = @('--edition', '2021', '--crate-name', 'h264_bench', '-C', 'opt-level=3',
        '-C', 'target-feature=+crt-static', '--extern', "deadlib_video_h264=$codec",
        '--extern', "surface_codec=$surfaceCodec", "-Lnative=$ffLib", '-lavcodec', '-lavutil', "-Clink-arg=$object")
    $bench = Join-Path $PSScriptRoot 'bench.rs'
    rustc @common -o $cyclesExe $bench
    Assert-Exit 'Building the cycle harness'
    rustc @common --cfg allocations -o $allocExe $bench
    Assert-Exit 'Building the allocation harness'
    if ($BuildOnly) { return }

    $samples = foreach ($style in 'srpg9', 'srpg10') {
        $inputPath = "assets/graphics/visual_styles/$style/background_video.mp4"
        $sample = Join-Path $out "$style.h264"
        ffmpeg -nostdin -v error -y -i $inputPath -map 0:v:0 -frames:v 150 -c:v copy -bsf:v h264_mp4toannexb -f h264 $sample
        Assert-Exit "Extracting $style"
        $sample
    }

    $env:Path = "$ffLib;$originalPath"
    & $cyclesExe $CycleRuns $Cpu @samples | Out-File -Encoding ascii (Join-Path $out 'cycles.csv')
    Assert-Exit 'Measuring cycles'
    & $allocExe $AllocationRuns $Cpu @samples | Out-File -Encoding ascii (Join-Path $out 'allocations.csv')
    Assert-Exit 'Measuring allocations'
    py (Join-Path $PSScriptRoot 'summarize.py') $out --cycle-runs $CycleRuns --allocation-runs $AllocationRuns
    Assert-Exit 'Summarizing measurements'
} finally {
    $env:Path = $originalPath
    Pop-Location
}
