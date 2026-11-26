# Cross-codec PNG benchmark

Run from the repository root on Windows x86-64 with AVX2, Visual Studio 2022
Build Tools, CMake, Rust, Git and Python with Pillow.

```powershell
& ./crates/deadlib-image/png/bench/contenders/setup.ps1
$base = 'target/png-codec-comparison'
$exe = "$base/rust-build/release/compare.exe"
$env:PNG_BENCH_MS = '8'
$env:PNG_BENCH_CPU = '2'
foreach ($run in 1, 2) {
    $modes = if ($run -eq 1) { 'decode', 'encode', 'fpng' } else { 'fpng', 'encode', 'decode' }
    foreach ($mode in $modes) {
        & $exe $mode "$base/$mode-inputs.tsv" | Set-Content -Encoding utf8 "$base/$mode-run$run.tsv"
        if ($LASTEXITCODE -ne 0) { throw "$mode failed" }
    }
}
$env:PNG_BENCH_MS = '20'
foreach ($run in 1, 2) {
    & $exe animation "$base/animation-inputs.tsv" | Set-Content -Encoding utf8 "$base/animation-run$run.tsv"
    if ($LASTEXITCODE -ne 0) { throw "animation failed" }
}
$runs = Get-ChildItem $base -Filter '*-run?.tsv' | ForEach-Object FullName
python crates/deadlib-image/png/bench/contenders/summarize.py --output "$base/summary.json" $runs
```

The setup pins the external repositories in [upstream.tsv](upstream.tsv) and
Rust dependencies in [Cargo.lock](../Cargo.lock). It builds the C/C++ codecs
into one benchmark DLL. `corpus.py` prepares the input lists, verifies the
original Kodak SHA-256 hashes, and writes the chosen paths and input hashes to
`inputs.json`. It uses the available captures from the prior screenshot manifest.
Use the archived paths and hashes for an exact corpus comparison.

The binary accepts `--verify-only` after the input list. The static decoders
must reproduce the Rust png pixel output and dimensions. RGBA16 output uses
little-endian samples. The static encoders must reproduce the native color type,
sample depth, interlace mode and packed pixel values, including indexed palette
entries. RGBA8 encoder inputs preserve their existing alpha. The APNG workload
compares the default image and all owned subframes, including their rectangles,
delay, disposal and blend. Times include the codec operation and destruction of
the owned result. Input I/O, verification and TSV output happen outside timing.
Set `PNG_COMPARE_ENCODE_MODES` to a comma-separated list of codec names
from the encode TSV to run only those modes.

Each number is the median of five batches. The cycle and wall time medians are
selected independently. Use a logical CPU available to the process. The TSV
records verification failures and unsupported modes. The summary combines
repeat medians and compares each pair on the intersection of cases it verified.
