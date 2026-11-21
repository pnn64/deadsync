$ErrorActionPreference = 'Stop'
$bmpRoot = (Get-Location).Path
$bmpTask = Join-Path $bmpRoot 'target/bmp-decode-opt-20261120-pass2'
$bmpBaseline = Join-Path $bmpTask 'baseline'
if (Test-Path -LiteralPath $bmpBaseline) { throw 'Baseline directory already exists; preparation is not replayable.' }
Start-Transcript -LiteralPath (Join-Path $bmpTask 'preparation.log') -NoClobber | Out-Null
New-Item -ItemType Directory -Path $bmpBaseline | Out-Null
New-Item -ItemType Directory -Path (Join-Path $bmpBaseline 'sources') | Out-Null
New-Item -ItemType Directory -Path (Join-Path $bmpBaseline 'inputs') | Out-Null

function Invoke-BmpCaptured {
    param([string]$Executable, [string[]]$Arguments, [string]$OutputPrefix)
    $bmpStart = [System.Diagnostics.ProcessStartInfo]::new()
    $bmpStart.FileName = $Executable
    $bmpStart.Arguments = ($Arguments | ForEach-Object { '"' + $_ + '"' }) -join ' '
    $bmpStart.WorkingDirectory = $bmpRoot
    $bmpStart.UseShellExecute = $false
    $bmpStart.CreateNoWindow = $true
    $bmpStart.RedirectStandardOutput = $true
    $bmpStart.RedirectStandardError = $true
    $bmpProcess = [System.Diagnostics.Process]::Start($bmpStart)
    $bmpStdout = $bmpProcess.StandardOutput.ReadToEndAsync()
    $bmpStderr = $bmpProcess.StandardError.ReadToEndAsync()
    $bmpProcess.WaitForExit()
    [System.IO.File]::WriteAllText("$OutputPrefix.tsv", $bmpStdout.Result, [System.Text.UTF8Encoding]::new($false))
    [System.IO.File]::WriteAllText("$OutputPrefix.log", $bmpStderr.Result, [System.Text.UTF8Encoding]::new($false))
    if ($bmpProcess.ExitCode -ne 0) { throw "BMP invocation failed ($($bmpProcess.ExitCode)): $($bmpStart.Arguments); see $OutputPrefix.log" }
    Write-Output "Completed: $($bmpStart.Arguments) -> $OutputPrefix"
    $bmpProcess.Dispose()
}

