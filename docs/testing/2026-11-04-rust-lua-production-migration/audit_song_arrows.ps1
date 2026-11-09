$ErrorActionPreference='Stop'
$repo='C:\GitHub\deadsync'
$dir=Join-Path $repo 'docs/testing/2026-11-04-rust-lua-production-migration'
$expected=Get-Content -LiteralPath (Join-Path $dir 'song-arrows-reviewed-sources.json') -Raw | ConvertFrom-Json
foreach($property in $expected.PSObject.Properties){
    $actual=(Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $repo $property.Name)).Hash.ToLowerInvariant()
    if($actual -ne $property.Value){throw "Source changed: $($property.Name)"}
}
$refs=Get-Content -LiteralPath (Join-Path $dir 'song-arrows-reference-inputs.json') -Raw | ConvertFrom-Json
foreach($property in $refs.PSObject.Properties){
    $actual=(Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $repo $property.Name)).Hash.ToLowerInvariant()
    if($actual -ne $property.Value){throw "Reference changed: $($property.Name)"}
}
$objective='C:\Users\PerfectTaste\.codex\attachments\bcf567e6-d9b3-4fcf-b693-47949c76632c\pasted-text-1.txt'
$objectiveHash=(Get-FileHash -Algorithm SHA256 -LiteralPath $objective).Hash.ToLowerInvariant()
if($objectiveHash -ne '86cbb91ab372fad5ac5326b3c3b04fb3db8cab9d4f3a4cc8e71cb64dba48ae34'){throw 'Objective changed'}
$review=Get-Content -LiteralPath (Join-Path $dir 'song-arrows-review.json') -Raw | ConvertFrom-Json
if(-not($review.formatting_passed -and $review.engine_unchanged -and $review.native_algorithms_byte_unchanged -and $review.existing_tests_byte_unchanged -and $review.native_math_expression_transfer_verified -and $review.clippy_diagnostics_match_prior_baseline -and $review.without_test_support_compiles -and $review.ownership_regression_corrected)){throw 'Incomplete source review'}
$documents=[ordered]@{}
foreach($name in @('docs/lua-architecture.md','docs/testing/2026-11-04-rust-lua-production-migration/README.md','docs/testing/2026-11-04-rust-lua-production-migration/review_song_arrows.py','docs/testing/2026-11-04-rust-lua-production-migration/song-arrows-review.json','docs/testing/2026-11-04-rust-lua-production-migration/document_song_arrows.py','docs/testing/2026-11-04-rust-lua-production-migration/audit_song_arrows.ps1')){
    $documents[$name]=(Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $repo $name)).Hash.ToLowerInvariant()
}
$report=[ordered]@{
    goal_status='active';goal_complete=$false;production_cutover=$false;final_application_validation=$false
    source_count=@($expected.PSObject.Properties).Count
    tests_passed=$review.tests_passed;test_targets=$review.test_targets;library_tests=$review.library_tests
    arrow_cases=$review.arrow_cases;captured_reference_cases=$review.captured_reference_cases
    engine_unchanged=$true;native_algorithms_byte_unchanged=$true;existing_tests_byte_unchanged=$true
    native_math_expression_transfer_verified=$true;formatting_passed=$true
    clippy_diagnostics_match_prior_baseline=$true;without_test_support_compiles=$true
    ownership_regression_corrected=$review.ownership_regression_corrected
    objective_sha256=$objectiveHash;previous_goal_turn=$review.previous_goal_turn;current_goal_turn=$review.current_goal_turn
    documents=$documents
}
$json=$report | ConvertTo-Json -Depth 8
[IO.File]::WriteAllText((Join-Path $dir 'song-arrows-final-state.json'),$json+"`n")
Write-Output $json
