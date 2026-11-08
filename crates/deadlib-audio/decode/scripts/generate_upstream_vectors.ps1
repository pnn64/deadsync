param([string]$ReferenceRoot = '', [string]$OutputDirectory = '')
$ErrorActionPreference = 'Stop'
function Invoke-Checked([string]$Program, [string[]]$Arguments) {
    $null = Get-Command -Name $Program -ErrorAction Stop
    $previousPreference = $ErrorActionPreference
    try {
        # Windows PowerShell wraps successful native stderr in ErrorRecords.
        $ErrorActionPreference = 'Continue'
        & $Program @Arguments
        $nativeExit = $LASTEXITCODE
    } finally { $ErrorActionPreference = $previousPreference }
    if ($nativeExit -ne 0) { throw "Native command failed ($nativeExit): $Program" }
}
$decodeRoot = Split-Path $PSScriptRoot -Parent
$repositoryRoot = (Resolve-Path (Join-Path $decodeRoot '../../..')).Path
if (-not $ReferenceRoot) { $ReferenceRoot = Join-Path $repositoryRoot 'target/deadlib-audio-reference' }
$ReferenceRoot = (Resolve-Path -LiteralPath $ReferenceRoot).Path
$pins = @{
    ogg = 'be05b13e98b048f0b5a0f5fa8ce514d56db5f822'
    vorbis = '0657aee69dec8508a0011f47f3b69d7538e9d262'
}
foreach ($package in @('ogg', 'vorbis')) {
    $source = Join-Path $ReferenceRoot $package
    $actual = & git -C $source rev-parse HEAD
    if ($LASTEXITCODE -ne 0 -or $actual -ne $pins[$package]) { throw "Wrong reference commit: $package" }
    $changes = & git -C $source diff HEAD --name-only
    if ($LASTEXITCODE -ne 0 -or $changes) { throw "Modified reference sources: $package" }
}
if (-not $OutputDirectory) {
    $OutputDirectory = Join-Path $ReferenceRoot ('corpus/upstream-' + [guid]::NewGuid().ToString('N'))
}
if (Test-Path -LiteralPath $OutputDirectory) { throw "OutputDirectory must be new: $OutputDirectory" }
New-Item -ItemType Directory -Path $OutputDirectory | Out-Null
$output = (Resolve-Path -LiteralPath $OutputDirectory).Path
$build = Join-Path $ReferenceRoot 'vorbis-oracle-build'
Invoke-Checked -Program cmake -Arguments @('-S', (Join-Path $decodeRoot 'vorbis/tests/oracle'), '-B', $build, "-DREFERENCE_ROOT=$ReferenceRoot")
Invoke-Checked -Program cmake -Arguments @('--build', $build, '--config', 'Release', '--target', 'deadlib_vorbis_upstream_vectors', '--parallel', '4')
$executable = Join-Path $build 'Release/deadlib_vorbis_upstream_vectors.exe'
Push-Location $output
try {
    Invoke-Checked -Program $executable -Arguments @() *> generation.log
} finally { Pop-Location }
$vectors = @(Get-ChildItem -LiteralPath $output -File -Filter '*.ogg' | Sort-Object Name)
if ($vectors.Count -ne 528) { throw "Incomplete upstream grid: $($vectors.Count) files" }
$hashes = foreach ($file in $vectors) {
    (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant() + '  ' + $file.Name
}
$hashes | Set-Content -LiteralPath (Join-Path $output 'assets.sha256') -Encoding ascii
@{ upstream = $pins; platform = [System.Environment]::OSVersion.VersionString; powershell = $PSVersionTable.PSVersion.ToString(); files = 528 } |
    ConvertTo-Json | Set-Content -LiteralPath (Join-Path $output 'provenance.json') -Encoding utf8
Write-Output "DEADLIB_VORBIS_UPSTREAM_CORPUS=$output"
