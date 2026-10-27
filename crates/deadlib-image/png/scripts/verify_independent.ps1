param([string]$LogPath, [string]$Test)
$ErrorActionPreference = 'Stop'
$pngSource = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
$sharedSource = Split-Path -Parent $pngSource
$isolatedRoot = Join-Path ([System.IO.Path]::GetTempPath()) ('deadlib-image-independent-' + [guid]::NewGuid().ToString('N'))
if (-not $LogPath) { $LogPath = Join-Path $pngSource 'independent-build.log' }
New-Item -ItemType Directory -Path $isolatedRoot | Out-Null
New-Item -ItemType Directory -Path (Join-Path $isolatedRoot 'png') | Out-Null
Copy-Item -LiteralPath (Join-Path $sharedSource 'src') -Destination $isolatedRoot -Recurse
$sharedManifest = Get-Content -LiteralPath (Join-Path $sharedSource 'Cargo.toml') -Raw -Encoding UTF8
$sharedManifest += "`n[workspace]`nmembers = [`".`", `"png`"]`nresolver = `"3`"`n"
Set-Content -LiteralPath (Join-Path $isolatedRoot 'Cargo.toml') -Value $sharedManifest -Encoding UTF8
foreach ($entry in @('Cargo.toml', 'src', 'tests', 'benches', 'examples')) {
    Copy-Item -LiteralPath (Join-Path $pngSource $entry) -Destination (Join-Path $isolatedRoot 'png') -Recurse
}
$testArguments = @('test', '-p', 'deadlib-image', '-p', 'deadlib-image-png', '--offline')
if ($Test) { $testArguments = @('test', '-p', 'deadlib-image-png', '--offline', '--test', $Test) }
$evidence = @(('Isolated workspace: ' + $isolatedRoot), ('Command: cargo ' + ($testArguments -join ' ')))
Push-Location -LiteralPath $isolatedRoot
try {
    $evidence += (& rustc --version)
    $evidence += (& cargo --version)
    # PowerShell 5 can treat native stderr progress as terminating records when
    # ErrorActionPreference is Stop. Preserve the actual native exit code.
    $ErrorActionPreference = 'Continue'
    $evidence += (& cargo @testArguments 2>&1 | ForEach-Object { $_.ToString() })
    $testExit = $LASTEXITCODE
    $evidence += (& cargo tree -p deadlib-image-png --edges normal --offline 2>&1 | ForEach-Object { $_.ToString() })
    $treeExit = $LASTEXITCODE
} finally { Pop-Location }
Set-Content -LiteralPath $LogPath -Value ((($evidence -join "`n") + "`n").Replace("`r`n", "`n")) -Encoding UTF8 -NoNewline
$evidence
if ($testExit -ne 0 -or $treeExit -ne 0) { exit 1 }
Write-Output 'Independent build passed. The temporary source copy is retained for inspection.'
