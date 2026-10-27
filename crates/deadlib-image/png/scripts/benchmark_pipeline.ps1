param(
    [Parameter(Mandatory=$true)][string]$LogPrefix,
    [int]$Iterations = 40,
    [int]$Runs = 3,
    [ValidateSet('pipeline','deflate')][string]$Stage = 'pipeline',
    [ValidateSet('pipeline','corpus','inflate','filter','decode')][string]$Target = 'pipeline',
    [string]$CorpusDirectory,
    [int]$Cpu = -1
)
$ErrorActionPreference = 'Stop'
if ($Iterations -lt 1 -or $Runs -lt 1) { throw 'Iterations and Runs must be positive' }
$pngRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
$manifest = Join-Path $pngRoot 'Cargo.toml'
$usesCorpus = $Target -in @('corpus','inflate','decode')
if ($Target -ne 'pipeline' -and $Stage -ne 'pipeline') { throw 'Stage selection applies only to the synthetic pipeline target' }
if ($usesCorpus) {
    if (-not $CorpusDirectory) { throw 'CorpusDirectory is required for the corpus target' }
    $CorpusDirectory = (Resolve-Path -LiteralPath $CorpusDirectory).Path
}
$absolutePrefix = [System.IO.Path]::GetFullPath($LogPrefix)
$manifestPath = $absolutePrefix + '-manifest.json'
if (Test-Path -LiteralPath $manifestPath) { throw ('Evidence exists: ' + $manifestPath) }
for ($run=1; $run -le $Runs; $run++) {
    if (Test-Path -LiteralPath ($absolutePrefix + '-run' + $run + '.tsv')) { throw 'Evidence exists; choose a new LogPrefix' }
}
New-Item -ItemType Directory -Path (Split-Path -Parent $absolutePrefix) -Force | Out-Null
# Cargo reports the exact executable for either workspace layout. Native stderr
# is progress output, not a PowerShell error; check Cargo's exit code instead.
$ErrorActionPreference = 'Continue'
$binaryName = switch ($Target) { 'corpus' { 'corpus_bench' } 'inflate' { 'inflate_bench' } 'filter' { 'filter_bench' } 'decode' { 'decode_bench' } default { 'pipeline' } }
if ($Target -ne 'pipeline') {
    $build = & cargo build --manifest-path $manifest -p deadlib-image-png --profile bench --example $binaryName --message-format json
} else {
    $build = & cargo bench --manifest-path $manifest -p deadlib-image-png --bench $binaryName --no-run --message-format json
}
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
$ErrorActionPreference = 'Stop'
$artifact = $build | ForEach-Object { $_ | ConvertFrom-Json } |
    Where-Object { $_.reason -eq 'compiler-artifact' -and $_.target.name -eq $binaryName -and $_.executable } | Select-Object -Last 1
