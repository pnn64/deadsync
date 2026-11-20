# BMP codec measurements

This independent Cargo workspace compares the dependency-free native BMP codec
with `image 0.25.10` using its BMP feature only. It adds no dependencies to the
production crate. Build both timing and allocation binaries before changing the
codec, retain copies, and use the same generated inputs and options after each
change. The timing executable includes independent expected-pixel verification.

From the repository root:

```powershell
cargo build --manifest-path crates/deadlib-image/bmp/bench/Cargo.toml --release --locked --bin bmp-codec-bench --target-dir D:/deadsync-bmp-perf-20261120/timing
$env:BMP_BENCH_CPU = '2'
$env:BMP_BENCH_MS = '20'
$env:BMP_BENCH_SAMPLES = '5'
D:/deadsync-bmp-perf-20261120/timing/release/bmp-codec-bench.exe --mode rgba
```

The Windows thread is pinned to the selected logical CPU (default 0) in its
current processor group. `thread_cycles` uses `QueryThreadCycleTime`; all measured
BMP operations are synchronous and single threaded. Other targets explicitly
report that cycles are unavailable and produce zero cycle counts. Timing and
cycle medians are selected independently from five samples by default. Each
sample runs for at least `BMP_BENCH_MS` milliseconds in groups of eight operations,
or one above one million pixels. Modes rotate within each sample to reduce order
bias. Destruction is included, file I/O and fixture generation are excluded, and
each operation receives three untimed warmups. The minimum and maximum sampled
cycle counts accompany the median. `mpixels_sec` is output pixels per second,
divided by one million, derived from median elapsed time.

## Workloads and comparisons

The default 120-case synthetic matrix covers 1x1, 17x9, 511x257, and 1920x1080
images, both row orientations where supported, full 1/4/8-bit palettes, a short
8-bit palette, OS/2 indexed/RGB headers, RGB24, RGB555, RGB565, unusual RGB333
masks, RGBA4444, BGRX32, explicit standard/swapped RGBA32, implicit BGRA32, and
legacy all-zero alpha. Pixel values vary across the image. Odd widths exercise
every relevant packing boundary; nonzero padding and unused packed bits must
not leak into output.

Fixtures construct BMP wire bytes and expected `Bitmap`/RGBA values independently
of both decoders. Mask expansion uses independent bit-by-bit repetition. Before
timing, native metadata, representation, and every pixel must match, and native
encoding must round-trip to those expected pixels. Reference RGBA is compared
byte for byte. Image timing is included only where the exact RGBA outputs agree.
Known mask-expansion and implicit-alpha differences are logged individually as
`NON_EQUIVALENT`, with differing-sample counts and maximum differences; they are
not accepted as equivalent performance comparisons. Unsupported reference inputs
are logged as `REFERENCE_UNAVAILABLE`. No speed threshold filters the corpus.

| Mode | Measured work |
| --- | --- |
| `native-decode` | `decode` producing the existing palette/RGB/RGBA representation |
| `native-rgba` | `decode` followed by the public borrowed `to_rgba8` API |
| `image-rgba` | `load_from_memory_with_format(Bmp).into_rgba8()` with exactly equal output |
| `native-convert` | `to_rgba8` on a previously built bitmap, excluding decode |
| `native-encode` | `encode` from indexed/RGB/RGBA input |
| `image-encode` | `BmpEncoder::encode` from RGB/RGBA input, when decoded output agrees |

The fair end-to-end decoder comparison is `native-rgba` versus `image-rgba`.
Decode-only results describe the native API and are not compared with image's
expanded palette output. Encoding compares equal source samples and equal
decoded pixels, allowing different legal file headers. Indexed image encoding
is excluded because its file representation differs from native RGB24 encoding.
All encoder measurements use their normal bottom-up output orientation; the
fixture suffix names decode input orientation.

`--mode all|rgba|decode|convert|encode` selects operations (default `all`),
`--filter SUBSTRING` selects case names, and `--verify-only` performs untimed
decode/pixel/native-encode checks. Positional BMP files/directories replace the
synthetic matrix. External corpus inputs use current native decoding for their
expected representation: those runs establish reference agreement and timings,
not independence from the previous native implementation. Native errors are
logged as `NATIVE_UNAVAILABLE`. Use captured independent fixtures or the frozen
snapshot below for exact native behavior preservation. The application's
specialized indexed-BMP loader is a separate comparison from image's public API.

## Allocation measurements

```powershell
cargo build --manifest-path crates/deadlib-image/bmp/bench/Cargo.toml --release --locked --features allocations --bin bmp-codec-bench --target-dir D:/deadsync-bmp-perf-20261120/allocations
D:/deadsync-bmp-perf-20261120/allocations/release/bmp-codec-bench.exe --mode rgba
```

The instrumented allocator forwards to `System`; a calibration checks allocation,
zeroed allocation, reallocation, and freeing before measuring. Counts include
allocation/zeroed allocation/reallocation calls; requested bytes include complete
replacement sizes on reallocation. They are not peak live memory or RSS. Timing
columns are zero in the instrumented binary, so atomic counter overhead never
affects the reported performance measurements. All unsafe code in this workspace
is measurement FFI or transparent allocator forwarding; the codec remains safe.

## Exact native snapshots

```powershell
cargo build --manifest-path crates/deadlib-image/bmp/bench/Cargo.toml --release --locked --bin native_snapshot --target-dir D:/deadsync-bmp-perf-20261120/timing
# Capture with the prechange binary; the destination must not exist.
D:/deadsync-bmp-perf-20261120/timing/release/native_snapshot.exe capture D:/deadsync-bmp-perf-20261120/baseline.snapshot
# Rebuild after each optimization, then compare the complete serialization.
D:/deadsync-bmp-perf-20261120/timing/release/native_snapshot.exe verify D:/deadsync-bmp-perf-20261120/baseline.snapshot
```

The snapshot stores full input bytes and observable probe/decode/convert/encode
results, including exact error variants/messages, sample representation, palette,
dimensions, alpha, and encoder bytes in both orientations. It covers the same
layouts at small sizes, every truncated prefix of 17x3 fixtures, header/mask and
pixel mutations, input/pixel/storage limit boundaries, and invalid caller-created
bitmaps. Mutated files use explicit small limits to bound resource use. Replay
compares every byte; it does not rely on a pixel hash or on reference codec
agreement. These bounded generated cases supplement the crate's correctness
tests and do not claim exhaustive coverage of all BMP files.
