param(
    [Parameter(Mandatory=$true)][string]$Executable,
    [Parameter(Mandatory=$true)][string]$Label,
    [ValidateSet('decode','owned-rgba','rgba','convert','all')][string]$Mode = 'owned-rgba',
    [int]$Milliseconds = 20,
    [int]$Samples = 5,
    [int]$Cpu = 2,
    [string]$Filter = '',
    [string]$Sizes = '',
    [string[]]$Files = @(),
    [switch]$Allocations,
    [switch]$Corpus
)
$ErrorActionPreference = 'Stop'
$bmpRoot = (Get-Location).Path
$bmpTask = Join-Path $bmpRoot 'target/bmp-decode-opt-20261120-pass2'
if ($Label -notmatch '^[a-zA-Z0-9_-]+$') { throw 'Label must be a plain artifact name.' }
if ($Milliseconds -le 0 -or $Samples -le 0) { throw 'Batch duration and count must be positive.' }
$bmpExe = (Resolve-Path -LiteralPath $Executable).Path
if ($Allocations -and [System.IO.Path]::GetFileName($bmpExe) -ne 'bmp-allocations.exe') {
    throw 'Allocation runs require the separately built bmp-allocations.exe.'
}
$bmpPrefix = Join-Path $bmpTask $Label
foreach ($bmpExtension in @('.tsv','.log','.json')) {
    if (Test-Path -LiteralPath ($bmpPrefix + $bmpExtension)) { throw "Refusing to overwrite $bmpPrefix$bmpExtension" }
}
$env:BMP_BENCH_CPU = $Cpu.ToString()
$env:BMP_BENCH_MS = $Milliseconds.ToString()
$env:BMP_BENCH_SAMPLES = $Samples.ToString()
if ($Sizes) { $env:BMP_BENCH_SIZES = $Sizes } else { Remove-Item Env:BMP_BENCH_SIZES -ErrorAction SilentlyContinue }
$bmpArguments = @('--mode',$Mode)
if ($Filter) { $bmpArguments += @('--filter',$Filter) }
if ($Corpus) {
    if ($Files.Count -ne 0) { throw 'Use either the repository corpus or explicit files.' }
    $bmpArguments += @('tests/fixtures/itgmania-song-lua-micro/indexed-bitmap-files','tests/fixtures/itgmania-primary-bitmaps','tests/fixtures/itgmania-song-lua-micro/indexed-bitmap-files/misnamed.png')
}
if ($Files.Count -ne 0) { $bmpArguments += $Files }
$bmpStart = [System.Diagnostics.ProcessStartInfo]::new()
$bmpStart.FileName = $bmpExe
$bmpStart.Arguments = ($bmpArguments | ForEach-Object { '"' + $_ + '"' }) -join ' '
$bmpStart.WorkingDirectory = $bmpRoot
$bmpStart.UseShellExecute = $false
$bmpStart.CreateNoWindow = $true
$bmpStart.RedirectStandardOutput = $true
$bmpStart.RedirectStandardError = $true
$bmpRecord = [ordered]@{
    started_utc=[DateTime]::UtcNow.ToString('o')
    executable=$bmpExe
    executable_sha256=(Get-FileHash -LiteralPath $bmpExe -Algorithm SHA256).Hash.ToLowerInvariant()
    arguments=$bmpArguments
    milliseconds=$Milliseconds
    samples=$Samples
    cpu=$Cpu
    sizes=$Sizes
    head=(& git rev-parse HEAD)
}
if ($Allocations) {
    $bmpRecord['measurement'] = 'allocation counters only'
    $bmpRecord['milliseconds'] = $null
    $bmpRecord['samples'] = $null
}
$bmpProcess = [System.Diagnostics.Process]::Start($bmpStart)
$bmpStdout = $bmpProcess.StandardOutput.ReadToEndAsync()
$bmpStderr = $bmpProcess.StandardError.ReadToEndAsync()
$bmpProcess.WaitForExit()
[System.IO.File]::WriteAllText("$bmpPrefix.tsv", $bmpStdout.Result, [System.Text.UTF8Encoding]::new($false))
[System.IO.File]::WriteAllText("$bmpPrefix.log", $bmpStderr.Result, [System.Text.UTF8Encoding]::new($false))
$bmpRecord['completed_utc'] = [DateTime]::UtcNow.ToString('o')
$bmpRecord['exit_code'] = $bmpProcess.ExitCode
[System.IO.File]::WriteAllText("$bmpPrefix.json", (ConvertTo-Json -InputObject $bmpRecord -Depth 4), [System.Text.UTF8Encoding]::new($false))
if ($bmpProcess.ExitCode -ne 0) { throw "BMP measurement failed ($($bmpProcess.ExitCode)); see $bmpPrefix.log" }
$bmpProcess.Dispose()
if ($Allocations) {
    Write-Output "Completed $Label ($Mode, allocation counts only)."
} else {
    Write-Output "Completed $Label ($Mode, $Samples batches x $Milliseconds ms, CPU $Cpu)."
}
