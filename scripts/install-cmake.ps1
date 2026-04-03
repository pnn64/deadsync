Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

if (Get-Command cmake -ErrorAction SilentlyContinue) {
    $existing = (cmake --version | Select-Object -First 1).Trim()
    Write-Host "CMake already installed: $existing"
    return
}

$isAdmin = ([Security.Principal.WindowsPrincipal] [Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole(
    [Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $isAdmin) {
    Write-Error 'This script must be run as Administrator.'
    exit 1
}

$version = '3.31.6'
$url = "https://github.com/Kitware/CMake/releases/download/v${version}/cmake-${version}-windows-x86_64.zip"
$zip = Join-Path $env:TEMP 'cmake-win.zip'
$dest = Join-Path $env:ProgramFiles 'cmake'

Write-Host "Downloading CMake v${version}..."
Invoke-WebRequest -Uri $url -OutFile $zip -UseBasicParsing

Write-Host "Extracting to $dest..."
if (Test-Path $dest) { Remove-Item $dest -Recurse -Force }
Expand-Archive -Path $zip -DestinationPath $dest

# The zip extracts into a versioned subdirectory; find the bin folder.
$binDir = Get-ChildItem $dest -Recurse -Directory -Filter 'bin' | Select-Object -First 1
if (-not $binDir) {
    Write-Error '::error::CMake extraction failed — no bin directory found.'
    exit 1
}

if ($env:GITHUB_PATH) {
    $binDir.FullName | Out-File -FilePath $env:GITHUB_PATH -Append -Encoding utf8
}

Write-Host "CMake v${version} installed at: $($binDir.FullName)"
