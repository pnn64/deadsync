param(
    [Parameter(Mandatory = $true)] [string] $Before,
    [Parameter(Mandatory = $true)] [string] $After,
    [Parameter(Mandatory = $true)] [string] $Inputs,
    [Parameter(Mandatory = $true)] [string] $Work,
    [string] $Trace,
    [ValidateSet('scan', 'intra', 'dc', 'pipeline')] [string] $Operation = 'scan',
    [int] $Rounds = 5,
    [int] $Cpu = 12,
    [switch] $Capture,
    [switch] $Full,
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
        Add-Content -Encoding UTF8 $block ("`n" + (Get-Content -Raw -LiteralPath $hooks))
        Add-Content -Encoding UTF8 $lib "`npub use block::{PerfIntra, perf_intra_batch, perf_intra_exact, perf_dc_batch, perf_dc_exact};"
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
    $exe = Build-Bench $left $right 'capture' @('--cfg', 'itg_capture')
    & $exe capture 1 $Cpu $Inputs $Trace
    Check 'Capturing actual intra blocks'
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
if ($Operation -in 'intra', 'dc') {
    $left = Build-Library $Before 'baseline' 'kernels'
    $right = Build-Library $After 'candidate' 'kernels'
    $kernel = Build-Bench $left $right 'kernel-cycles' @('--cfg', 'itg_kernels')
    $kernelAlloc = Build-Bench $left $right 'kernel-allocations' @('--cfg', 'itg_kernels', '--cfg', 'allocations')
    & $kernel intra ($Rounds * 2) $Cpu $Trace > (Join-Path $Work 'kernel-cycles.csv')
    Check 'Kernel cycles'
    & $kernelAlloc intra 1 $Cpu $Trace > (Join-Path $Work 'kernel-allocations.csv')
    Check 'Kernel allocations'
    if ($Operation -eq 'dc') {
        & $kernel dc ($Rounds * 3) $Cpu $Trace > (Join-Path $Work 'dc-cycles.csv')
        Check 'DC lookup cycles'
        & $kernelAlloc dc 1 $Cpu $Trace > (Join-Path $Work 'dc-allocations.csv')
        Check 'DC lookup allocations'
    }
}
