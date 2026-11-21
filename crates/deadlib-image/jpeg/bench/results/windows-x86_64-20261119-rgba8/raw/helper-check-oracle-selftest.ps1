$ErrorActionPreference = 'Stop'
$checkRepository = 'C:\GitHub\deadsync'
$checkRoot = Join-Path $checkRepository 'target\jpeg-perf-20261119\helper-check-oracle-20261119'
$checkBinary = Join-Path $checkRepository 'target\jpeg-decode-bench\release\native_snapshot.exe'
$checkFixture = Join-Path $checkRepository 'crates\deadlib-image\jpeg\tests\fixtures\sequential\8-gray.jpg'
if (Test-Path -LiteralPath $checkRoot) { throw "Self-check directory already exists: $checkRoot" }
New-Item -ItemType Directory -Path $checkRoot | Out-Null
$checkInput = Join-Path $checkRoot 'input.jpg'
Copy-Item -LiteralPath $checkFixture -Destination $checkInput
$checkBaseline = Join-Path $checkRoot 'baseline'
$checkResults = New-Object 'System.Collections.Generic.List[object]'

function Invoke-OracleCheck {
    param([string]$Name, [string[]]$NativeArguments, [int]$ExpectedExit)
    $checkOutput = Join-Path $checkRoot ("helper-self-check-" + $Name + ".stdout.txt")
    $checkError = Join-Path $checkRoot ("helper-self-check-" + $Name + ".stderr.txt")
    $checkOldPreference = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    & $checkBinary @NativeArguments > $checkOutput 2> $checkError
    $checkExit = $LASTEXITCODE
    $ErrorActionPreference = $checkOldPreference
    $checkResults.Add([pscustomobject]@{
        name = $Name
        expected_exit = $ExpectedExit
        actual_exit = $checkExit
        passed = ($checkExit -eq $ExpectedExit)
    })
    if ($checkExit -ne $ExpectedExit) {
        throw "Unexpected exit $checkExit in $Name; expected $ExpectedExit. See $checkError"
    }
}

try {
    Invoke-OracleCheck 'capture' @('capture', $checkBaseline, $checkInput) 0
    Invoke-OracleCheck 'verify' @('verify', $checkBaseline, $checkInput) 0
    Invoke-OracleCheck 'recapture-rejected' @('capture', $checkBaseline, $checkInput) 1

    $checkSnapshot = Join-Path $checkBaseline '000000.snap'
    $checkOriginal = [System.IO.File]::ReadAllBytes($checkSnapshot)
    [System.IO.File]::WriteAllBytes((Join-Path $checkRoot 'original.snap'), $checkOriginal)
    $checkChanged = [byte[]]$checkOriginal.Clone()
    $checkMutationOffset = [Math]::Min(256, $checkChanged.Length - 1)
    $checkChanged[$checkMutationOffset] = $checkChanged[$checkMutationOffset] -bxor 1
    [System.IO.File]::WriteAllBytes($checkSnapshot, $checkChanged)
    Invoke-OracleCheck 'corruption-rejected' @('verify', $checkBaseline, $checkInput) 1

    [System.IO.File]::WriteAllBytes($checkSnapshot, $checkOriginal)
    Invoke-OracleCheck 'restored-verifies' @('verify', $checkBaseline, $checkInput) 0

    $checkShort = [byte[]]$checkOriginal[0..($checkOriginal.Length - 2)]
    [System.IO.File]::WriteAllBytes($checkSnapshot, $checkShort)
    Invoke-OracleCheck 'short-snapshot-rejected' @('verify', $checkBaseline, $checkInput) 1

    $checkExtra = New-Object byte[] ($checkOriginal.Length + 1)
    [Array]::Copy($checkOriginal, $checkExtra, $checkOriginal.Length)
    [System.IO.File]::WriteAllBytes($checkSnapshot, $checkExtra)
    Invoke-OracleCheck 'extra-snapshot-rejected' @('verify', $checkBaseline, $checkInput) 1
    [System.IO.File]::WriteAllBytes($checkSnapshot, $checkOriginal)

    $checkOriginalInput = [System.IO.File]::ReadAllBytes($checkInput)
    $checkChangedInput = [byte[]]$checkOriginalInput.Clone()
    $checkChangedInput[$checkChangedInput.Length - 1] = $checkChangedInput[$checkChangedInput.Length - 1] -bxor 1
    [System.IO.File]::WriteAllBytes($checkInput, $checkChangedInput)
    Invoke-OracleCheck 'changed-input-rejected' @('verify', $checkBaseline, $checkInput) 1
    [System.IO.File]::WriteAllBytes($checkInput, $checkOriginalInput)
    Invoke-OracleCheck 'final-restored-verifies' @('verify', $checkBaseline, $checkInput) 0
} finally {
    $checkResults | ConvertTo-Json -Depth 3 | Set-Content -LiteralPath (Join-Path $checkRoot 'helper-self-check-status.json') -Encoding UTF8
}
$checkResults | Format-Table -AutoSize
Write-Output ("SELF_CHECK_COMPLETE " + $checkResults.Count + " cases; original fixture never modified; " + $checkRoot)
