#Requires -Version 5.1
param(
    [Parameter(Mandatory)]
    [string]$Tag,

    [string]$Arch,

    [string]$Target,

    [ValidateSet('windows', 'win7')]
    [string]$Platform = 'windows'
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Map-Arch([string]$Value) {
    switch ($Value.ToLowerInvariant()) {
        { $_ -in 'x64', 'amd64', 'x86_64' } { return 'x86_64' }
        { $_ -in 'x86', 'i386', 'i686' }     { return 'i686' }
        { $_ -in 'arm64', 'aarch64' }        { return 'arm64' }
        default {
            Write-Error "unknown arch: $Value"
            exit 1
        }
    }
}

if (-not $Arch) {
    $archRaw = if ($env:RUNNER_ARCH) { $env:RUNNER_ARCH } else { $env:PROCESSOR_ARCHITECTURE }
    $Arch = Map-Arch $archRaw
} else {
    $Arch = Map-Arch $Arch
}

$binPath = if ($Target) {
    Join-Path (Join-Path (Join-Path 'target' $Target) 'release') 'deadsync.exe'
} else {
    'target\release\deadsync.exe'
}

if (-not (Test-Path $binPath)) {
    Write-Error "missing executable: $binPath"
    exit 1
}
foreach ($dir in 'assets', 'songs', 'courses') {
    if (-not (Test-Path $dir -PathType Container)) {
        Write-Error "missing directory: $dir"
        exit 1
    }
}

$distDir     = 'dist'
$pkgName     = "deadsync-$Tag-$Arch-$Platform"
$stageDir    = Join-Path $distDir 'DeadSync'
$archivePath = Join-Path $distDir "$pkgName.zip"
$checksumPath = "$archivePath.sha256"

if (Test-Path $stageDir) { Remove-Item $stageDir -Recurse -Force }
New-Item -ItemType Directory -Path $stageDir -Force | Out-Null

Copy-Item $binPath       -Destination $stageDir
Copy-Item 'assets'       -Destination $stageDir -Recurse
Copy-Item 'songs'        -Destination $stageDir -Recurse
Copy-Item 'courses'      -Destination $stageDir -Recurse
Copy-Item 'README.md'    -Destination $stageDir
Copy-Item 'LICENSE'      -Destination $stageDir
New-Item -ItemType File -Path (Join-Path $stageDir 'portable.txt') -Force | Out-Null

if (Test-Path $archivePath) { Remove-Item $archivePath -Force }
if (Test-Path $checksumPath) { Remove-Item $checksumPath -Force }
Compress-Archive -Path $stageDir -DestinationPath $archivePath
$hash = (Get-FileHash -Algorithm SHA256 -Path $archivePath).Hash.ToLowerInvariant()
$archiveName = Split-Path -Leaf $archivePath
"$hash  $archiveName" | Out-File -FilePath $checksumPath -Encoding ascii

if ($env:GITHUB_OUTPUT) {
    "archive=$archivePath"  | Out-File -FilePath $env:GITHUB_OUTPUT -Append -Encoding utf8
    "checksum=$checksumPath" | Out-File -FilePath $env:GITHUB_OUTPUT -Append -Encoding utf8
    "stage=$stageDir"       | Out-File -FilePath $env:GITHUB_OUTPUT -Append -Encoding utf8
}

Write-Host "Packaged: $archivePath"
