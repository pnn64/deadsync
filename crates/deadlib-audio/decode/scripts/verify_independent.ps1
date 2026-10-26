param([string]$ScratchRoot = '')
$ErrorActionPreference = 'Stop'
$decodeRoot = Split-Path $PSScriptRoot -Parent
$repositoryRoot = (Resolve-Path (Join-Path $decodeRoot '../../..')).Path
if (-not $ScratchRoot) {
    $ScratchRoot = Join-Path $repositoryRoot ('target/deadlib-audio-independent/' + [guid]::NewGuid().ToString('N'))
}
if (Test-Path -LiteralPath $ScratchRoot) { throw "ScratchRoot must be a new directory: $ScratchRoot" }
New-Item -ItemType Directory -Path $ScratchRoot | Out-Null
$scratch = (Resolve-Path -LiteralPath $ScratchRoot).Path
foreach ($package in @('ogg', 'vorbis')) {
    Copy-Item -LiteralPath (Join-Path $decodeRoot $package) -Destination (Join-Path $scratch $package) -Recurse
}
@'
[workspace]
members = ["ogg", "vorbis"]
resolver = "3"
'@ | Set-Content -LiteralPath (Join-Path $scratch 'Cargo.toml') -Encoding utf8
$log = Join-Path $scratch 'validation.log'
function Invoke-Cargo([string[]]$Arguments) {
    $previousPreference = $ErrorActionPreference
    try {
        # Windows PowerShell wraps native stderr in ErrorRecord objects even
        # for successful Cargo progress output. Judge the actual exit code.
        $ErrorActionPreference = 'Continue'
        & cargo @Arguments 2>&1 | ForEach-Object { $_.ToString() } | Tee-Object -FilePath $log -Append
        $cargoExit = $LASTEXITCODE
    } finally { $ErrorActionPreference = $previousPreference }
    if ($cargoExit -ne 0) { throw "Independent cargo command failed ($cargoExit): $Arguments" }
}
Push-Location $scratch
try {
    Invoke-Cargo -Arguments @('test', '--offline', '--workspace')
    foreach ($package in @('deadlib-audio-decode-ogg', 'deadlib-audio-decode-vorbis')) {
        Invoke-Cargo -Arguments @('tree', '--offline', '-p', $package, '--edges', 'normal')
    }
} finally { Pop-Location }
Write-Output "Independent workspace and evidence retained at $scratch"
