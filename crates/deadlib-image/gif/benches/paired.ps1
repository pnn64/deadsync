# Alternate saved pipeline executables on the same pinned CPU.
# Build both with the same harness/profile, then run from the workspace root:
# ./crates/deadlib-image/gif/benches/paired.ps1 before.exe after.exe out.tsv composite tiles16-palette solid2-palette
param(
    [Parameter(Mandatory)] [string] $Before,
    [Parameter(Mandatory)] [string] $After,
    [Parameter(Mandatory)] [string] $Output,
    [Parameter(Mandatory)] [string] $Operation,
    [Parameter(Mandatory, ValueFromRemainingArguments)] [string[]] $Cases
)
$ErrorActionPreference = 'Stop'
$env:DEADLIB_GIF_BENCH_NATIVE_ONLY = '1'
$env:DEADLIB_GIF_BENCH_ITERS = '40'
$env:DEADLIB_GIF_BENCH_SAMPLES = '7'
$env:DEADLIB_GIF_BENCH_OPERATION = $Operation
$binaries = @((Resolve-Path -LiteralPath $Before).Path, (Resolve-Path -LiteralPath $After).Path)
$rows = @()
function Median($Values) {
    $sorted = @($Values | Sort-Object)
    $sorted[[int][Math]::Floor($sorted.Count / 2)]
}
foreach ($case in $Cases) {
    $env:DEADLIB_GIF_BENCH_CASE = $case
    for ($pair = 0; $pair -lt 6; $pair++) {
        foreach ($variant in @(($pair % 2), (1 - $pair % 2))) {
            $measured = @(& $binaries[$variant] | ConvertFrom-Csv -Delimiter "`t")
            if ($LASTEXITCODE -ne 0 -or $measured.Count -ne 1) { throw 'Benchmark failed' }
            $measured[0] | Add-Member -NotePropertyName pair -NotePropertyValue $pair
            $measured[0] | Add-Member -NotePropertyName variant -NotePropertyValue @('before', 'after')[$variant]
            $rows += $measured[0]
        }
    }
    foreach ($metric in @('median_cycles', 'median_ns', 'allocations', 'allocated_bytes')) {
        $values = foreach ($variant in @('before', 'after')) {
            Median @($rows | Where-Object { $_.case -eq $case -and $_.variant -eq $variant } | ForEach-Object { [double]$_.$metric })
        }
        Write-Output ("{0} {1} {2}: {3:g} -> {4:g}" -f $case, $Operation, $metric, $values[0], $values[1])
    }
    $ratios = foreach ($pair in 0..5) {
        $matched = @($rows | Where-Object { $_.case -eq $case -and [int]$_.pair -eq $pair })
        $old = [double]($matched | Where-Object variant -eq before).median_cycles
        $new = [double]($matched | Where-Object variant -eq after).median_cycles
        if ($new -gt 0) { $old / $new }
    }
    Write-Output ("{0} {1} median paired CPU throughput: {2:f3}x" -f $case, $Operation, (Median $ratios))
    $rows | Export-Csv -LiteralPath $Output -Delimiter "`t" -NoTypeInformation
}
