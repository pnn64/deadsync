Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Get-VulkanSdkInstaller {
    param(
        [Parameter(Mandatory)]
        [string]$Uri,

        [Parameter(Mandatory)]
        [string]$OutFile
    )

    # Retry transport failures too (for example a connection reset mid-download).
    # With --output, curl truncates the partial download before each retry.
    & curl.exe --fail --location --no-progress-meter `
        --retry 5 --retry-all-errors --retry-delay 5 --retry-max-time 600 `
        --connect-timeout 30 --max-time 600 --output $OutFile $Uri
    $downloadExitCode = $LASTEXITCODE
    if ($downloadExitCode -ne 0) {
        Remove-Item -LiteralPath $OutFile -Force -ErrorAction SilentlyContinue
        throw "Vulkan SDK download failed after retries (curl exit code $downloadExitCode): $Uri"
    }
    if (-not (Test-Path -LiteralPath $OutFile -PathType Leaf) -or
        (Get-Item -LiteralPath $OutFile).Length -eq 0) {
        throw "Vulkan SDK download produced no installer: $Uri"
    }
}

$isAdmin = ([Security.Principal.WindowsPrincipal] [Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole(
    [Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $isAdmin) {
    Write-Error 'This script must be run as Administrator.'
    exit 1
}

$installerUrl = 'https://sdk.lunarg.com/sdk/download/latest/windows/vulkan-sdk.exe'
$installer = Join-Path $env:TEMP 'vulkan-sdk.exe'

Write-Host 'Downloading Vulkan SDK...'
Get-VulkanSdkInstaller -Uri $installerUrl -OutFile $installer

Write-Host 'Installing Vulkan SDK...'
$installerArgs = '--accept-licenses', '--default-answer', '--confirm-command', 'install'
$installProcess = Start-Process -FilePath $installer -ArgumentList $installerArgs -WindowStyle Hidden -Wait -PassThru
if ($installProcess.ExitCode -ne 0) {
    throw "Vulkan SDK installer failed with exit code $($installProcess.ExitCode)."
}

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
