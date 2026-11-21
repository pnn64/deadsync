$ErrorActionPreference = 'Stop'
$bmpTask = Join-Path (Get-Location) 'target/bmp-decode-opt-20261120-pass2'
$bmpPattern = '^(cargo|rustc|rustdoc|clippy-driver|cl|clang|clang-cl|link|lld|lld-link|ninja|cmake|native_snapshot)$|bench|timing'
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
Save-BmpBoundary -Name 'baseline-boundary-before'
& (Join-Path $bmpTask 'measure.ps1') -Executable target/bmp-decode-opt-20261120-pass2/baseline/bmp-timing.exe -Label baseline-decode -Mode decode
& (Join-Path $bmpTask 'measure.ps1') -Executable target/bmp-decode-opt-20261120-pass2/baseline/bmp-timing.exe -Label baseline-owned-rgba -Mode owned-rgba
& (Join-Path $bmpTask 'measure.ps1') -Executable target/bmp-decode-opt-20261120-pass2/baseline/bmp-timing.exe -Label baseline-rgba -Mode rgba
& (Join-Path $bmpTask 'measure.ps1') -Executable target/bmp-decode-opt-20261120-pass2/baseline/bmp-timing.exe -Label baseline-corpus -Mode owned-rgba -Corpus
Save-BmpBoundary -Name 'baseline-boundary-after'
Write-Output 'BMP baseline timing lease complete.'
