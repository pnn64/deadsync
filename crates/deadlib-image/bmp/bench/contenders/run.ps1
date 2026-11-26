param([Parameter(Mandatory = $true)][string]$Work)

$ErrorActionPreference = 'Stop'
$Work = (Resolve-Path $Work).Path
$exe = Join-Path $Work 'rust-build/release/compare.exe'
$dll = Join-Path $Work 'bridges/Release'
$env:Path = "$dll;$env:Path"
$env:PYTHONPATH = Join-Path $Work 'python'
if (-not $env:BMP_BENCH_CPU) { $env:BMP_BENCH_CPU = '2' }
if (-not $env:BMP_BENCH_MS) { $env:BMP_BENCH_MS = '6' }

foreach ($round in @(1, 2)) {
    if ($round -eq 2) {
        $env:BMP_BENCH_REVERSE = '1'
        $modes = @('encode', 'decode')
    } else {
        Remove-Item Env:BMP_BENCH_REVERSE -ErrorAction SilentlyContinue
        $modes = @('decode', 'encode')
    }
    foreach ($mode in $modes) {
        $manifest = Join-Path $Work "corpus/$mode.tsv"
        Write-Host "Pass $round $mode native"
        & $exe $mode $manifest $dll | Set-Content -Encoding UTF8 "$Work/$mode-$round.tsv"
        if ($LASTEXITCODE -ne 0) { throw "Native $mode failed ($LASTEXITCODE)" }
        Write-Host "Pass $round $mode Python"
        python "$PSScriptRoot/python_contenders.py" $mode $manifest | Set-Content -Encoding UTF8 "$Work/$mode-python-$round.tsv"
        if ($LASTEXITCODE -ne 0) { throw "Python $mode failed ($LASTEXITCODE)" }
    }
}
Remove-Item Env:BMP_BENCH_REVERSE
Write-Host 'Native indexed encode'
& "$Work/rust-build/release/bmp-codec-bench.exe" --mode encode --filter indexed |
    Set-Content -Encoding UTF8 "$Work/indexed.tsv"
if ($LASTEXITCODE -ne 0) { throw "Indexed encode failed ($LASTEXITCODE)" }
