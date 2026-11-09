$ErrorActionPreference = 'Stop'
$taskRoot = (Resolve-Path (Join-Path $PSScriptRoot '../../..')).Path
$review = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'song-compiler-facade-review.json') -Raw | ConvertFrom-Json
$expected = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'song-compiler-facade-reviewed-sources.json') -Raw | ConvertFrom-Json
$crateNames = @('deadlib-lua', 'deadsync-song-lua', 'deadsync-noteskin', 'deadsync-theme-simply-love', 'deadsync-assets', 'deadsync-shell', 'deadsync-notefield')
$cratePaths = $crateNames | ForEach-Object { 'crates/' + $_ }
$sourceNames = & git -C $taskRoot ls-files --cached --others --exclude-standard -- $cratePaths
if ($LASTEXITCODE -ne 0) { throw 'Source inventory failed' }
$sourceNames = @($sourceNames | Where-Object { [IO.Path]::GetExtension($_) -in @('.rs', '.lua', '.toml') }) + @('Cargo.toml', 'Cargo.lock', '.cargo/config.toml')
$current = [ordered]@{}
foreach ($name in ($sourceNames | Sort-Object -Unique)) {
    $path = Join-Path $taskRoot $name
    if (Test-Path -LiteralPath $path -PathType Leaf) {
        $current[$name] = (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant()
    }
}
if ($current.Count -ne @($expected.PSObject.Properties).Count) { throw 'Source membership changed' }
foreach ($entry in $expected.PSObject.Properties) {
    if ($current[$entry.Name] -ne $entry.Value) { throw ('Source mismatch: ' + $entry.Name) }
}
$objective = 'C:\Users\PerfectTaste\.codex\attachments\bcf567e6-d9b3-4fcf-b693-47949c76632c\pasted-text-1.txt'
$objectiveHash = (Get-FileHash -LiteralPath $objective -Algorithm SHA256).Hash.ToLowerInvariant()
if ($objectiveHash -ne '86cbb91ab372fad5ac5326b3c3b04fb3db8cab9d4f3a4cc8e71cb64dba48ae34') { throw 'Objective changed' }
foreach ($stage in $review.results.PSObject.Properties) {
    $path = Join-Path $PSScriptRoot ($stage.Name + '/run.log')
    if ((Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant() -ne $stage.Value.log_sha256) { throw ('Log mismatch: ' + $stage.Name) }
}
$priorLog = Join-Path $PSScriptRoot ($review.prior_run.stage + '-song-linux-debug/run.log')
if ((Get-FileHash -LiteralPath $priorLog -Algorithm SHA256).Hash.ToLowerInvariant() -ne $review.prior_run.log_sha256) { throw 'Prior library log mismatch' }
$documentHashes = [ordered]@{}
foreach ($name in @('docs/lua-architecture.md', 'docs/testing/2026-11-04-rust-lua-production-migration/README.md', 'docs/testing/2026-11-04-rust-lua-production-migration/review_compiler_facade.py', 'docs/testing/2026-11-04-rust-lua-production-migration/audit_compiler_facade.ps1', 'docs/testing/2026-11-04-rust-lua-production-migration/checks.py')) {
    $documentHashes[$name] = (Get-FileHash -LiteralPath (Join-Path $taskRoot $name) -Algorithm SHA256).Hash.ToLowerInvariant()
}
$report = [ordered]@{
    goal_complete = $false
    production_cutover = $false
    final_application_validation = $false
    independent_verifier = 'PowerShell Get-FileHash and Git source inventory'
    objective_sha256 = $objectiveHash
    objective_unchanged = $true
    source_count = $current.Count
    full_watched_snapshot_matches = $true
    native_algorithms_byte_unchanged = $review.native_algorithms_byte_unchanged
    existing_tests_byte_unchanged = $review.existing_tests_byte_unchanged
    engine_unchanged = $review.engine_unchanged
    tests_passed = $review.tests_passed
    prior_native_library_tests = $review.prior_native_library_tests
    type_alias_equivalence_byte_verified = $review.type_alias_equivalence_byte_verified
    final_rust_checks_cover_all_212_activation_cases = $review.final_rust_checks_cover_all_212_activation_cases
    test_targets = $review.test_targets
    formatting_passed = $review.formatting_passed
    clippy_diagnostics_match_prior_baseline = $review.clippy_diagnostics_match_prior_baseline
    final_focused_tests = $review.final_focused_tests
    timing_diagnostic_observed = $review.timing_diagnostic_observed
    reference_only_setter_fields_pending = $true
    live_session_integration_pending = $true
    document_and_runner_hashes = $documentHashes
}
$current | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $PSScriptRoot 'song-compiler-facade-final-sources.json') -Encoding utf8
$report | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $PSScriptRoot 'song-compiler-facade-final-state.json') -Encoding utf8
$report | ConvertTo-Json -Depth 6
