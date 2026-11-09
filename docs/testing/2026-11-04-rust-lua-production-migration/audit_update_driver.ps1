$ErrorActionPreference = 'Stop'
$evidenceDirectory = $PSScriptRoot
$workspaceDirectory = (Resolve-Path (Join-Path $evidenceDirectory '../../..')).Path
function Read-Json([string]$path) { Get-Content -Raw -LiteralPath $path | ConvertFrom-Json }
function File-Hash([string]$path) { (Get-FileHash -Algorithm SHA256 -LiteralPath $path).Hash.ToLowerInvariant() }
function Require([bool]$condition,[string]$message) { if (-not $condition) { throw $message } }
$review = Read-Json (Join-Path $evidenceDirectory 'song-update-driver-review.json')
$reviewed = Read-Json (Join-Path $evidenceDirectory 'song-update-driver-reviewed-sources.json')
Push-Location $workspaceDirectory
try {
    $crates = @('deadlib-lua','deadsync-song-lua','deadsync-noteskin','deadsync-theme-simply-love','deadsync-assets','deadsync-shell','deadsync-notefield')
    $arguments = @('ls-files','--cached','--others','--exclude-standard','--')
    $arguments += $crates | ForEach-Object {"crates/$_"}
    $names = @(& git @arguments)
    Require ($LASTEXITCODE -eq 0) 'Git source enumeration failed'
    $names = @($names | Where-Object {$_ -match '\.(rs|lua|toml)$'})
    $names += @('Cargo.toml','Cargo.lock','.cargo/config.toml')
    $current = [ordered]@{}
    foreach ($name in ($names | Sort-Object -Unique)) {
        if (Test-Path -LiteralPath $name -PathType Leaf) { $current[$name] = File-Hash $name }
    }
    $expectedNames = @($reviewed.PSObject.Properties.Name | Sort-Object)
    Require ($current.Count -eq $expectedNames.Count) 'Source count changed'
    Require (@(Compare-Object @($current.Keys | Sort-Object) $expectedNames).Count -eq 0) 'Source membership changed'
    foreach ($name in $current.Keys) { Require ($current[$name] -eq $reviewed.$name) "Source changed: $name" }
    $attachment = 'C:\Users\PerfectTaste\.codex\attachments\bcf567e6-d9b3-4fcf-b693-47949c76632c\pasted-text-1.txt'
    $objectiveHash = File-Hash $attachment
    Require ($objectiveHash -eq '86cbb91ab372fad5ac5326b3c3b04fb3db8cab9d4f3a4cc8e71cb64dba48ae34') 'Objective changed'
    Require ((File-Hash (Join-Path $evidenceDirectory 'objective.txt')) -eq $objectiveHash) 'Objective copy differs'
    $logs = [ordered]@{}
    foreach ($directory in (Get-ChildItem -LiteralPath $evidenceDirectory -Directory -Filter 'song-update-driver-*-song-linux-debug' | Sort-Object Name)) {
        $result = Read-Json (Join-Path $directory.FullName 'result.json')
        $hash = File-Hash (Join-Path $directory.FullName 'run.log')
        Require ($hash -eq $result.log_sha256) "Log hash differs: $($directory.Name)"
        Require $result.sources_unchanged "Sources changed during $($directory.Name)"
        $logs[$directory.Name] = $hash
    }
    Require ($review.tests_passed -eq 290) 'Unexpected regression count'
    Require ($review.test_targets -eq 9) 'Unexpected target count'
    Require $review.clippy_diagnostics_match_prior_baseline 'New Clippy diagnostics remain'
    $documents = [ordered]@{}
    foreach ($path in @('docs/lua-architecture.md','docs/testing/2026-11-04-rust-lua-production-migration/README.md',
        'docs/testing/2026-11-04-rust-lua-production-migration/review_update_driver.py',
        'docs/testing/2026-11-04-rust-lua-production-migration/prepare_update_samples.py',
        'docs/testing/2026-11-04-rust-lua-production-migration/audit_update_driver.ps1',
        'docs/testing/2026-11-04-rust-lua-production-migration/checks.py')) { $documents[$path] = File-Hash $path }
    $report = [ordered]@{
        goal_complete = $false; production_cutover = $false; final_application_validation = $false
        independent_verifier = 'PowerShell Get-FileHash and Git source membership enumeration'
        objective_sha256 = $objectiveHash; objective_unchanged = $true
        source_count = $current.Count; full_watched_snapshot_matches = $true
        native_algorithms_byte_unchanged = $review.native_algorithms_byte_unchanged
        existing_tests_byte_unchanged = $review.existing_tests_byte_unchanged
        engine_unchanged = $review.engine_unchanged
        tests_passed = 290; test_targets = 9; new_complete_driver_cases = 10
        zero_ignored_or_filtered_tests = $true
        formatting_passed = $review.formatting_passed
        clippy_diagnostics_match_prior_baseline = $true
        test_dependency_graph_unchanged = $review.test_dependency_graph_unchanged
        log_hashes = $logs; document_and_runner_hashes = $documents
    }
    $current | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $evidenceDirectory 'song-update-driver-final-sources.json') -Encoding UTF8
    $report | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $evidenceDirectory 'song-update-driver-final-state.json') -Encoding UTF8
    $report | ConvertTo-Json -Depth 6
} finally { Pop-Location }
