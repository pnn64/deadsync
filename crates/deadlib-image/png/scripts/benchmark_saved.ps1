param(
    [Parameter(Mandatory=$true)][string]$Manifest,
    [Parameter(Mandatory=$true)][string]$Executable,
    [Parameter(Mandatory=$true)][string]$LogPrefix,
    [string]$CorpusDirectory,
    [int]$Iterations = 0,
    [int]$Runs = 3
)
$ErrorActionPreference = 'Stop'
$manifestFile = Get-Item -LiteralPath $Manifest
$evidence = Get-Content -LiteralPath $manifestFile.FullName -Raw -Encoding UTF8 | ConvertFrom-Json
$binary = Get-Item -LiteralPath $Executable
$binaryHash = (Get-FileHash -LiteralPath $binary.FullName -Algorithm SHA256).Hash
if ($binaryHash -ne $evidence.executable_sha256) { throw 'Saved executable does not match original manifest' }
if ($Iterations -eq 0) { $Iterations = $evidence.iterations }
if ($Iterations -lt 1 -or $Runs -lt 1) { throw 'Iterations and Runs must be positive' }
$usesCorpus = $evidence.target -in @('corpus','inflate','decode')
if ($usesCorpus) {
    $CorpusDirectory = (Resolve-Path -LiteralPath $CorpusDirectory).Path
    $files = @(Get-ChildItem -LiteralPath $CorpusDirectory -Filter '*.png' | Sort-Object Name)
    if ($files.Count -ne $evidence.corpus_sha256.Count) { throw 'Corpus file count differs from original manifest' }
    foreach ($entry in $evidence.corpus_sha256) {
        $file = Get-Item -LiteralPath (Join-Path $CorpusDirectory $entry.file)
        if ($file.Length -ne $entry.bytes -or (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash -ne $entry.sha256) {
            throw ('Corpus file differs: ' + $entry.file)
        }
    }
}
$prefix = [System.IO.Path]::GetFullPath($LogPrefix)
if (Test-Path -LiteralPath ($prefix + '-manifest.json')) { throw 'Evidence already exists' }
for ($i=1; $i -le $Runs; $i++) {
    if (Test-Path -LiteralPath ($prefix + '-run' + $i + '.tsv')) { throw 'Evidence already exists' }
}
New-Item -ItemType Directory -Path (Split-Path -Parent $prefix) -Force | Out-Null
$eligible = [System.Diagnostics.Process]::GetCurrentProcess().ProcessorAffinity.ToInt64()
if (($eligible -band $evidence.affinity) -ne $evidence.affinity) { throw 'Original CPU affinity is not available' }
$utf8 = New-Object System.Text.UTF8Encoding($false)
$records = @()
for ($i=1; $i -le $Runs; $i++) {
    $start = [System.Diagnostics.ProcessStartInfo]::new()
    $start.FileName = $binary.FullName
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    $start.EnvironmentVariables['DEADLIB_PNG_BENCH_ITERS'] = $Iterations.ToString()
    $start.EnvironmentVariables['DEADLIB_PNG_BENCH_STAGE'] = $evidence.stage
    if ($usesCorpus) { $start.EnvironmentVariables['DEADLIB_PNG_CORPUS'] = $CorpusDirectory }
    $process = [System.Diagnostics.Process]::new()
    $process.StartInfo = $start
    $began = [DateTime]::UtcNow
    [void]$process.Start()
    try {
        $process.ProcessorAffinity = [IntPtr]$evidence.affinity
        if ($process.ProcessorAffinity.ToInt64() -ne $evidence.affinity) { throw 'CPU affinity assignment failed' }
        $stdout = $process.StandardOutput.ReadToEndAsync()
        $stderr = $process.StandardError.ReadToEndAsync()
        $process.WaitForExit()
        $text = $stdout.GetAwaiter().GetResult().Replace("`r`n", "`n")
        $errors = $stderr.GetAwaiter().GetResult()
        if ($process.ExitCode -ne 0) { throw ('Saved benchmark failed: ' + $errors) }
        $path = $prefix + '-run' + $i + '.tsv'
        [System.IO.File]::WriteAllText($path, $text, $utf8)
        $records += [ordered]@{ run=$i; cpu=$evidence.cpu; affinity=$evidence.affinity; start_utc=$began.ToString('o'); end_utc=[DateTime]::UtcNow.ToString('o'); output=$path; stderr=$errors }
    } finally {
        if (-not $process.HasExited) { $process.Kill(); $process.WaitForExit() }
        $process.Dispose()
    }
    Write-Output ('Completed saved run ' + $i + ': ' + $path)
}
# Carry forward the source/toolchain hashes belonging to the verified binary,
# rather than attaching the current checkout's potentially different source.
$evidence.iterations = $Iterations
$evidence.runs = $Runs
$evidence.executable = $binary.FullName
$evidence.measurements = $records
$evidence | Add-Member -NotePropertyName control_of -NotePropertyValue $manifestFile.FullName
$evidence | Add-Member -NotePropertyName control_manifest_sha256 -NotePropertyValue (Get-FileHash -LiteralPath $manifestFile.FullName -Algorithm SHA256).Hash
$evidence | Add-Member -NotePropertyName control_runner_sha256 -NotePropertyValue (Get-FileHash -LiteralPath $PSCommandPath -Algorithm SHA256).Hash
[System.IO.File]::WriteAllText($prefix + '-manifest.json', (($evidence | ConvertTo-Json -Depth 9).Replace("`r`n", "`n") + "`n"), $utf8)
