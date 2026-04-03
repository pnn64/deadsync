Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

# ---------- self-elevate if needed ----------
$isAdmin = ([Security.Principal.WindowsPrincipal] [Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole(
    [Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $isAdmin) {
    Write-Host "Relaunching as Administrator..."
    Start-Process pwsh -Verb RunAs -Wait -ArgumentList "-ExecutionPolicy Bypass -File `"$PSCommandPath`""
    exit $LASTEXITCODE
}

# ---------- locate VS Installer ----------
$progX86 = [Environment]::GetFolderPath('ProgramFilesX86')
$vsWhere = "$progX86\Microsoft Visual Studio\Installer\vswhere.exe"
if (-not (Test-Path $vsWhere)) {
    Write-Error "vswhere.exe not found - is Visual Studio installed?"
    exit 1
}

$vsInfo = & $vsWhere -latest -products * -format json | ConvertFrom-Json
if (-not $vsInfo) {
    Write-Error "No Visual Studio installation found."
    exit 1
}

$installPath = $vsInfo[0].installationPath
$vsVersion   = $vsInfo[0].installationVersion
Write-Host "Found Visual Studio at: $installPath"
Write-Host "Version: $vsVersion"

# ---------- show current toolsets ----------
$msvcRoot = Join-Path $installPath "VC\Tools\MSVC"
Write-Host "`nCurrently installed MSVC toolsets:"
Get-ChildItem $msvcRoot -Directory | ForEach-Object { Write-Host "  $($_.Name)" }

# ---------- step 1: update VS to latest ----------
# The 14.42 toolset component requires VS 17.12+. If VS is older, we must
# update first. We always run update; it's a no-op if already current.
$vsInstaller = "$progX86\Microsoft Visual Studio\Installer\vs_installer.exe"
if (-not (Test-Path $vsInstaller)) {
    Write-Error "vs_installer.exe not found at expected path."
    exit 1
}

# Close any running VS processes that would block the update.
Write-Host "`nClosing Visual Studio processes..."
$vsProcs = @('devenv', 'PerfWatson2', 'ServiceHub.Host.CLR', 'ServiceHub.Host.dotnet',
             'ServiceHub.IdentityHost', 'ServiceHub.IndexingService', 'ServiceHub.ThreadedWaitDialog',
             'Microsoft.ServiceHub.Controller', 'vstest.console')
foreach ($name in $vsProcs) {
    Get-Process -Name $name -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
}

Write-Host "`nStep 1: Updating Visual Studio to latest version (required for newer toolsets)..."
Write-Host "This may take a long time. Please be patient.`n"

$updateArgs = @(
    "update"
    "--installPath", "`"$installPath`""
    "--passive"
    "--force"
)

Start-Process -FilePath $vsInstaller -ArgumentList $updateArgs -Wait

# Re-query version after update
$vsInfo = & $vsWhere -latest -products * -format json | ConvertFrom-Json
$vsVersion = $vsInfo[0].installationVersion
Write-Host "`nVisual Studio updated to: $vsVersion"

# ---------- step 2: add the 14.42 toolset component ----------
$componentId = "Microsoft.VisualStudio.Component.VC.14.42.17.12.x86.x64"
Write-Host "`nStep 2: Installing toolset component $componentId..."

$modifyArgs = @(
    "modify"
    "--installPath", "`"$installPath`""
    "--add", $componentId
    "--passive"
    "--force"
)

Start-Process -FilePath $vsInstaller -ArgumentList $modifyArgs -Wait

# ---------- verify ----------
Write-Host "`nInstalled MSVC toolsets after update:"
Get-ChildItem $msvcRoot -Directory | ForEach-Object { Write-Host "  $($_.Name)" }

Write-Host "`nDone. You can now run 'cargo clean' and 'cargo build --release'."
Write-Host "If the new toolset is not picked up automatically, you may need to"
Write-Host "open a fresh 'Developer Command Prompt for VS 2022' or 'x64 Native"
Write-Host "Tools Command Prompt' so the updated paths take effect."
