param(
    [ValidateRange(1, 86400)][int]$Seconds = 300,
    [string]$RunRoot = '',
    [ValidateSet('ogg_container', 'vorbis_headers', 'vorbis_packets', 'vorbis_stream', 'vorbis_encode')]
    [string[]]$Targets = @('ogg_container', 'vorbis_headers', 'vorbis_packets', 'vorbis_stream', 'vorbis_encode'),
    [string]$FloorZero = '',
    [string[]]$ExtraSeeds = @(),
    [string]$ResumeCorpus = '',
    [int]$Seed = 1337,
    [string]$MsvcBin = ''
)
$ErrorActionPreference = 'Stop'
$decodeRoot = Split-Path $PSScriptRoot -Parent
$repositoryRoot = (Resolve-Path (Join-Path $decodeRoot '../../..')).Path
$fuzzRoot = Join-Path $decodeRoot 'fuzz'
$buildRoot = Join-Path $repositoryRoot 'target/deadlib-audio-fuzz'
$localTools = Join-Path $repositoryRoot 'target/deadlib-audio-reference/fuzz-tools/bin'
if (Test-Path -LiteralPath $localTools) { $env:PATH = "$localTools;$env:PATH" }
Get-Command cargo-fuzz -ErrorAction Stop | Out-Null
if (-not $RunRoot) {
    $RunRoot = Join-Path $repositoryRoot ('target/deadlib-audio-reference/fuzz-' + [guid]::NewGuid().ToString('N'))
}
if (Test-Path -LiteralPath $RunRoot) { throw "RunRoot must be new: $RunRoot" }
New-Item -ItemType Directory -Path $RunRoot | Out-Null
$run = (Resolve-Path -LiteralPath $RunRoot).Path
if ($env:OS -eq 'Windows_NT') {
    if (-not $MsvcBin) {
        $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
        $installation = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
        if ($LASTEXITCODE -ne 0 -or -not $installation) { throw 'Cannot locate MSVC; pass -MsvcBin.' }
        $version = Get-Content -LiteralPath (Join-Path $installation 'VC/Auxiliary/Build/Microsoft.VCToolsVersion.default.txt')
        $MsvcBin = Join-Path $installation "VC/Tools/MSVC/$($version.Trim())/bin/Hostx64/x64"
    }
    if (-not (Test-Path -LiteralPath (Join-Path $MsvcBin 'clang_rt.asan_dynamic-x86_64.dll'))) {
        throw 'MSVC AddressSanitizer DLL is missing; install the C++ AddressSanitizer component.'
    }
    $env:PATH = "$MsvcBin;$env:PATH"
}
$external = @()
if ($FloorZero) {
    $env:DEADLIB_VORBIS_FLOOR0 = (Resolve-Path -LiteralPath $FloorZero).Path
    $external += $env:DEADLIB_VORBIS_FLOOR0
} elseif ($env:DEADLIB_VORBIS_FLOOR0) {
    $external += (Resolve-Path -LiteralPath $env:DEADLIB_VORBIS_FLOOR0).Path
}
$extra = @($ExtraSeeds | ForEach-Object { (Resolve-Path -LiteralPath $_).Path })
$external += $extra
function Invoke-Cargo([string[]]$Arguments, [string]$LogName) {
    $previous = $ErrorActionPreference
    try {
        $ErrorActionPreference = 'Continue'
        & cargo @Arguments 2>&1 | ForEach-Object { $_.ToString() } | Set-Content -LiteralPath (Join-Path $run $LogName)
        $code = $LASTEXITCODE
    } finally { $ErrorActionPreference = $previous }
    if ($code -ne 0) { throw "Cargo failed ($code); see $run/$LogName" }
}
$manifest = Join-Path $fuzzRoot 'Cargo.toml'
Invoke-Cargo -Arguments (@('run', '--locked', '--manifest-path', $manifest, '--release', '--features', 'seed', '--bin', 'seed', '--', (Join-Path $run 'corpus')) + $extra) -LogName 'seeds.log'
if ($ResumeCorpus) {
    $resume = (Resolve-Path -LiteralPath $ResumeCorpus).Path
    foreach ($target in $Targets) {
        $directory = Join-Path $resume $target
        foreach ($file in Get-ChildItem -LiteralPath $directory -File) {
            if ($file.Length -gt 131073) { throw "Resume seed exceeds input cap: $($file.FullName)" }
            $hash = (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
            Copy-Item -LiteralPath $file.FullName -Destination (Join-Path $run "corpus/$target/resume-$hash")
        }
    }
}
Invoke-Cargo -Arguments @('+nightly', 'fuzz', 'build', '--fuzz-dir', $fuzzRoot, '--target-dir', $buildRoot) -LogName 'build.log'
$hashes = @{}
foreach ($file in Get-ChildItem -LiteralPath (Join-Path $run 'corpus') -File -Recurse) {
    $hashes[$file.FullName.Substring($run.Length + 1).Replace('\', '/')] = (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
}
$hashes | ConvertTo-Json -Depth 3 | Set-Content -LiteralPath (Join-Path $run 'initial-corpus.sha256.json')
$sources = @{}
foreach ($part in @('fuzz', 'ogg/src', 'vorbis/src')) {
    foreach ($file in Get-ChildItem -LiteralPath (Join-Path $decodeRoot $part) -File -Recurse) {
        if ($file.FullName -match '[\\/](target|corpus|artifacts|coverage)[\\/]') { continue }
        $sources[$file.FullName.Substring($decodeRoot.Length + 1).Replace('\', '/')] = (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    }
}
$externalHashes = @{}
foreach ($file in $external) { $externalHashes[$file] = (Get-FileHash -LiteralPath $file -Algorithm SHA256).Hash.ToLowerInvariant() }
$sources['scripts/fuzz.ps1'] = (Get-FileHash -LiteralPath $PSCommandPath -Algorithm SHA256).Hash.ToLowerInvariant()
$provenance = [ordered]@{
    started_utc = [DateTime]::UtcNow.ToString('o'); seconds_per_target = $Seconds; targets = $Targets
    git_head = (& git -C $repositoryRoot rev-parse HEAD); rustc = (& rustc +nightly -Vv) -join "`n"
    cargo_fuzz = (& cargo fuzz --version) -join "`n"; sanitizer = 'address'; seed = $Seed
    max_len = 131073; rss_limit_mb = 2048; timeout_seconds = 20; resumed_corpus = $ResumeCorpus
    msvc_bin = $MsvcBin; external_sha256 = $externalHashes; source_sha256 = $sources
    os = [Environment]::OSVersion.ToString(); processor_count = [Environment]::ProcessorCount
}
$provenance | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $run 'provenance.json')
$jobs = @()
foreach ($target in $Targets) {
    $artifacts = Join-Path $run "artifacts/$target"
    New-Item -ItemType Directory -Path $artifacts -Force | Out-Null
    $exe = Join-Path $buildRoot "x86_64-pc-windows-msvc/release/$target.exe"
    $arguments = @(
        ('"' + (Join-Path $run "corpus/$target") + '"'),
        "-max_total_time=$Seconds", '-max_len=131073', '-timeout=20', '-rss_limit_mb=2048',
        "-seed=$($Seed + $jobs.Count)", '-print_final_stats=1',
        ('"-artifact_prefix=' + $artifacts.Replace('\', '/') + '/"'),
        ('"-dict=' + (Join-Path $fuzzRoot 'ogg-vorbis.dict') + '"')
    )
    $binaryHash = (Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash.ToLowerInvariant()
    $process = Start-Process -FilePath $exe -ArgumentList $arguments -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $run "$target.stdout.log") -RedirectStandardError (Join-Path $run "$target.log")
    $null = $process.Handle
    $jobs += [pscustomobject]@{ target = $target; process = $process; started = [DateTime]::UtcNow; binary_sha256 = $binaryHash; arguments = $arguments }
    Write-Output "Started $target (PID $($process.Id)); evidence: $run"
}
while (@($jobs | Where-Object { -not $_.process.HasExited }).Count -ne 0) { Start-Sleep -Milliseconds 500 }
$results = @()
foreach ($job in $jobs) {
    $job.process.WaitForExit()
    $log = Get-Content -LiteralPath (Join-Path $run "$($job.target).log") -Raw
    function Stat([string]$Name) {
        $match = [regex]::Match($log, "stat::$($Name):\s+(\d+)")
        if ($match.Success) { return [long]$match.Groups[1].Value }
        return $null
    }
    $done = [regex]::Matches($log, '#(\d+)\s+DONE\s+cov:\s+(\d+)\s+ft:\s+(\d+)')
    $coverage = $null; $features = $null
    if ($done.Count -ne 0) {
        $last = $done[$done.Count - 1]
        $coverage = [long]$last.Groups[2].Value; $features = [long]$last.Groups[3].Value
    }
    $result = [ordered]@{
        target = $job.target; exit_code = $job.process.ExitCode
        executions = (Stat 'number_of_executed_units'); coverage = $coverage; features = $features
        peak_rss_mb = (Stat 'peak_rss_mb'); average_exec_per_second = (Stat 'average_exec_per_sec')
        binary_sha256 = $job.binary_sha256; arguments = $job.arguments
        wall_seconds = ($job.process.ExitTime.ToUniversalTime() - $job.started).TotalSeconds
    }
    $results += [pscustomobject]$result
    Write-Output "$($job.target): exit $($job.process.ExitCode), $($result.executions) executions, coverage $coverage"
}
ConvertTo-Json -InputObject @($results) -Depth 4 | Set-Content -LiteralPath (Join-Path $run 'results.json')
$results | Select-Object target, exit_code, executions, coverage, features, peak_rss_mb, average_exec_per_second, wall_seconds | Export-Csv -LiteralPath (Join-Path $run 'results.tsv') -Delimiter "`t" -NoTypeInformation
Write-Output "Retained fuzz evidence: $run"
if (@($results | Where-Object { $_.exit_code -ne 0 }).Count -ne 0) { throw 'A fuzz target failed; preserve its artifact and replay before changing the oracle.' }
