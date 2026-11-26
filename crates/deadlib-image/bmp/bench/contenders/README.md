# BMP contender benchmark

The benchmark compares end-to-end in-memory BMP APIs. Decode produces an owned,
packed, straight RGBA8 raster in top-down order. Encode starts with equivalent
packed RGB8 or RGBA8 pixels and produces a complete in-memory BMP. The source
buffers, output lifetime, and pixel format conversions follow each public API.
Allocation and destruction are timed. Disk access, corpus generation, and
verification are outside the measured intervals.

The corpus combines the existing 120-case independent fixture matrix, 67 BMP
Suite files, eight generated large RLE4/RLE8 files, and eight RGB24 BMPs made from
hash-checked Kodak images. The suite uses published PNG renderings as its default
reference. It uses Pillow raw pixels for the horizontally stretched display
reference and the alternate ICC sample, and the embedded JPEG is decoded with
Pillow. Color management and display aspect ratio are outside the contract.
Cases with ambiguous delta/background rendering or truncated RLE data and the
suite's deliberately broken files are skipped. [BMP Suite 2.8](https://entropymine.com/jason/bmpsuite/)
and [Kodak image collection](https://r0k.us/graphics/kodak/).

Encode measures the two native output layouts, RGB24 and RGBA32, both row
directions when available, and a fully transparent RGBA32 case at each size.
Palette-expanded workloads use matching packed RGB input. The original Rust
harness separately measures Deadlib's indexed input API, which writes RGB24.
The encoder verification probes other codecs for an exact RGBA round trip and
records the DIB size, bit depth, compression, and row order. An independent
decoder must agree byte for byte for the row to enter the timing tables.

CMake builds the C and C++ adapters with MSVC in Release mode. The shared
FFmpeg build uses a single complete BMP packet per call and swscale for output
conversion. WIC reuses the system imaging factory. SDL3 uses native surfaces
and memory streams. OpenCV and Pillow use their Python bindings, so their
results include the work of that public API. The Rust builds use the workspace
release profile with the default Windows x64 target CPU.

## Reproduce on Windows x64

Run from the repository root with Python 3.13, Git, Rust, CMake, and the Visual
Studio 2022 C++ toolchain. Downloads, compiled binaries, and generated images
go under the chosen work directory.

```powershell
$bmpTools = 'crates/deadlib-image/bmp/bench/contenders'
$bmpWork = [IO.Path]::GetFullPath('target/bmp-codec-comparison')
python "$bmpTools/fetch.py" $bmpWork
python -m pip install --target "$bmpWork/python" Pillow==12.3.0 opencv-python-headless==4.13.0.92 numpy==2.5.5
$env:PYTHONPATH = "$bmpWork/python"
cmake -S $bmpTools -B "$bmpWork/build" -G 'Visual Studio 17 2022' -A x64 "-DWORK=$bmpWork" "-DPINS=$bmpWork/upstream" "-DFFMPEG=$bmpWork/upstream/ffmpeg-9.1-full_build-shared"
cmake --build "$bmpWork/build" --config Release --parallel 2
cargo build --manifest-path crates/deadlib-image/bmp/bench/Cargo.toml --release --locked --features comparison --bins --target-dir "$bmpWork/rust-build"
& "$bmpWork/rust-build/release/compare.exe" prepare "$bmpWork/corpus"
python "$bmpTools/corpus.py" "$bmpWork/corpus" "$bmpWork/upstream/suite/bmpsuite-2.8" "$bmpWork/kodak"
& "$bmpTools/run.ps1" -Work $bmpWork
python "$bmpTools/summarize.py" "$bmpWork/results" --bundle-from $bmpWork
```

`BMP_BENCH_CPU` selects the logical processor within the process's group
(default 2). `BMP_BENCH_MS` selects the minimum time of each of five samples
(default 6 ms). The runners rotate codec order, reverse the traversal on pass
two, and report the median process cycles and median elapsed time separately.
The report combines the two pass medians with a geometric mean, and normalizes
comparisons to identical verified cases. A 3% band is used when counting
pairwise wins and losses. `summarize.py REPORT_DIR` rebuilds the summary from
saved results and checks that every case and codec produced identical
non-timing fields across both passes.

`compare.exe decode-wuffs MANIFEST DLL_DIRECTORY` compares the staged native
API, the direct `decode_rgba8` API, and Wuffs in the same process. Its TSV
labels are `deadlib`, `deadlib-rgba`, and `wuffs`. Use the full decode manifest
for coverage or a filtered copy for shorter experiments.
