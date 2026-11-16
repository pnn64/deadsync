#Requires -Version 5.1
param(
    [Parameter(Mandatory)]
    [string]$Target
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $PSScriptRoot
$manifest = Join-Path $repoRoot 'Cargo.toml'
$metadataJson = & cargo metadata --locked --format-version 1 --manifest-path $manifest --filter-platform $Target
if ($LASTEXITCODE -ne 0) {
    throw 'Cannot verify the vendored rfd dependency: cargo metadata failed.'
}
$metadata = ($metadataJson -join [Environment]::NewLine) | ConvertFrom-Json
$resolvedIds = @($metadata.resolve.nodes | ForEach-Object { $_.id })
$rfd = @($metadata.packages | Where-Object { $_.name -eq 'rfd' -and $_.id -in $resolvedIds })
$expectedManifest = [IO.Path]::GetFullPath((Join-Path $repoRoot 'vendor/rfd-win7/Cargo.toml'))
if ($rfd.Count -ne 1 -or $null -ne $rfd[0].source -or
    [IO.Path]::GetFullPath($rfd[0].manifest_path) -ne $expectedManifest) {
    throw 'Windows 7 requires vendor/rfd-win7. Update the vendored rfd version and Cargo.lock together with deadsync-shell.'
}

Write-Host "Vendored rfd $($rfd[0].version) verified for $Target"
