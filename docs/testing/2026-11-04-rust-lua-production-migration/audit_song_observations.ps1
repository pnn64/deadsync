$ErrorActionPreference = 'Stop'
$migrationRoot = 'C:\GitHub\deadsync'
$evidenceRoot = Join-Path $migrationRoot 'docs\testing\2026-11-04-rust-lua-production-migration'
$review = Get-Content -LiteralPath (Join-Path $evidenceRoot 'song-observations-review.json') -Raw | ConvertFrom-Json
$reviewed = Get-Content -LiteralPath (Join-Path $evidenceRoot 'song-observations-reviewed-sources.json') -Raw | ConvertFrom-Json
$sourceCrates = @('deadlib-lua', 'deadsync-song-lua', 'deadsync-noteskin', 'deadsync-theme-simply-love', 'deadsync-assets', 'deadsync-shell', 'deadsync-notefield') | ForEach-Object { "crates/$_" }
$sourceNames = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
$gitNames = & git -C $migrationRoot -c core.quotePath=false ls-files --cached --others --exclude-standard -- $sourceCrates
if ($LASTEXITCODE -ne 0) { throw 'Git source enumeration failed' }
foreach ($name in $gitNames) {
    if ([System.IO.Path]::GetExtension($name) -in @('.rs', '.lua', '.toml') -and (Test-Path -LiteralPath (Join-Path $migrationRoot $name) -PathType Leaf)) {
        [void]$sourceNames.Add($name)
    }
}
foreach ($name in @('Cargo.toml', 'Cargo.lock', '.cargo/config.toml')) {
    if (Test-Path -LiteralPath (Join-Path $migrationRoot $name) -PathType Leaf) { [void]$sourceNames.Add($name) }
}
if ($sourceNames.Count -ne @($reviewed.PSObject.Properties).Count) { throw 'Source inventory changed' }
$checked = [ordered]@{}
foreach ($entry in $reviewed.PSObject.Properties) {
    if (-not $sourceNames.Contains($entry.Name)) { throw "Source missing from inventory: $($entry.Name)" }
    $absolute = Join-Path $migrationRoot $entry.Name
    $digest = (Get-FileHash -LiteralPath $absolute -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($digest -ne $entry.Value) { throw "Source changed: $($entry.Name)" }
    $checked[$entry.Name] = $digest
}
$objectivePath = 'C:\Users\PerfectTaste\.codex\attachments\bcf567e6-d9b3-4fcf-b693-47949c76632c\pasted-text-1.txt'
$objectiveSha = (Get-FileHash -LiteralPath $objectivePath -Algorithm SHA256).Hash.ToLowerInvariant()
if ($objectiveSha -ne '86cbb91ab372fad5ac5326b3c3b04fb3db8cab9d4f3a4cc8e71cb64dba48ae34') { throw 'Objective changed' }
$hashes = [ordered]@{}
foreach ($relative in @('docs/lua-architecture.md', 'docs/testing/2026-11-04-rust-lua-production-migration/README.md', 'docs/testing/2026-11-04-rust-lua-production-migration/song-observations-review.json', 'docs/testing/2026-11-04-rust-lua-production-migration/review_song_observations.py', 'docs/testing/2026-11-04-rust-lua-production-migration/audit_song_observations.ps1')) {
    $hashes[$relative] = (Get-FileHash -LiteralPath (Join-Path $migrationRoot $relative) -Algorithm SHA256).Hash.ToLowerInvariant()
}
$state = [ordered]@{
    objective_sha256 = $objectiveSha
    goal_complete = $false
    production_cutover = $false
    final_application_validation = $false
    previous_visible_turn = 'no progress: scope acknowledgement only; objective revalidated before edits'
    current_turn = 'progress: compiler observation fields wired and checked'
    source_count = $checked.Count
    existing_tests_byte_unchanged = $review.existing_tests_byte_unchanged
    native_algorithms_byte_unchanged = $review.native_algorithms_byte_unchanged
    engine_unchanged = $review.engine_unchanged
    tests_passed = $review.tests_passed
    observation_cases = $review.observation_cases
    without_test_support_compiles = $review.without_test_support_compiles
    clippy_diagnostics_match_prior_baseline = $review.clippy_diagnostics_match_prior_baseline
    remaining = @('production live/session/fallback/cache wiring', 'unconditional production cutover and legacy graph removal', 'final downstream/content/application/render/performance validation')
    hashes = $hashes
}
$checked | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $evidenceRoot 'song-observations-final-sources.json') -Encoding UTF8
$state | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $evidenceRoot 'song-observations-final-state.json') -Encoding UTF8
$state | ConvertTo-Json -Depth 8
