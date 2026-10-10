#Requires -Version 5.1
param(
    [Parameter(Mandatory)]
    [string]$Path,

    [string]$Dumpbin
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
    throw "Missing executable: $Path"
}

if (-not $Dumpbin) {
    $command = Get-Command dumpbin.exe -ErrorAction SilentlyContinue
    if ($command) {
        $Dumpbin = $command.Source
    } else {
        $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
        if (-not (Test-Path -LiteralPath $vswhere)) {
            throw 'Cannot find dumpbin.exe. Run from a Visual Studio developer shell or pass -Dumpbin.'
        }
        $candidates = @(& $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -find 'VC\Tools\MSVC\**\bin\Hostx64\x64\dumpbin.exe')
        if ($LASTEXITCODE -ne 0 -or $candidates.Count -eq 0) {
            throw 'Cannot find dumpbin.exe in the Visual Studio C++ toolchain.'
        }
        $Dumpbin = $candidates[-1]
    }
}

$output = @(& $Dumpbin /nologo /imports $Path 2>&1)
if ($LASTEXITCODE -ne 0) {
    throw "dumpbin failed for ${Path}:`n$($output -join [Environment]::NewLine)"
}

# Only eager imports are loaded before main. Delay-loaded libraries may be
# optional on Windows 7, so do not treat those as startup dependencies.
$imports = @(
    foreach ($line in $output) {
        if ($line -match 'Section contains the following delay load imports') {
            break
        }
        if ($line -match '^\s+(\S+\.dll)\s*$') {
            $Matches[1].ToLowerInvariant()
        }
    }
)
if ($imports.Count -eq 0) {
    throw "No DLL imports found in $Path; cannot verify Windows 7 startup dependencies."
}

# Catch the WinRT loader regressions from #770. This is not an exhaustive
# Windows 7 API audit; a successful build on a modern host is not a runtime test.
$unsupported = @($imports | Where-Object {
    $_ -eq 'combase.dll' -or $_ -like 'api-ms-win-core-winrt-*.dll'
} | Sort-Object -Unique)
if ($unsupported.Count -ne 0) {
    throw "Windows 7 cannot load ${Path}: unsupported DLL imports: $($unsupported -join ', ')"
}

Write-Host "Windows 7 WinRT import check passed: $Path ($($imports.Count) DLL imports)"
