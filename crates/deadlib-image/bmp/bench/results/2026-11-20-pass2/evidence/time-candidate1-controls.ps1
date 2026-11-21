param(
    [string]$Stage = 'candidate1-alpha',
    [string]$Label = 'candidate1-controls'
)
$ErrorActionPreference = 'Stop'
$bmpTask = Join-Path (Get-Location) 'target/bmp-decode-opt-20261120-pass2'
$bmpPattern = '^(cargo|rustc|rustdoc|clippy-driver|cl|clang|clang-cl|link|lld|lld-link|ninja|cmake)$|bench|timing|baseline|candidate|snapshot'
function Save-BmpBoundary {
    param([string]$Name)
    $bmpDestination = Join-Path $bmpTask "$Name.json"
    if (Test-Path -LiteralPath $bmpDestination) { throw "Refusing to overwrite $bmpDestination" }
    $bmpBusy = @(Get-Process | Where-Object { $_.ProcessName -match $bmpPattern } | Select-Object Id,ProcessName,CPU)
    $bmpRecord = [ordered]@{captured_utc=[DateTime]::UtcNow.ToString('o'); process_pattern=$bmpPattern; processes=$bmpBusy}
    [System.IO.File]::WriteAllText($bmpDestination, (ConvertTo-Json -InputObject $bmpRecord -Depth 4), [System.Text.UTF8Encoding]::new($false))
    if ($bmpBusy.Count -ne 0) { throw "Concurrent build/benchmark processes at $Name; see boundary record." }
    Write-Output "$Name clean."
}
Save-BmpBoundary -Name "$Label-boundary-before"
& (Join-Path $bmpTask 'measure.ps1') -Executable target/bmp-decode-opt-20261120-pass2/baseline/bmp-timing.exe -Label "$Label-before-decode" -Mode decode -Milliseconds 80 -Files target/bmp-decode-opt-20261120-pass2/alpha-controls
& (Join-Path $bmpTask 'measure.ps1') -Executable (Join-Path (Join-Path $bmpTask $Stage) 'bmp-timing.exe') -Label "$Label-after-decode" -Mode decode -Milliseconds 80 -Files target/bmp-decode-opt-20261120-pass2/alpha-controls
& (Join-Path $bmpTask 'measure.ps1') -Executable (Join-Path (Join-Path $bmpTask $Stage) 'bmp-timing.exe') -Label "$Label-after-owned" -Mode owned-rgba -Milliseconds 80 -Files target/bmp-decode-opt-20261120-pass2/alpha-controls
& (Join-Path $bmpTask 'measure.ps1') -Executable target/bmp-decode-opt-20261120-pass2/baseline/bmp-timing.exe -Label "$Label-before-owned" -Mode owned-rgba -Milliseconds 80 -Files target/bmp-decode-opt-20261120-pass2/alpha-controls
Save-BmpBoundary -Name "$Label-boundary-after"
Write-Output "BMP $Stage control timing lease complete."
