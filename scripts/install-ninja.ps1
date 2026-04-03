Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

if (Get-Command ninja -ErrorAction SilentlyContinue) {
    $existing = (ninja --version 2>&1).Trim()
    Write-Host "Ninja already installed: $existing"
    return
}

$isAdmin = ([Security.Principal.WindowsPrincipal] [Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole(
    [Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $isAdmin) {
    Write-Error 'This script must be run as Administrator.'
    exit 1
}

$version = '1.12.1'
$url = "https://github.com/ninja-build/ninja/releases/download/v${version}/ninja-win.zip"
$zip = Join-Path $env:TEMP 'ninja-win.zip'
$dest = Join-Path $env:ProgramFiles 'ninja'

Write-Host "Downloading Ninja v${version}..."
Invoke-WebRequest -Uri $url -OutFile $zip -UseBasicParsing

Write-Host "Extracting to $dest..."
if (Test-Path $dest) { Remove-Item $dest -Recurse -Force }
Expand-Archive -Path $zip -DestinationPath $dest

if ($env:GITHUB_PATH) {
    $dest | Out-File -FilePath $env:GITHUB_PATH -Append -Encoding utf8
}

Write-Host "Ninja v${version} installed at: $dest"
