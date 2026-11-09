$ErrorActionPreference = 'Stop'
$evidenceDirectory = $PSScriptRoot
$workspaceDirectory = (Resolve-Path (Join-Path $evidenceDirectory '../../..')).Path
$prefix = 'song-frame-handoff'
function Read-Json([string]$path) {
    Get-Content -Raw -LiteralPath $path | ConvertFrom-Json
}
function File-Hash([string]$path) {
    (Get-FileHash -Algorithm SHA256 -LiteralPath $path).Hash.ToLowerInvariant()
}
function Require([bool]$condition, [string]$message) {
    if (-not $condition) { throw $message }
}
$review = Read-Json (Join-Path $evidenceDirectory "$prefix-review.json")
$reviewed = Read-Json (Join-Path $evidenceDirectory "$prefix-reviewed-sources.json")
$crates = @('deadlib-lua','deadsync-song-lua','deadsync-noteskin',
    'deadsync-theme-simply-love','deadsync-assets','deadsync-shell','deadsync-notefield')
Push-Location $workspaceDirectory
try {
    $arguments = @('ls-files','--cached','--others','--exclude-standard','--')
    $arguments += $crates | ForEach-Object { "crates/$_" }
    $names = @(& git @arguments)
    Require ($LASTEXITCODE -eq 0) 'Git enumeration failed'
    $names = @($names | Where-Object { $_ -match '\.(rs|lua|toml)$' })
    $names += @('Cargo.toml','Cargo.lock','.cargo/config.toml')
    $current = [ordered]@{}
    foreach ($name in ($names | Sort-Object -Unique)) {
        $path = Join-Path $workspaceDirectory $name
        if (Test-Path -LiteralPath $path -PathType Leaf) {
            $current[$name] = File-Hash $path
        }
    }
    $expectedNames = @($reviewed.PSObject.Properties.Name | Sort-Object)
    Require ($current.Count -eq $expectedNames.Count) 'Source count changed'
    Require (@(Compare-Object @($current.Keys | Sort-Object) $expectedNames).Count -eq 0) 'Source membership changed'
    foreach ($name in $current.Keys) {
        Require ($current[$name] -eq $reviewed.$name) "Source hash mismatch: $name"
    }
    $attachment = 'C:\Users\PerfectTaste\.codex\attachments\bcf567e6-d9b3-4fcf-b693-47949c76632c\pasted-text-1.txt'
    $objectiveHash = File-Hash $attachment
    Require ($objectiveHash -eq '86cbb91ab372fad5ac5326b3c3b04fb3db8cab9d4f3a4cc8e71cb64dba48ae34') 'Objective changed'
    Require ((File-Hash (Join-Path $evidenceDirectory 'objective.txt')) -eq $objectiveHash) 'Objective copy differs'
    $logs = [ordered]@{}
    foreach ($stage in @('song-frame-handoff-focus-v1-song-linux-debug',
        'song-frame-handoff-regressions-v1-song-linux-debug','song-frame-handoff-lint-v1-song-linux-debug')) {
        $directory = Join-Path $evidenceDirectory $stage
        $result = Read-Json (Join-Path $directory 'result.json')
        $hash = File-Hash (Join-Path $directory 'run.log')
        Require ($hash -eq $result.log_sha256) "Log hash differs: $stage"
        Require $result.sources_unchanged "Sources changed during $stage"
        $logs[$stage] = $hash
    }
    Require ($review.total_tests_passed -eq 1104) 'Unexpected regression count'
    Require ($review.library_tests_passed -eq 849) 'Unexpected library count'
    Require ($review.adapter_tests_passed -eq 255) 'Unexpected adapter count'
    $documents = [ordered]@{}
    foreach ($name in @('docs/lua-architecture.md',
        "docs/testing/2026-11-04-rust-lua-production-migration/README.md",
        "docs/testing/2026-11-04-rust-lua-production-migration/review_frame_handoff.py",
        "docs/testing/2026-11-04-rust-lua-production-migration/prepare_frame_handoff.py",
        "docs/testing/2026-11-04-rust-lua-production-migration/audit_frame_handoff.ps1",
        "docs/testing/2026-11-04-rust-lua-production-migration/checks.py")) {
        $documents[$name] = File-Hash (Join-Path $workspaceDirectory $name)
    }
    $report = [ordered]@{
        goal_complete = $false; production_cutover = $false; final_application_validation = $false
        independent_verifier = 'PowerShell Get-FileHash and Git membership enumeration'
        objective_unchanged = $true; objective_sha256 = $objectiveHash
        source_count = $current.Count; full_watched_snapshot_matches = $true
        watched_dependency_sources_match_review = $true
        existing_tests_byte_unchanged = $review.existing_test_bodies_preserved
        extraction_verified = $review.scalar_routing_and_pose_metadata_extraction_verified
        queue_ownership_preserved_by_frame_reset = $review.queue_ownership_preserved_by_frame_reset
        total_tests_passed = 1104; library_tests_passed = 849; adapter_tests_passed = 255
        test_targets = 6; new_handoff_cases = 8; zero_ignored_or_filtered_tests = $true
        clippy_diagnostics_match_prior_baseline = $review.clippy_diagnostics_match_prior_baseline
        source_diff_sha256 = File-Hash (Join-Path $evidenceDirectory "$prefix-source-diff.diff")
        log_hashes = $logs; document_and_runner_hashes = $documents
    }
    $current | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $evidenceDirectory "$prefix-final-sources.json") -Encoding UTF8
    $report | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $evidenceDirectory "$prefix-final-state.json") -Encoding UTF8
    $report | ConvertTo-Json -Depth 6
} finally {
    Pop-Location
}
