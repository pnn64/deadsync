param(
    [Parameter(Mandatory = $true)] [string] $Before,
    [Parameter(Mandatory = $true)] [string] $After,
    [Parameter(Mandatory = $true)] [string] $Inputs,
    [Parameter(Mandatory = $true)] [string] $Work,
    [Parameter(Mandatory = $true)] [string] $Avutil,
    [string] $Trace,
    [ValidateSet('scan', 'block', 'inverse')] [string] $Operation = 'scan',
    [int] $Rounds = 5,
    [int] $Cpu = 12,
    [switch] $Capture,
    [switch] $SkipVerify,
    [switch] $VerifyOnly,
    [switch] $FullCorpus
)

$ErrorActionPreference = 'Stop'
$Before = (Resolve-Path -LiteralPath $Before).Path
$After = (Resolve-Path -LiteralPath $After).Path
$Inputs = (Resolve-Path -LiteralPath $Inputs).Path
$Avutil = (Resolve-Path -LiteralPath $Avutil).Path
$Work = (New-Item -ItemType Directory -Force -Path $Work).FullName
if (!$Trace) { $Trace = $Work }
$Trace = (New-Item -ItemType Directory -Force -Path $Trace).FullName
$env:Path = "$Avutil;$env:Path"
$bench = Join-Path $PSScriptRoot 'itg_pass.rs'
$hooks = Join-Path $PSScriptRoot 'itg_kernel_hooks.rs'
$flags = @('--edition', '2021', '-C', 'opt-level=3', '-C', 'codegen-units=1')

function Check([string] $what) {
    if ($LASTEXITCODE -ne 0) { throw "$what failed: $LASTEXITCODE" }
}

function Build-Library([string] $source, [string] $name, [string] $variant) {
    $dir = Join-Path $Work "$name-$variant"
    New-Item -ItemType Directory -Force -Path $dir | Out-Null
    Copy-Item "$source/*.rs" $dir -Force
    $extra = @()
    if ($variant -ne 'plain') {
        $frame = Join-Path $dir 'frame.rs'
        $lib = Join-Path $dir 'lib.rs'
        Add-Content -Encoding UTF8 $frame ("`n" + (Get-Content -Raw -LiteralPath $hooks))
        Add-Content -Encoding UTF8 $lib "`npub use frame::{PerfPlane, perf_inverse_batch};"
        if ($variant -eq 'capture') {
            $extra += @('--cfg', 'itg_capture')
            Add-Content -Encoding UTF8 $lib "`npub use frame::write_trace;"
            $text = Get-Content -Raw -LiteralPath $frame
            $needle = 'pub(crate) fn block(&mut self, x: usize, y: usize, block: &[i16; 64], intra: bool) {'
            if (!$text.Contains($needle)) { throw 'Plane::block signature changed' }
            $text = $text.Replace($needle, $needle + @'

        if sample_plane() {
            let mut prediction = [0; 64];
            for (row, dst) in prediction.as_chunks_mut::<8>().0.iter_mut().enumerate() {
                let start = (y + row) * self.stride + x;
                dst.copy_from_slice(&self.samples[start..start + 8]);
            }
            record_plane(intra, *block, prediction);
        }
'@)
            Set-Content -Encoding UTF8 -LiteralPath $frame $text
            $transform = Join-Path $dir 'transform.rs'
            $text = Get-Content -Raw -LiteralPath $transform
            $needle = 'pub(crate) fn inverse(coefficients: &[i16; 64]) -> [i16; 64] {'
            if (!$text.Contains($needle)) { throw 'inverse signature changed' }
            $text = $text.Replace($needle, "$needle`n    crate::frame::record_transform(coefficients);")
            Set-Content -Encoding UTF8 -LiteralPath $transform $text
        }
    }
    $rlib = Join-Path $Work "lib$name-$variant.rlib"
    rustc @flags @extra --crate-name $name --crate-type rlib (Join-Path $dir 'lib.rs') -o $rlib
    Check "Building $name ($variant)"
    return $rlib
}

function Build-Bench([string] $left, [string] $right, [string] $name, [string[]] $extra) {
    $exe = Join-Path $Work "$name.exe"
    rustc @flags @extra --extern "baseline=$left" --extern "candidate=$right" -L "native=$Avutil" $bench -o $exe
    Check "Building $name"
    return $exe
}

$left = Build-Library $Before 'baseline' 'plain'
$right = Build-Library $After 'candidate' 'plain'
$cycles = Build-Bench $left $right 'cycles' @()
if ($FullCorpus) {
    & $cycles full 1 $Cpu $Inputs (Join-Path $Work 'exact.csv') > (Join-Path $Work 'cycles.csv')
    Check 'Full corpus comparison'
    exit
}
if ($VerifyOnly) {
    & $cycles exact 1 $Cpu $Inputs > (Join-Path $Work 'verify.csv')
    Check 'Verification'
    exit
}
if ($Capture) {
    $captureLib = Build-Library $Before 'baseline' 'capture'
    $captureExe = Build-Bench $captureLib $right 'capture' @('--cfg', 'itg_capture')
    & $captureExe capture 1 $Cpu $Inputs $Trace
    Check 'Capturing actual coded blocks'
    exit
}

$alloc = Build-Bench $left $right 'allocations' @('--cfg', 'allocations')
$mode = if ($Operation -eq 'scan') { 'both' } else { 'pipeline' }
& $cycles $mode $Rounds $Cpu $Inputs > (Join-Path $Work 'cycles.csv')
Check 'Pipeline cycles'
& $alloc $mode 1 $Cpu $Inputs > (Join-Path $Work 'allocations.csv')
Check 'Pipeline allocations'
if (!$SkipVerify) {
    & $cycles exact 1 $Cpu $Inputs > (Join-Path $Work 'verify.csv')
    Check 'Verification'
}

if ($Operation -ne 'scan') {
    $left = Build-Library $Before 'baseline' 'kernels'
    $right = Build-Library $After 'candidate' 'kernels'
    $kernel = Build-Bench $left $right 'kernel-cycles' @('--cfg', 'itg_kernels')
    $kernelAlloc = Build-Bench $left $right 'kernel-allocations' @('--cfg', 'itg_kernels', '--cfg', 'allocations')
    & $kernel kernels ($Rounds * 2) $Cpu $Trace $Operation > (Join-Path $Work 'kernel-cycles.csv')
    Check 'Kernel cycles'
    & $kernelAlloc kernels 1 $Cpu $Trace $Operation > (Join-Path $Work 'kernel-allocations.csv')
    Check 'Kernel allocations'
}
