Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$isAdmin = ([Security.Principal.WindowsPrincipal] [Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole(
    [Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $isAdmin) {
    Write-Error 'This script must be run as Administrator.'
    exit 1
}

$installerUrl = 'https://sdk.lunarg.com/sdk/download/latest/windows/vulkan-sdk.exe'
$installer = Join-Path $env:TEMP 'vulkan-sdk.exe'

Write-Host 'Downloading Vulkan SDK...'
Invoke-WebRequest -Uri $installerUrl -OutFile $installer -UseBasicParsing

Write-Host 'Installing Vulkan SDK...'
$installerArgs = '--accept-licenses', '--default-answer', '--confirm-command', 'install'
Start-Process -FilePath $installer -ArgumentList $installerArgs -NoNewWindow -Wait

$vulkanRoot = Join-Path $env:SystemDrive 'VulkanSDK'

if (-not (Test-Path $vulkanRoot)) {
    Write-Error "::error::Vulkan SDK installation failed — $vulkanRoot does not exist."
    exit 1
}

$sdkDir = Get-ChildItem $vulkanRoot -Directory | Sort-Object Name -Descending | Select-Object -First 1
if (-not $sdkDir) {
    Write-Host "Contents of ${vulkanRoot}:"
    Get-ChildItem $vulkanRoot -Force | Format-Table Name, Mode, LastWriteTime
    Write-Error "::error::Vulkan SDK installation failed — no version directory found under $vulkanRoot."
    exit 1
}
$sdkPath = $sdkDir.FullName

Write-Host "Vulkan SDK installed at: $sdkPath"

if ($env:GITHUB_ENV) {
    "VULKAN_SDK=$sdkPath" | Out-File -FilePath $env:GITHUB_ENV -Append -Encoding utf8
}
if ($env:GITHUB_PATH) {
    "$sdkPath\Bin" | Out-File -FilePath $env:GITHUB_PATH -Append -Encoding utf8
}