$bmpFiles = @(
    Get-ChildItem -LiteralPath 'crates/deadlib-image/bmp/src','crates/deadlib-image/bmp/tests','crates/deadlib-image/bmp/bench/src' -File -Recurse
    Get-Item -LiteralPath 'crates/deadlib-image/bmp/Cargo.toml','crates/deadlib-image/bmp/README.md','crates/deadlib-image/bmp/bench/Cargo.toml','crates/deadlib-image/bmp/bench/Cargo.lock','crates/deadlib-image/bmp/bench/README.md','crates/deadlib-image/bmp/bench/analyze.py','.cargo/config.toml','Cargo.toml','Cargo.lock','rust-performance.md'
) | Sort-Object FullName -Unique
$bmpSourceIdentity = @(foreach ($bmpFile in $bmpFiles) {
    $bmpRelative = $bmpFile.FullName.Substring($bmpRoot.Length + 1).Replace('\','/')
    $bmpDestination = Join-Path (Join-Path $bmpBaseline 'sources') $bmpRelative
    New-Item -ItemType Directory -Path (Split-Path -Parent $bmpDestination) -Force | Out-Null
    [System.IO.File]::Copy($bmpFile.FullName, $bmpDestination, $false)
    [pscustomobject]@{path=$bmpRelative; bytes=$bmpFile.Length; sha256=(Get-FileHash -LiteralPath $bmpFile.FullName -Algorithm SHA256).Hash.ToLowerInvariant()}
})
$bmpInputs = @(
    Get-ChildItem -LiteralPath 'tests/fixtures/itgmania-song-lua-micro/indexed-bitmap-files','tests/fixtures/itgmania-primary-bitmaps' -File | Where-Object { $_.Extension -ieq '.bmp' }
    Get-Item -LiteralPath 'tests/fixtures/itgmania-song-lua-micro/indexed-bitmap-files/misnamed.png'
) | Sort-Object FullName -Unique
$bmpInputIdentity = @(foreach ($bmpFile in $bmpInputs) {
    $bmpRelative = $bmpFile.FullName.Substring($bmpRoot.Length + 1).Replace('\','/')
    $bmpDestination = Join-Path (Join-Path $bmpBaseline 'inputs') $bmpRelative
    New-Item -ItemType Directory -Path (Split-Path -Parent $bmpDestination) -Force | Out-Null
    [System.IO.File]::Copy($bmpFile.FullName, $bmpDestination, $false)
    [pscustomobject]@{path=$bmpRelative; bytes=$bmpFile.Length; sha256=(Get-FileHash -LiteralPath $bmpFile.FullName -Algorithm SHA256).Hash.ToLowerInvariant()}
})
$bmpIdentity = [ordered]@{
    captured_utc=[DateTime]::UtcNow.ToString('o')
    root=$bmpRoot
    head=(& git rev-parse HEAD)
    bmp_status=@(& git status --porcelain -- 'crates/deadlib-image/bmp')
    rustc=@(& rustc -Vv)
    cargo=(& cargo -V)
    environment=[ordered]@{RUSTFLAGS=$env:RUSTFLAGS; CARGO_ENCODED_RUSTFLAGS=$env:CARGO_ENCODED_RUSTFLAGS; CARGO_TARGET_DIR=$env:CARGO_TARGET_DIR; RUSTC_WRAPPER=$env:RUSTC_WRAPPER}
    cpu=@(Get-CimInstance Win32_Processor | Select-Object Name,NumberOfCores,NumberOfLogicalProcessors)
    os=[Environment]::OSVersion.VersionString
    sources=$bmpSourceIdentity
    inputs=$bmpInputIdentity
}
$bmpJson = ConvertTo-Json -InputObject $bmpIdentity -Depth 8
[System.IO.File]::WriteAllText((Join-Path $bmpBaseline 'identity.json'), $bmpJson, [System.Text.UTF8Encoding]::new($false))
Write-Output "FROZEN: $($bmpSourceIdentity.Count) source/config files and $($bmpInputIdentity.Count) corpus inputs at $($bmpIdentity.head)"

& cargo build --manifest-path crates/deadlib-image/bmp/bench/Cargo.toml --release --locked --offline --bins -j 1 --target-dir target/bmp-decode-opt-20261120-pass2/timing
if ($LASTEXITCODE -ne 0) { throw "Timing build failed: $LASTEXITCODE" }
[System.IO.File]::Copy((Join-Path $bmpTask 'timing/release/bmp-codec-bench.exe'), (Join-Path $bmpBaseline 'bmp-timing.exe'), $false)
[System.IO.File]::Copy((Join-Path $bmpTask 'timing/release/native_snapshot.exe'), (Join-Path $bmpBaseline 'native_snapshot.exe'), $false)

& cargo build --manifest-path crates/deadlib-image/bmp/bench/Cargo.toml --release --locked --offline --features allocations --bin bmp-codec-bench -j 1 --target-dir target/bmp-decode-opt-20261120-pass2/allocations
if ($LASTEXITCODE -ne 0) { throw "Allocation build failed: $LASTEXITCODE" }
[System.IO.File]::Copy((Join-Path $bmpTask 'allocations/release/bmp-codec-bench.exe'), (Join-Path $bmpBaseline 'bmp-allocations.exe'), $false)

$bmpExeIdentity = @(foreach ($bmpExe in @('bmp-timing.exe','bmp-allocations.exe','native_snapshot.exe')) {
    $bmpFile = Get-Item -LiteralPath (Join-Path $bmpBaseline $bmpExe)
    [pscustomobject]@{path=$bmpExe; bytes=$bmpFile.Length; sha256=(Get-FileHash -LiteralPath $bmpFile.FullName -Algorithm SHA256).Hash.ToLowerInvariant()}
})
[System.IO.File]::WriteAllText((Join-Path $bmpBaseline 'executables.json'), (ConvertTo-Json -InputObject $bmpExeIdentity -Depth 3), [System.Text.UTF8Encoding]::new($false))
Write-Output 'PRESERVED timing, allocation, and snapshot executables.'

& (Join-Path $bmpBaseline 'native_snapshot.exe') capture (Join-Path $bmpBaseline 'native.snapshot') | Tee-Object -FilePath (Join-Path $bmpTask 'baseline-snapshot.log')
if ($LASTEXITCODE -ne 0) { throw "Snapshot capture failed: $LASTEXITCODE" }
Get-FileHash -LiteralPath (Join-Path $bmpBaseline 'native.snapshot') -Algorithm SHA256 | Format-List | Out-File -LiteralPath (Join-Path $bmpTask 'baseline-snapshot-sha256.txt')

$env:BMP_BENCH_CPU = '2'
Remove-Item Env:BMP_BENCH_SIZES -ErrorAction SilentlyContinue
Invoke-BmpCaptured -Executable (Join-Path $bmpBaseline 'bmp-timing.exe') -Arguments @('--verify-only','--mode','owned-rgba') -OutputPrefix (Join-Path $bmpTask 'baseline-verify-synthetic')
$bmpCorpusArgs = @('tests/fixtures/itgmania-song-lua-micro/indexed-bitmap-files','tests/fixtures/itgmania-primary-bitmaps','tests/fixtures/itgmania-song-lua-micro/indexed-bitmap-files/misnamed.png')
Invoke-BmpCaptured -Executable (Join-Path $bmpBaseline 'bmp-timing.exe') -Arguments (@('--verify-only','--mode','owned-rgba') + $bmpCorpusArgs) -OutputPrefix (Join-Path $bmpTask 'baseline-verify-corpus')

foreach ($bmpMode in @('decode','owned-rgba','rgba')) {
    Invoke-BmpCaptured -Executable (Join-Path $bmpBaseline 'bmp-allocations.exe') -Arguments @('--mode',$bmpMode) -OutputPrefix (Join-Path $bmpTask "baseline-$bmpMode-allocations")
}
Invoke-BmpCaptured -Executable (Join-Path $bmpBaseline 'bmp-allocations.exe') -Arguments (@('--mode','owned-rgba') + $bmpCorpusArgs) -OutputPrefix (Join-Path $bmpTask 'baseline-corpus-allocations')
Write-Output 'BASELINE verification and allocation-only runs complete; no timings were run.'

& cargo test --manifest-path target/bmp-decode-opt-20261120-pass2/validation/Cargo.toml --release --offline -j 1 --target-dir target/bmp-decode-opt-20261120-pass2/check
if ($LASTEXITCODE -ne 0) { throw "Initial BMP tests failed: $LASTEXITCODE" }
Write-Output 'READY: preparation completed without timed measurements or production edits.'
Stop-Transcript | Out-Null
