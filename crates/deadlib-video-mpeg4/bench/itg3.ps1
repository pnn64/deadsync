param(
    [Parameter(Mandatory = $true)] [string] $Before,
    [Parameter(Mandatory = $true)] [string] $After,
    [Parameter(Mandatory = $true)] [string] $Inputs,
    [Parameter(Mandatory = $true)] [string] $Work,
    [string] $Trace,
    [ValidateSet('scan', 'intra', 'dc', 'quant', 'motion', 'edge', 'transform', 'dcpredict', 'pipeline')] [string] $Operation = 'scan',
    [int] $Rounds = 5,
    [int] $Cpu = 12,
    [switch] $Capture,
    [switch] $CaptureExtra,
    [switch] $CapturePixels,
    [switch] $Full,
    [switch] $KernelsOnly,
    [switch] $SkipVerify
)
$ErrorActionPreference = 'Stop'
$Before = (Resolve-Path -LiteralPath $Before).Path
$After = (Resolve-Path -LiteralPath $After).Path
$Inputs = (Resolve-Path -LiteralPath $Inputs).Path
$Work = (New-Item -ItemType Directory -Force -Path $Work).FullName
$flags = @('--edition', '2021', '-C', 'opt-level=3', '-C', 'codegen-units=1')
$bench = Join-Path $PSScriptRoot 'itg3.rs'
$hooks = Join-Path $PSScriptRoot 'itg3_intra_hooks.rs'
$motionHooks = Join-Path $PSScriptRoot 'itg3_motion_hooks.rs'
$pixelHooks = Join-Path $PSScriptRoot 'itg3_pixel_hooks.rs'

function Check([string] $what) {
    if ($LASTEXITCODE -ne 0) { throw "$what failed: $LASTEXITCODE" }
}

