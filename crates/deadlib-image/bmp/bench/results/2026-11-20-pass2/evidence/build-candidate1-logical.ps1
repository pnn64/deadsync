param([string]$Stage = 'candidate1-logical')
$ErrorActionPreference = 'Stop'
$bmpTask = Join-Path (Get-Location) 'target/bmp-decode-opt-20261120-pass2'
$bmpStage = Join-Path $bmpTask $Stage
foreach ($bmpName in @('bmp-timing.exe','bmp-allocations.exe','native_snapshot.exe','executables.json','source-before-build.json')) {
    if (Test-Path -LiteralPath (Join-Path $bmpStage $bmpName)) { throw "Candidate preparation already started: $bmpName" }
}
Start-Transcript -LiteralPath (Join-Path $bmpTask "$Stage-build.log") -NoClobber | Out-Null
& python target/bmp-decode-opt-20261120-pass2/verify-stage.py $Stage before-build
if ($LASTEXITCODE -ne 0) { throw "Pre-build source identity mismatch: $LASTEXITCODE" }
& rustfmt --check --edition 2024 crates/deadlib-image/bmp/src/decode.rs crates/deadlib-image/bmp/tests/bmp_optimized.rs target/bmp-decode-opt-20261120-pass2/validation/alpha_controls.rs
if ($LASTEXITCODE -ne 0) { throw "Rust formatting check failed: $LASTEXITCODE" }
& cargo build --manifest-path crates/deadlib-image/bmp/bench/Cargo.toml --release --locked --offline --bins -j 2 --target-dir target/bmp-decode-opt-20261120-pass2/timing
if ($LASTEXITCODE -ne 0) { throw "Timing build failed: $LASTEXITCODE" }
[System.IO.File]::Copy((Join-Path $bmpTask 'timing/release/bmp-codec-bench.exe'), (Join-Path $bmpStage 'bmp-timing.exe'), $false)
[System.IO.File]::Copy((Join-Path $bmpTask 'timing/release/native_snapshot.exe'), (Join-Path $bmpStage 'native_snapshot.exe'), $false)
& cargo build --manifest-path crates/deadlib-image/bmp/bench/Cargo.toml --release --locked --offline --features allocations --bin bmp-codec-bench -j 2 --target-dir target/bmp-decode-opt-20261120-pass2/allocations
if ($LASTEXITCODE -ne 0) { throw "Allocation build failed: $LASTEXITCODE" }
[System.IO.File]::Copy((Join-Path $bmpTask 'allocations/release/bmp-codec-bench.exe'), (Join-Path $bmpStage 'bmp-allocations.exe'), $false)
$bmpExecutables = @(foreach ($bmpName in @('bmp-timing.exe','bmp-allocations.exe','native_snapshot.exe')) {
    $bmpFile = Get-Item -LiteralPath (Join-Path $bmpStage $bmpName)
    [pscustomobject]@{path=$bmpName; bytes=$bmpFile.Length; sha256=(Get-FileHash -LiteralPath $bmpFile.FullName -Algorithm SHA256).Hash.ToLowerInvariant()}
})
[System.IO.File]::WriteAllText((Join-Path $bmpStage 'executables.json'), (ConvertTo-Json -InputObject $bmpExecutables -Depth 3), [System.Text.UTF8Encoding]::new($false))
& (Join-Path $bmpStage 'native_snapshot.exe') verify (Join-Path $bmpTask 'baseline/native.snapshot') | Tee-Object -FilePath (Join-Path $bmpTask "$Stage-snapshot.log")
if ($LASTEXITCODE -ne 0) { throw "Exact snapshot verification failed: $LASTEXITCODE" }
& cargo test --manifest-path target/bmp-decode-opt-20261120-pass2/validation/Cargo.toml --release --locked --offline -j 2 --target-dir target/bmp-decode-opt-20261120-pass2/check
if ($LASTEXITCODE -ne 0) { throw "BMP tests failed: $LASTEXITCODE" }
foreach ($bmpMode in @('decode','owned-rgba','rgba')) {
    & (Join-Path $bmpTask 'measure.ps1') -Executable (Join-Path $bmpStage 'bmp-allocations.exe') -Label "$Stage-$bmpMode-allocations" -Mode $bmpMode -Allocations
}
& (Join-Path $bmpTask 'measure.ps1') -Executable (Join-Path $bmpStage 'bmp-allocations.exe') -Label "$Stage-corpus-allocations" -Mode owned-rgba -Corpus -Allocations
foreach ($bmpMode in @('decode','owned-rgba')) {
    & (Join-Path $bmpTask 'measure.ps1') -Executable (Join-Path $bmpStage 'bmp-allocations.exe') -Label "$Stage-alpha-controls-$bmpMode-allocations" -Mode $bmpMode -Files target/bmp-decode-opt-20261120-pass2/alpha-controls -Allocations
}
& python target/bmp-decode-opt-20261120-pass2/verify-stage.py $Stage after-build
if ($LASTEXITCODE -ne 0) { throw "Post-build source identity mismatch: $LASTEXITCODE" }
Write-Output "BMP $Stage build, snapshot, tests, and allocation-only passes complete. No timings were run."
Stop-Transcript | Out-Null
