$ErrorActionPreference = 'Stop'
$repo = 'C:\GitHub\deadsync'
$dir = Join-Path $repo 'docs/testing/2026-11-04-rust-lua-production-migration'
$expected = Get-Content -LiteralPath (Join-Path $dir 'song-live-gameplay-reviewed-sources.json') -Raw | ConvertFrom-Json
foreach ($property in $expected.PSObject.Properties) {
    $actual = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $repo $property.Name)).Hash.ToLowerInvariant()
    if ($actual -ne $property.Value) { throw "Source changed: $($property.Name)" }
}
$objective = 'C:\Users\PerfectTaste\.codex\attachments\bcf567e6-d9b3-4fcf-b693-47949c76632c\pasted-text-1.txt'
$objectiveHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $objective).Hash.ToLowerInvariant()
if ($objectiveHash -ne '86cbb91ab372fad5ac5326b3c3b04fb3db8cab9d4f3a4cc8e71cb64dba48ae34') { throw 'Objective changed' }
$review = Get-Content -LiteralPath (Join-Path $dir 'song-live-gameplay-review.json') -Raw | ConvertFrom-Json
if (-not ($review.formatting_passed -and $review.engine_unchanged -and $review.native_algorithms_unchanged -and $review.existing_tests_only_guard_lifetime_adjustments -and $review.clippy_diagnostics_match_prior_baseline -and $review.without_test_support_compiles)) { throw 'Review incomplete' }
$documents = [ordered]@{}
foreach ($name in @('docs/lua-architecture.md','docs/testing/2026-11-04-rust-lua-production-migration/README.md','docs/testing/2026-11-04-rust-lua-production-migration/review_live_gameplay.py','docs/testing/2026-11-04-rust-lua-production-migration/song-live-gameplay-review.json','docs/testing/2026-11-04-rust-lua-production-migration/document_live_gameplay.py','docs/testing/2026-11-04-rust-lua-production-migration/audit_live_gameplay.ps1')) {
    $documents[$name] = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $repo $name)).Hash.ToLowerInvariant()
}
$report = [ordered]@{
    goal_status='active';goal_complete=$false;production_cutover=$false
    source_count=@($expected.PSObject.Properties).Count
    tests_passed=$review.tests_passed;test_targets=$review.test_targets
    library_tests=$review.library_tests;gameplay_cases=$review.gameplay_cases
    existing_tests_only_guard_lifetime_adjustments=$true;native_algorithms_unchanged=$true;engine_unchanged=$true
    formatting_passed=$true;clippy_diagnostics_match_prior_baseline=$true;without_test_support_compiles=$true
    objective_sha256=$objectiveHash
    previous_goal_turn=$review.previous_goal_turn;current_goal_turn=$review.current_goal_turn
    documents=$documents
}
$json = $report | ConvertTo-Json -Depth 8
[IO.File]::WriteAllText((Join-Path $dir 'song-live-gameplay-final-state.json'),$json+"`n")
Write-Output $json
