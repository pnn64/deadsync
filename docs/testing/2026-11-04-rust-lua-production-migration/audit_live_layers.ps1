$ErrorActionPreference='Stop'
$root=Split-Path -Parent (Split-Path -Parent (Split-Path -Parent $PSScriptRoot))
$review=Get-Content -LiteralPath (Join-Path $PSScriptRoot 'song-live-layers-review.json') -Raw | ConvertFrom-Json
$sources=Get-Content -LiteralPath (Join-Path $PSScriptRoot 'song-live-layers-reviewed-sources.json') -Raw | ConvertFrom-Json
$verified=[ordered]@{}
foreach($item in $sources.PSObject.Properties){
 $path=Join-Path $root $item.Name
 $hash=(Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant()
 if($hash -ne $item.Value){throw "Source changed: $($item.Name)"}
 $verified[$item.Name]=$hash
}
$objective='C:\Users\PerfectTaste\.codex\attachments\bcf567e6-d9b3-4fcf-b693-47949c76632c\pasted-text-1.txt'
$hash=(Get-FileHash -LiteralPath $objective -Algorithm SHA256).Hash.ToLowerInvariant()
if($hash -ne '86cbb91ab372fad5ac5326b3c3b04fb3db8cab9d4f3a4cc8e71cb64dba48ae34'){throw 'Objective changed'}
if($review.goal_complete -or $review.production_cutover -or $review.final_application_validation){throw 'Migration completion is not established'}
$documents=[ordered]@{}
foreach($relative in @('docs/lua-architecture.md','docs/testing/2026-11-04-rust-lua-production-migration/README.md','docs/testing/2026-11-04-rust-lua-production-migration/review_live_layers.py','docs/testing/2026-11-04-rust-lua-production-migration/song-live-layers-review.json','docs/testing/2026-11-04-rust-lua-production-migration/document_live_layers.py','docs/testing/2026-11-04-rust-lua-production-migration/audit_live_layers.ps1')){
 $documents[$relative]=(Get-FileHash -LiteralPath (Join-Path $root $relative) -Algorithm SHA256).Hash.ToLowerInvariant()
}
$result=[ordered]@{goal_status='active';goal_complete=$false;production_cutover=$false;source_count=$verified.Count;tests_passed=$review.tests_passed;test_targets=$review.test_targets;live_layer_cases=$review.live_layer_cases;existing_tests_byte_unchanged=$review.existing_tests_byte_unchanged;native_algorithms_byte_unchanged=$review.native_algorithms_byte_unchanged;engine_unchanged=$review.engine_unchanged;formatting_passed=$review.formatting_passed;clippy_log_matches_prior_baseline=$review.clippy_log_matches_prior_baseline;without_test_support_compiles=$review.without_test_support_compiles;objective_sha256=$hash;previous_goal_turn='progress: live actor traversal and player feedback; 204 tests';current_goal_turn='progress: Rust live background and foreground scheduling';documents=$documents}
$verified|ConvertTo-Json -Depth 8|Set-Content -LiteralPath (Join-Path $PSScriptRoot 'song-live-layers-final-sources.json') -Encoding UTF8
$result|ConvertTo-Json -Depth 8|Set-Content -LiteralPath (Join-Path $PSScriptRoot 'song-live-layers-final-state.json') -Encoding UTF8
$result|ConvertTo-Json -Depth 8
