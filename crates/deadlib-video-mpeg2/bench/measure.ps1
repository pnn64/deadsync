param(
    [Parameter(Mandatory = $true)] [string] $ItgmaniaRoot,
    [ValidateSet('mpeg1', 'mpeg2')] [string] $Codec = 'mpeg2',
    [string] $OpenItgRoot = '../openitg-master',
    [string] $Results,
    [ValidateRange(0, 63)] [int] $Cpu = 2,
    [ValidateRange(1, 100)] [int] $CycleRuns = 9,
    [ValidateRange(1, 100)] [int] $AllocationRuns = 3
)

$ErrorActionPreference = 'Stop'
if (!$Results) { $Results = "target/$Codec-bench-run" }
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
    $object = Join-Path $out 'ffmpeg.obj'
    $cyclesExe = Join-Path $out 'cycles.exe'
    $allocExe = Join-Path $out 'allocations.exe'
    $rlib = Join-Path $build 'release\libdeadlib_video_mpeg2.rlib'

    cargo build --release --locked --offline --manifest-path crates/deadlib-video-mpeg2/Cargo.toml --target-dir $build
    Assert-Exit 'Building deadlib-video-mpeg2'
    $defines = @()
    if ($Codec -eq 'mpeg1') { $defines = @('/DBENCH_MPEG1') }
    cl.exe /nologo /c /O2 /MT /std:c11 @defines "/I$ffInclude" "/Fo$object" (Join-Path $PSScriptRoot 'ffmpeg.c')
    Assert-Exit 'Compiling the FFmpeg harness'
    $common = @('--edition', '2021', '--crate-name', 'mpeg2_bench', '-C', 'opt-level=3',
        '-C', 'target-feature=+crt-static', '--extern', "deadlib_video_mpeg2=$rlib",
        "-Lnative=$ffLib", '-lavcodec', '-lavutil', "-Clink-arg=$object")
    $bench = Join-Path $PSScriptRoot 'bench.rs'
    rustc @common -o $cyclesExe $bench
    Assert-Exit 'Building the cycle harness'
    rustc @common --cfg allocations -o $allocExe $bench
    Assert-Exit 'Building the allocation harness'

    if ($Codec -eq 'mpeg1') {
        $nexus = Join-Path $OpenItgRoot 'assets/d4/Themes/default/BGAnimations/ScreenAdvertisement background/nexuswave.mpg'
        $samples = (Resolve-Path -LiteralPath @(
            'assets/media/openitg/company.mpg', 'assets/media/openitg/intro.mpg', $nexus
        )).Path
    } else {
        $fixtureRoot = 'tests/fixtures/itgmania-song-lua-micro/eros-and-apollo/inputs'
        $samples = foreach ($relative in 'turn/dix.mpg', 'how/lights.mpg') {
            $name = [IO.Path]::GetFileNameWithoutExtension($relative)
            $sample = Join-Path $out "$name.m2v"
            ffmpeg -nostdin -v error -y -i "$fixtureRoot/$relative" -map 0:v:0 -c:v copy -f mpeg2video $sample
            Assert-Exit "Extracting $name"
            $sample
        }
    }

    $env:Path = "$ffLib;$originalPath"
    & $cyclesExe $CycleRuns $Cpu @samples | Out-File -Encoding ascii (Join-Path $out 'cycles.csv')
    Assert-Exit 'Measuring thread cycles and checking decoded output'
    & $allocExe $AllocationRuns $Cpu @samples | Out-File -Encoding ascii (Join-Path $out 'allocations.csv')
    Assert-Exit 'Measuring allocations'
    py (Join-Path $PSScriptRoot 'summarize.py') $out $CycleRuns $AllocationRuns $Codec
    Assert-Exit 'Summarizing measurements'
} finally {
    $env:Path = $originalPath
    Pop-Location
}
