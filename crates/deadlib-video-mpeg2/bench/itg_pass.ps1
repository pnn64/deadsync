param(
    [Parameter(Mandatory = $true)] [string] $Before,
    [Parameter(Mandatory = $true)] [string] $After,
    [Parameter(Mandatory = $true)] [string] $Inputs,
    [Parameter(Mandatory = $true)] [string] $Work,
    [Parameter(Mandatory = $true)] [string] $Avutil,
    [string] $Trace,
    [ValidateSet('scan', 'block', 'inverse', 'flag', 'read-motion', 'prediction')] [string] $Operation = 'scan',
    [int] $Rounds = 5,
    [int] $Cpu = 12,
    [int] $KernelRepeats,
    [switch] $Capture,
    [switch] $SkipVerify,
    [switch] $VerifyOnly,
    [switch] $FullCorpus
)

$ErrorActionPreference = 'Stop'
$kernelExtra = @()
if ($PSBoundParameters.ContainsKey('KernelRepeats')) {
    if ($KernelRepeats -le 0) { throw 'KernelRepeats must be positive' }
    $kernelExtra = @($KernelRepeats)
}
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
$flagHooks = Join-Path $PSScriptRoot 'itg_flag_hooks.rs'
$motionHooks = Join-Path $PSScriptRoot 'itg_motion_hooks.rs'
$motionBits = Join-Path $PSScriptRoot 'itg_motion_bits.rs'
$predictionHooks = Join-Path $PSScriptRoot 'itg_prediction_hooks.rs'
$flags = @('--edition', '2021', '-C', 'opt-level=3', '-C', 'codegen-units=1')

function Check([string] $what) {
    if ($LASTEXITCODE -ne 0) { throw "$what failed: $LASTEXITCODE" }
}

function Build-Library([string] $source, [string] $name, [string] $variant) {
    $dir = Join-Path $Work "$name-$variant"
    New-Item -ItemType Directory -Force -Path $dir | Out-Null
    Copy-Item "$source/*.rs" $dir -Force
    $extra = @()
    if ($variant -ne 'plain' -and $Operation -eq 'flag') {
        $bits = Join-Path $dir 'bitstream.rs'
        $lib = Join-Path $dir 'lib.rs'
        Add-Content -Encoding UTF8 $bits ("`n" + (Get-Content -Raw -LiteralPath $flagHooks))
        Add-Content -Encoding UTF8 $lib "`npub use bitstream::{perf_flag_batch, perf_flag_values};"
        if ($variant -eq 'capture') {
            $extra += @('--cfg', 'itg_flag_capture')
            Add-Content -Encoding UTF8 $lib "`npub use bitstream::write_flag_trace as write_trace;"
            $text = Get-Content -Raw -LiteralPath $bits
            $needle = 'pub(crate) fn flag(&mut self) -> Result<bool> {'
            if (!$text.Contains($needle)) { throw 'Bits::flag signature changed' }
            $text = $text.Replace($needle, "$needle`n        record_flag(self);")
            Set-Content -Encoding UTF8 -LiteralPath $bits $text
        }
    } elseif ($variant -ne 'plain' -and $Operation -eq 'read-motion') {
        $bits = Join-Path $dir 'bitstream.rs'
        $slice = Join-Path $dir 'slice.rs'
        $lib = Join-Path $dir 'lib.rs'
        Add-Content -Encoding UTF8 $bits ("`n" + (Get-Content -Raw -LiteralPath $motionBits))
        Add-Content -Encoding UTF8 $slice ("`n" + (Get-Content -Raw -LiteralPath $motionHooks))
        Add-Content -Encoding UTF8 $lib "`npub use slice::{perf_motion_batch, perf_motion_values};"
        if ($variant -eq 'capture') {
            $extra += @('--cfg', 'itg_motion_capture')
            Add-Content -Encoding UTF8 $lib "`npub use slice::write_motion_trace as write_trace;"
            $text = Get-Content -Raw -LiteralPath $slice
            $needle = 'for (predictor, code) in motion.iter_mut().zip(self.coding.f_code[direction]) {'
            if (!$text.Contains($needle)) { throw 'Slice::read_motion loop changed' }
            $text = $text.Replace($needle, "record_motion(bits, *motion, self.coding.f_code[direction]);`n        $needle")
            Set-Content -Encoding UTF8 -LiteralPath $slice $text
        }
    } elseif ($variant -ne 'plain' -and $Operation -eq 'prediction') {
        $frame = Join-Path $dir 'frame.rs'
        $lib = Join-Path $dir 'lib.rs'
        Add-Content -Encoding UTF8 $frame ("`n" + (Get-Content -Raw -LiteralPath $predictionHooks))
        Add-Content -Encoding UTF8 $lib "`npub use frame::PerfPredictions;"
        if ($variant -eq 'capture') {
            $extra += @('--cfg', 'itg_prediction_capture')
            Add-Content -Encoding UTF8 $lib "`npub use frame::write_prediction_trace as write_trace;"
            $text = Get-Content -Raw -LiteralPath $frame
            $needle = 'let source_x = x as i32 + (motion[0] >> 1);'
            if (!$text.Contains($needle)) { throw 'Plane::predict source coordinate changed' }
            $text = $text.Replace($needle, "record_prediction::<W>(self, x, y, source, motion, average);`n        $needle")
            Set-Content -Encoding UTF8 -LiteralPath $frame $text
        }
    } elseif ($variant -ne 'plain') {
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
    Check 'Capturing actual decoder operations'
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
    $kernelCfg = switch ($Operation) {
        'flag' { 'itg_flags' }
        'read-motion' { 'itg_motion' }
        'prediction' { 'itg_prediction' }
        default { 'itg_kernels' }
    }
    $kernel = Build-Bench $left $right 'kernel-cycles' @('--cfg', $kernelCfg)
    $kernelAlloc = Build-Bench $left $right 'kernel-allocations' @('--cfg', $kernelCfg, '--cfg', 'allocations')
    & $kernel kernels ($Rounds * 2) $Cpu $Trace $Operation @kernelExtra > (Join-Path $Work 'kernel-cycles.csv')
    Check 'Kernel cycles'
    & $kernelAlloc kernels 1 $Cpu $Trace $Operation @kernelExtra > (Join-Path $Work 'kernel-allocations.csv')
    Check 'Kernel allocations'
}
