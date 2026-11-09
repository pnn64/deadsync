$ErrorActionPreference='Stop'
$repo='C:\GitHub\deadsync'
$dir=Join-Path $repo 'docs/testing/2026-11-04-rust-lua-production-migration'
$destination=Join-Path $dir 'song-live-visuals-final-state.json'
if(Test-Path -LiteralPath $destination){throw 'Final audit already exists'}
$expected=Get-Content -LiteralPath (Join-Path $dir 'song-live-visuals-reviewed-sources.json') -Raw | ConvertFrom-Json
foreach($property in $expected.PSObject.Properties){
    $actual=(Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $repo $property.Name)).Hash.ToLowerInvariant()
    if($actual -ne $property.Value){throw "Source changed: $($property.Name)"}
}
$objective='C:\Users\PerfectTaste\.codex\attachments\bcf567e6-d9b3-4fcf-b693-47949c76632c\pasted-text-1.txt'
$objectiveHash=(Get-FileHash -Algorithm SHA256 -LiteralPath $objective).Hash.ToLowerInvariant()
if($objectiveHash -ne '86cbb91ab372fad5ac5326b3c3b04fb3db8cab9d4f3a4cc8e71cb64dba48ae34'){throw 'Objective changed'}
$review=Get-Content -LiteralPath (Join-Path $dir 'song-live-visuals-review.json') -Raw | ConvertFrom-Json
if(-not($review.formatting_passed -and $review.engine_unchanged -and $review.native_algorithms_byte_unchanged -and $review.existing_tests_byte_unchanged -and $review.reader_refactors_verified -and $review.clippy_diagnostics_match_prior_baseline -and $review.without_test_support_compiles -and $review.reentry_guard_verified)){throw 'Incomplete source review'}
$documents=[ordered]@{}
foreach($name in @('docs/lua-architecture.md','docs/testing/2026-11-04-rust-lua-production-migration/README.md','docs/testing/2026-11-04-rust-lua-production-migration/review_song_live_visuals.py','docs/testing/2026-11-04-rust-lua-production-migration/song-live-visuals-review.json','docs/testing/2026-11-04-rust-lua-production-migration/document_song_live_visuals.py','docs/testing/2026-11-04-rust-lua-production-migration/audit_song_live_visuals.ps1')){
    $documents[$name]=(Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $repo $name)).Hash.ToLowerInvariant()
}
$report=[ordered]@{
    goal_status='active';goal_complete=$false;production_cutover=$false;final_application_validation=$false
    source_count=@($expected.PSObject.Properties).Count
    tests_passed=$review.tests_passed;test_targets=$review.test_targets;library_tests=$review.library_tests
    visual_cases=$review.visual_cases;focused_tests=$review.focused_tests
    engine_unchanged=$true;native_algorithms_byte_unchanged=$true;existing_tests_byte_unchanged=$true
    reader_refactors_verified=$true;formatting_passed=$true
    clippy_diagnostics_match_prior_baseline=$true;without_test_support_compiles=$true
    reentry_guard_verified=$review.reentry_guard_verified;pre_reentry_regression_count=$review.pre_reentry_regression_count
    objective_sha256=$objectiveHash;previous_goal_turn=$review.previous_goal_turn;current_goal_turn=$review.current_goal_turn
    documents=$documents
}
$json=$report | ConvertTo-Json -Depth 8
[IO.File]::WriteAllText($destination,$json+"`n")
Write-Output $json