if (-not $artifact) { throw 'Benchmark executable was not reported by Cargo' }
$executable = Get-Item -LiteralPath $artifact.executable
$eligible = [System.Diagnostics.Process]::GetCurrentProcess().ProcessorAffinity.ToInt64()
if ($Cpu -eq -1) {
    for ($candidate=0; $candidate -lt 63; $candidate++) {
        if (($eligible -band ([long]1 -shl $candidate)) -ne 0) { $Cpu=$candidate; break }
    }
}
if ($Cpu -lt 0 -or $Cpu -gt 62 -or ($eligible -band ([long]1 -shl $Cpu)) -eq 0) { throw 'CPU is outside the process affinity mask' }
$affinity = [long]1 -shl $Cpu
$utf8 = New-Object System.Text.UTF8Encoding($false)
$sourceEvidence = Get-ChildItem -LiteralPath (Join-Path $pngRoot 'src') -Filter '*.rs' -Recurse | Sort-Object FullName | ForEach-Object {
    [ordered]@{ path=$_.FullName.Substring($pngRoot.Length+1).Replace('\','/'); sha256=(Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash }
}
$benchmarkSource = switch ($Target) { 'corpus' { 'examples/corpus_bench.rs' } 'inflate' { 'examples/inflate_bench.rs' } 'filter' { 'examples/filter_bench.rs' } 'decode' { 'examples/decode_bench.rs' } default { 'benches/pipeline.rs' } }
$benchmarkSourceHash = (Get-FileHash -LiteralPath (Join-Path $pngRoot $benchmarkSource) -Algorithm SHA256).Hash
$corpusEvidence = @()
if ($usesCorpus) {
    $corpusEvidence = Get-ChildItem -LiteralPath $CorpusDirectory -Filter '*.png' | Sort-Object Name | ForEach-Object {
        [ordered]@{ file=$_.Name; bytes=$_.Length; sha256=(Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash }
    }
}
$sourceCommit = (& git rev-parse HEAD)
$executableHash = (Get-FileHash -LiteralPath $executable.FullName -Algorithm SHA256).Hash
$records = @()
for ($run=1; $run -le $Runs; $run++) {
    $start = [System.Diagnostics.ProcessStartInfo]::new()
    $start.FileName = $executable.FullName
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    $start.EnvironmentVariables['DEADLIB_PNG_BENCH_ITERS'] = $Iterations.ToString()
    $start.EnvironmentVariables['DEADLIB_PNG_BENCH_STAGE'] = $Stage
    if ($usesCorpus) { $start.EnvironmentVariables['DEADLIB_PNG_CORPUS'] = $CorpusDirectory }
    $process = [System.Diagnostics.Process]::new()
    $process.StartInfo = $start
    $began = [DateTime]::UtcNow
    [void]$process.Start()
    try { $process.ProcessorAffinity = [IntPtr]$affinity }
    catch { $process.Kill(); $process.WaitForExit(); throw }
    $actualAffinity = $process.ProcessorAffinity.ToInt64()
    if ($actualAffinity -ne $affinity) { $process.Kill(); $process.WaitForExit(); throw 'Affinity assignment did not persist' }
    $stdout = $process.StandardOutput.ReadToEndAsync()
    $stderr = $process.StandardError.ReadToEndAsync()
    $process.WaitForExit()
    $text = $stdout.GetAwaiter().GetResult().Replace("`r`n", "`n")
    $errors = $stderr.GetAwaiter().GetResult()
    if ($process.ExitCode -ne 0) { throw ('Benchmark failed: ' + $errors) }
    $path = $absolutePrefix + '-run' + $run + '.tsv'
    [System.IO.File]::WriteAllText($path, $text, $utf8)
    $records += [ordered]@{ run=$run; cpu=$Cpu; affinity=$actualAffinity; start_utc=$began.ToString('o'); end_utc=[DateTime]::UtcNow.ToString('o'); output=$path; stderr=$errors }
    $process.Dispose()
    Write-Output ('Completed run ' + $run + ': ' + $path)
}
$processor = Get-CimInstance Win32_Processor | Select-Object Name,NumberOfCores,NumberOfLogicalProcessors
$evidence = [ordered]@{ target=$Target; stage=$Stage; iterations=$Iterations; runs=$Runs; warmups_per_operation=3;
    rustc=(& rustc --version); cargo=(& cargo --version); commit=$sourceCommit;
    source_sha256=$sourceEvidence; benchmark_source=$benchmarkSource; benchmark_source_sha256=$benchmarkSourceHash;
    corpus_sha256=$corpusEvidence;
    executable=$executable.FullName; executable_sha256=$executableHash;
    processor=$processor; eligible_affinity=$eligible; cpu=$Cpu; affinity=$affinity; measurements=$records;
    limits='Single logical CPU affinity; shared host, frequency and other workloads are not controlled. Includes each codec''s allocations. Deflate stage uses identical filtered bytes per source.' }
[System.IO.File]::WriteAllText($manifestPath, (($evidence | ConvertTo-Json -Depth 8).Replace("`r`n", "`n") + "`n"), $utf8)
Write-Output ('Manifest: ' + $manifestPath)
