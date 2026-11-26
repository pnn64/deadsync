# JPEG codec comparison

This Windows x86-64 benchmark compares the public Deadlib pixel APIs with
libjpeg-turbo, MozJPEG, JPEGli, Zune, image, jpeg-decoder, ZenJPEG, Wuffs, and
jpgd for decoding. Encoding also includes sjpeg and jpeg-encoder. The C and C++
bridges use statically linked upstream release builds inside separate DLLs so
that the different libjpeg implementations can coexist in one process.
The [Windows x86-64 results](../results/windows-x86_64-20261126-contenders/README.md)
include per-file measurements for both runs.

## Reproduce the measurements

Install Rust, Python with Pillow, Git, CMake, NASM and the Visual Studio 2022
C++ toolchain. The Windows binary requires an AVX2-capable x64 CPU for the
configured C/C++ builds. From the repository root, run the setup to fetch
[the pinned revisions](upstream.tsv), build the contenders, and prepare the
corpus under `target/jpeg-codec-comparison`. The setup reuses SHA-256-verified
Kodak PNGs when available. The optional work directory must be inside the repo.
The encoder corpus uses two fixture-submodule captures and one local screenshot
when present. The result archive identifies every input by SHA-256.

```powershell
& crates/deadlib-image/jpeg/bench/contenders/setup.ps1
$base = 'target/jpeg-codec-comparison'
$runner = "$base/rust-build/release/compare.exe"
$bridge = "$base/bridges/Release"
$env:RAYON_NUM_THREADS = '1'
$env:JPEG_BENCH_CPU = '2'
$env:JPEG_BENCH_MS = '8'

& $runner encode "$base/encode-inputs.tsv" $bridge | Out-File -Encoding utf8 "$base/encode-run1.tsv"
& $runner decode "$base/decode-inputs.tsv" $bridge | Out-File -Encoding utf8 "$base/decode-run1.tsv"
& $runner decode "$base/decode-inputs.tsv" $bridge | Out-File -Encoding utf8 "$base/decode-run2.tsv"
& $runner encode "$base/encode-inputs.tsv" $bridge | Out-File -Encoding utf8 "$base/encode-run2.tsv"

$summary = 'crates/deadlib-image/jpeg/bench/contenders/summarize.py'
python $summary decode --output "$base/decode-summary.json" "$base/decode-run1.tsv" "$base/decode-run2.tsv"
python $summary encode --output "$base/encode-summary.json" "$base/encode-run1.tsv" "$base/encode-run2.tsv"
```

Appending `--verify-only` to the runner arguments checks output without collecting
timings. The aggregate ratios use the sums of per-file CPU cycles on each pair's
shared set. Each per-file time is the median of five batches. The summarizer
takes the median across process runs and accepts supplementary runs over a
subset with `--repeat-subset`. JPEG and raw pixel buffers are in memory. Timings include
codec setup, allocation, raster conversion when needed, and output destruction.
The process runs on a single logical CPU. File I/O and correctness checks
happen outside the timed loop.

Ordinary decoders return packed grayscale or RGB. Turbo's default accurate IDCT
and fancy upsampling supply the pixel reference where available. Turbo Fast
uses its fast IDCT and disables fancy upsampling. Deadlib uses centered linear
upsampling except on 4:1:1 and 4:1:0, where the benchmark requests nearest to
match Turbo's box expansion. High-precision output is normalized to original
sample precision. RGB display for CMYK source files uses subtractive conversion
with the Adobe polarity. For paired decode timing the raster must agree in shape
and reach at least 28 dB PSNR, with exact gray samples required for lossless JPEG.
When Turbo cannot render a process, the harness uses Deadlib's raster as the
reference. Those numbers measure acceptance and agreement with that raster.

Encoders produce a complete in-memory JPEG. Verification checks its process,
precision, sampling and decoded dimensions. Results include file size, per-raster
RGB or gray PSNR, and maximum absolute sample error. Lossless output must round
trip exactly. JPEG quality factors are library-specific. The comparisons report
size and PSNR deltas at the specified quality setting. Encoder scan scripts and
quantization choices can differ. The optimized and fixed Huffman configurations
are timed separately where available. The sjpeg profiles use method 0 for
fixed tables and method 1 for optimized tables.
