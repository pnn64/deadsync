# BMP codec measurements

The [Windows contender benchmark](results/windows-x86_64-20261126-contenders/README.md)
compares Deadlib with ten decoders and eight encoders. Its reproducible harness
and pinned dependency setup are in [`contenders`](contenders/README.md).

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
or one at or above one million pixels. Modes rotate within each sample to reduce
order bias. Destruction is included, file I/O and fixture generation are excluded, and
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

Set `BMP_BENCH_SIZES` to a comma-separated list of positive `WIDTHxHEIGHT`
dimensions to replace only the synthetic sizes. The order is preserved, repeated
sizes are rejected, and the existing layouts, row orientations, and expected-pixel
construction remain unchanged. Keep each dimension at most 65535 for the OS/2
fixtures, whose headers store 16-bit dimensions. For example, these dimensions
exercise 1023, 1024, 1025, and 4096 pixels around a conversion cutoff:

```powershell
$env:BMP_BENCH_SIZES = '31x33,32x32,41x25,64x64'
D:/deadsync-bmp-perf-20261120/timing/release/bmp-codec-bench.exe --mode convert --filter indexed
Remove-Item Env:BMP_BENCH_SIZES
```

With the variable absent, the original 120-case matrix and 662 `all`-mode keys
remain unchanged. Positional external files/directories do not use this override.
Use identical size settings for both sides of a before/after comparison.

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
| `native-owned-rgba` | `decode` followed by the consuming `into_rgba8` API; explicit `owned-rgba` mode only |
| `image-rgba` | `load_from_memory_with_format(Bmp).into_rgba8()` with exactly equal output |
| `native-convert` | `to_rgba8` on a previously built bitmap, excluding decode |
| `native-encode` | `encode` from indexed/RGB/RGBA input |
| `image-encode` | `BmpEncoder::encode` from RGB/RGBA input, when decoded output agrees |

`native-rgba` measures the existing borrowed conversion: the returned RGBA buffer
is independent of the decoded bitmap. It remains available for before/after
comparisons and does not establish that every borrowed case beats image.
`native-owned-rgba` measures a caller that gives up the decoded bitmap: valid
RGBA storage can move directly into the result, while RGB/indexed pixels use the
existing conversion. Compare that consuming ownership contract with image's
`DynamicImage::into_rgba8`, which can also reuse existing RGBA storage. Both
comparisons require exactly equal dimensions and output pixels. Report borrowed
and consuming measurements separately; do not substitute consuming results into
the historical borrowed dataset.

Decode-only results describe the native API and are not compared with image's
expanded palette output. Encoding compares equal source samples and equal
decoded pixels, allowing different legal file headers. Indexed image encoding
is excluded because its file representation differs from native RGB24 encoding.
All encoder measurements use their normal bottom-up output orientation; the
fixture suffix names decode input orientation.

`--mode all|rgba|owned-rgba|decode|convert|encode` selects operations (default `all`),
`--filter SUBSTRING` selects case names, and `--verify-only` performs untimed
decode/pixel/native-encode checks. `owned-rgba` additionally checks the consuming
output against independent expected pixels before timing. It produces only
`native-owned-rgba` and byte-equivalent `image-rgba` rows. It is excluded from
`all`; the historical `all`/`rgba` operations and their verification work remain
unchanged so the original 662-row matrix stays comparable.

```powershell
D:/deadsync-bmp-perf-20261120/timing/release/bmp-codec-bench.exe --mode owned-rgba
python crates/deadlib-image/bmp/bench/analyze.py --reference owned.tsv --native-mode native-owned-rgba
```

The analyzer defaults to `--native-mode native-rgba`; select
`native-owned-rgba` explicitly for the consuming dataset. The same mode flag on
the allocation executable measures its allocation calls and requested bytes.

Positional BMP files/directories replace the
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
agreement. The helper also asserts that consuming a clone returns exactly the
borrowed conversion result, including errors, under default and custom limits.
Those assertions add no serialized fields, so the original 22,570-record snapshot
remains the comparison source. These bounded generated cases supplement the
crate's correctness tests and do not claim exhaustive coverage of all BMP files.

## Recorded optimization results

The [2026-11-20 report](results/windows-x86_64-20261120/README.md) includes every
before/after stage, final reference comparisons, allocation measurements,
observed regressions, validation, and exact environment metadata. The consuming
pipeline beat image on all 98 equivalent synthetic cases in two complete runs
and on all 12 valid repository BMP fixtures on that host.

The [second-pass report](results/2026-11-20-pass2/README.md) preserves the later
decoder experiments, target gains, unchanged-format regressions, and complete
reproduction evidence. No production optimization from that pass was retained:
the original BMP source was restored after the reduced candidate failed the
control gate. The additional independent regression tests remain.