function Build-Library([string] $source, [string] $name, [string] $variant) {
    $dir = Join-Path $Work "$name-$variant"
    New-Item -ItemType Directory -Force -Path $dir | Out-Null
    Copy-Item "$source/*.rs" $dir -Force
    $extra = @()
    if ($variant -ne 'plain') {
        $block = Join-Path $dir 'block.rs'
        $lib = Join-Path $dir 'lib.rs'
        $bits = Join-Path $dir 'bitstream.rs'
        $picture = Join-Path $dir 'picture.rs'
        $frame = Join-Path $dir 'frame.rs'
        if (!(Get-Content -Raw -LiteralPath $block).Contains('fn visual_dc_prediction(')) {
            # The original expression, used only by the isolated replay.
            Add-Content -Encoding UTF8 $block "`nfn visual_dc_prediction(dc: i32, scale: i32) -> i32 { (dc + scale / 2) / scale }"
        }
        Add-Content -Encoding UTF8 $block ("`n" + (Get-Content -Raw -LiteralPath $hooks))
        Add-Content -Encoding UTF8 $picture ("`n" + (Get-Content -Raw -LiteralPath $motionHooks))
        Add-Content -Encoding UTF8 $frame ("`n" + (Get-Content -Raw -LiteralPath $pixelHooks))
        Add-Content -Encoding UTF8 $lib "`npub use block::{PerfIntra, perf_intra_batch, perf_intra_exact, perf_dc_batch, perf_dc_exact, PerfQuant, perf_quant_batch, perf_quant_exact};"
        Add-Content -Encoding UTF8 $lib "`npub use block::{PerfDcPredict, perf_dcpredict_batch, perf_dcpredict_exact};"
        Add-Content -Encoding UTF8 $lib "`npub use picture::{PerfMotion, perf_motion_batch, perf_motion_exact};"
        Add-Content -Encoding UTF8 $lib "`npub use frame::{PerfEdge, perf_edge_batch, perf_edge_exact, PerfTransform, perf_transform_batch, perf_transform_exact};"
        Add-Content -Encoding UTF8 $bits @'

impl<'a> Bits<'a> {
    pub(crate) fn perf_position(&self) -> usize { self.position }
    #[cfg(itg_capture)]
    pub(crate) fn perf_tail(&self) -> (u8, &'a [u8]) {
        ((self.position % 8) as u8, &self.bytes[self.position / 8..])
    }
}
'@
        if ($variant -eq 'capture') {
            $extra += @('--cfg', 'itg_capture')
            Add-Content -Encoding UTF8 $lib "`npub use block::write_trace;"
            $text = Get-Content -Raw -LiteralPath $block
            $needle = '    let left_gradient = (left.dc - diagonal.dc).abs();'
            if (!$text.Contains($needle)) { throw 'intra signature changed' }
            $text = $text.Replace($needle, @'
    if !MP42 {
        trace::capture(bits, coded, separate_dc, predict_ac, quantizer, plane, [left, diagonal, above]);
    }
'@ + "`n" + $needle)
            Set-Content -Encoding UTF8 -LiteralPath $block $text
            if ($CaptureExtra) {
                $extra += @('--cfg', 'itg_capture_extra')
                Add-Content -Encoding UTF8 $lib "`npub use block::write_quant_trace; pub use picture::write_motion_trace;"
                $text = Get-Content -Raw -LiteralPath $block
                $needle = '    let mut scaled = coeff.map(|level| inverse_quantize::<MP42>(level, quantizer));'
                if (!$text.Contains($needle)) { throw 'intra quantization signature changed' }
                $text = $text.Replace($needle, "    if !MP42 { trace::capture_quant(quantizer, &coeff); }`n" + $needle)
                Set-Content -Encoding UTF8 -LiteralPath $block $text
                $text = Get-Content -Raw -LiteralPath $picture
                $needle = 'fn motion_component(bits: &mut Bits<''_>, predictor: i32, code: u8) -> Result<i32> {'
                if (!$text.Contains($needle)) { throw 'motion component signature changed' }
                $text = $text.Replace($needle, $needle + "`n    motion_trace::capture(bits, predictor, code);")
                Set-Content -Encoding UTF8 -LiteralPath $picture $text
            }
            if ($CapturePixels) {
                $extra += @('--cfg', 'itg_capture_pixels')
                Add-Content -Encoding UTF8 $lib "`npub use frame::{write_edges, write_transforms};"
                $text = Get-Content -Raw -LiteralPath $frame
                $needle = 'fn extend_edge(&self, [x, y]: [i32; 2], span: usize, rows: usize, tile: &mut [u8]) {'
                if (!$text.Contains($needle)) { throw 'edge extension signature changed' }
                $text = $text.Replace($needle, $needle + "`n        pixel_trace::edge(self, x, y, span, rows);")
                Set-Content -Encoding UTF8 -LiteralPath $frame $text
                $transform = Join-Path $dir 'transform.rs'
                $text = Get-Content -Raw -LiteralPath $transform
                $needle = 'pub(crate) fn inverse(coefficients: &[i16; 64]) -> [i16; 64] {'
                if (!$text.Contains($needle)) { throw 'inverse transform signature changed' }
                $text = $text.Replace($needle, $needle + "`n    crate::frame::capture_transform(coefficients);")
                Set-Content -Encoding UTF8 -LiteralPath $transform $text
            }
        }
    }
    $rlib = Join-Path $Work "lib$name-$variant.rlib"
    rustc @flags @extra --crate-name $name --crate-type rlib (Join-Path $dir 'lib.rs') -o $rlib
    Check "Building $name ($variant)"
    return $rlib
}

function Build-Bench([string] $left, [string] $right, [string] $name, [string[]] $extra) {
    $exe = Join-Path $Work "$name.exe"
    rustc @flags @extra --extern "baseline=$left" --extern "candidate=$right" $bench -o $exe
    Check "Building $name"
    return $exe
}

if (!$KernelsOnly) {
    $left = Build-Library $Before 'baseline' 'plain'
    $right = Build-Library $After 'candidate' 'plain'
    $cycles = Build-Bench $left $right 'cycles' @()
    if ($Full) {
        & $cycles full $Rounds $Cpu $Inputs (Join-Path $Work 'exact.csv') > (Join-Path $Work 'cycles.csv')
        Check 'Full corpus'
        $alloc = Build-Bench $left $right 'allocations' @('--cfg', 'allocations')
        & $alloc allocall 1 $Cpu $Inputs > (Join-Path $Work 'allocations.csv')
        Check 'Full corpus allocations'
        exit
    }
    if ($Capture) {
        if (!$Trace) { $Trace = Join-Path $Work 'intra.bin' }
        $left = Build-Library $Before 'baseline' 'capture'
        $captureFlags = @('--cfg', 'itg_capture')
        if ($CaptureExtra) { $captureFlags += @('--cfg', 'itg_capture_extra') }
        if ($CapturePixels) { $captureFlags += @('--cfg', 'itg_capture_pixels') }
        $exe = Build-Bench $left $right 'capture' $captureFlags
        & $exe capture 1 $Cpu $Inputs $Trace
        Check 'Capturing actual decoder inputs'
        exit
    }
    $alloc = Build-Bench $left $right 'allocations' @('--cfg', 'allocations')
    $mode = if ($Operation -eq 'scan') { 'both' } else { 'pipeline' }
    & $cycles $mode $Rounds $Cpu $Inputs > (Join-Path $Work 'cycles.csv')
    Check 'Pipeline cycles'
    & $alloc $mode 1 $Cpu $Inputs > (Join-Path $Work 'allocations.csv')
    Check 'Pipeline allocations'
    if (!$SkipVerify) {
        & $cycles full 0 $Cpu $Inputs (Join-Path $Work 'exact.csv') > $null
        Check 'Exact output comparison'
    }
}
if ($Operation -in 'intra', 'dc', 'quant', 'motion', 'edge', 'transform', 'dcpredict') {
    $left = Build-Library $Before 'baseline' 'kernels'
    $right = Build-Library $After 'candidate' 'kernels'
    $kernel = Build-Bench $left $right 'kernel-cycles' @('--cfg', 'itg_kernels')
    $kernelAlloc = Build-Bench $left $right 'kernel-allocations' @('--cfg', 'itg_kernels', '--cfg', 'allocations')
    $kernelMode = if ($Operation -in 'quant','motion','edge','transform','dcpredict') { $Operation } else { 'intra' }
    & $kernel $kernelMode ($Rounds * 2) $Cpu $Trace > (Join-Path $Work 'kernel-cycles.csv')
    Check 'Kernel cycles'
    & $kernelAlloc $kernelMode 1 $Cpu $Trace > (Join-Path $Work 'kernel-allocations.csv')
    Check 'Kernel allocations'
    if ($Operation -eq 'dc') {
        & $kernel dc ($Rounds * 3) $Cpu $Trace > (Join-Path $Work 'dc-cycles.csv')
        Check 'DC lookup cycles'
        & $kernelAlloc dc 1 $Cpu $Trace > (Join-Path $Work 'dc-allocations.csv')
        Check 'DC lookup allocations'
    }
}
