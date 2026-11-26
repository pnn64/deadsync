# PNG measurements

This development-only workspace compares `deadlib-image-png`, `png 0.18.1` and
`image 0.25.10`. It does not add production dependencies. Build in release mode;
the lock file pins the reference implementations and their dependencies.

Decoder measurements and remaining performance gaps are in [RESULTS.md](RESULTS.md).
Encoder measurements and their recorded baseline are in
[ENCODE_RESULTS.md](ENCODE_RESULTS.md).
The current default Fast/Adaptive results and implementation analysis are in
[FAST_ADAPTIVE.md](FAST_ADAPTIVE.md).

The expanded cross-codec comparison is in
[results/windows-x86_64-20261126-contenders](results/windows-x86_64-20261126-contenders/README.md).
It adds the `compare` binary, C/C++ contenders, native-format encoding and
whole-animation APNG workloads. It uses a separate build from the existing commands
below.

## Decoder workload

```powershell
python ../scripts/generate_decode_corpus.py --output C:/GitHub/deadsync/target/png-formats
python ../scripts/fetch_corpus.py --output C:/GitHub/deadsync/target/png-kodak
cargo build --release --locked --bin png-decode-bench --target-dir C:/GitHub/deadsync/target/png-bench
$env:PNG_BENCH_MS = '80'
C:/GitHub/deadsync/target/png-bench/release/png-decode-bench.exe C:/GitHub/deadsync/target/png-formats C:/GitHub/deadsync/target/png-kodak
```

Run from this directory. Files and directories are accepted; directory traversal
is recursive. The generated 60-file corpus covers all native color/depth pairs,
plain and Adam7 rows, compressible and noisy data, and all five filters. Kodak's
24 external photos are checked against the existing size/SHA-256 manifest.
Before timing, each input's native samples are compared to Rust png, and its
RGBA16 rendering is compared to image. All decodes include output destruction.

To check the bundled image corpus without measuring performance, run from the
repository root:

```powershell
cargo run --release --locked --manifest-path crates/deadlib-image/png/bench/Cargo.toml --bin png-decode-bench -- --verify-only assets courses crates/deadsync-song-lua/assets
```

`--verify-only` recursively checks the supplied PNG files and directories with
the same native-sample and RGBA16 comparisons. It also compares the complete
native buffered and reader results, including metadata. It prints each filename
to stderr before decoding, so a failure identifies the input, and reports the
verified file count on success. A decode error or mismatch fails the command.
This mode returns before CPU affinity pinning, `PNG_BENCH_MS` parsing, timing, or
allocation counter calibration and measurement, including in an `allocations`
build.

`native` measures the public `decode` API, including its metadata recovery and
CRC/Adler validation of image data. `png` includes unpacking sub-byte samples and
conversion of big-endian 16-bit bytes to native `u16` storage. Its unpacker uses
constant-depth byte groups. `png-packed` reports the underlying packed-byte PNG
API separately. `image` measures `load_from_memory_with_format` without a further
RGBA conversion. Its palette/sub-byte expansion is part of that API.

## Equivalent RGBA8 asset output

Add `--rgba8` to measure three adapters with the same RGBA8 pixel output:

```powershell
$env:PNG_BENCH_MS = '80'
C:/GitHub/deadsync/target/png-bench/release/png-decode-bench.exe --rgba8 C:/GitHub/deadsync/target/png-formats C:/GitHub/deadsync/target/png-kodak
```

The adapter names in the TSV are `native-rgba8`, `png-assets-rgba8` and
`image-rgba8`. Without this flag, the original four decoder names and output
representations remain available. Both modes include a `pixels` column equal to
width times height, alongside the existing timing and allocation columns.

`native-rgba8` consumes the public decoder's owned samples directly. RGBA8
reuses its sample allocation; other formats expand straight into RGBA8, with no
RGBA16 intermediate. Palette alpha and sub-byte grayscale scaling are applied,
and missing palette entries become opaque black. Sixteen-bit transparency keys
are matched at full precision before retaining each channel's high byte.
This adapter represents an efficient consumer of the current native API; it
does not change production decoding or add a public conversion API.

`png-assets-rgba8` mirrors `deadlib-assets::decode_png_from_reader` with color
keying disabled: the same allocation limits, `STRIP_16`, indexed packed output,
non-indexed `EXPAND`, palette expansion, and direct RGBA8 buffer reuse. It stops
after `next_frame`, as the asset loader does. `image-rgba8` consumes the image
decoder's output and normalizes sixteen-bit channels by taking their high byte.
Calling `image.into_rgba8()` directly for those channels would instead round
their values and would not match the asset loader.

All adapters receive the same in-memory bytes. Filesystem access, hot-pink color
keying, resizing, and texture upload are outside the measurement. Native still
performs its complete PNG CRC, metadata, and stream validation; the asset
adapter does not call `reader.finish()` or parse everything after its first
frame. This difference in validation work is retained and must be considered
when interpreting the results. No native validation is disabled for timing.
The adapters agree on accepted corpus pixels, not on every resource-limit
boundary. Native uses its decoder limits; the asset adapter also enforces the
loader's final RGBA allocation limit. A native asset integration must retain that
consumer guard.

The existing raw-sample and RGBA16 comparisons run before either timing mode.
With `--rgba8`, all three RGBA8 outputs must also match exactly before timing.
The flag works with `--verify-only` and with the separate `allocations` build:

```powershell
target/png-bench/release/png-decode-bench.exe --verify-only --rgba8 assets courses crates/deadsync-song-lua/assets
target/png-alloc/release/png-decode-bench.exe --rgba8 target/png-formats target/png-kodak
```

## Decoder measurement procedure

The uninstrumented binary reports median wall time and Windows
`QueryThreadCycleTime` CPU cycles over five batches per file/codec. Each batch
lasts at least `PNG_BENCH_MS` milliseconds (default 20), in groups of eight
decodes. The Windows thread is pinned to logical CPU 0; decoder order rotates
between files. On other platforms the cycle column is zero; use an external
counter such as `perf stat` there. Repeat runs without concurrent builds or tests
and compare individual corpora as well as any aggregate.
Wall-time and cycle medians are selected independently, so scheduling delays in
one batch do not decide which CPU-cycle sample is reported.

For allocation measurements, use a separate instrumented binary:

```powershell
cargo build --release --locked --features allocations --bin png-decode-bench --target-dir C:/GitHub/deadsync/target/png-alloc
C:/GitHub/deadsync/target/png-alloc/release/png-decode-bench.exe C:/GitHub/deadsync/target/png-formats C:/GitHub/deadsync/target/png-kodak
```

Its calibrated allocator forwards every request to `System`. `alloc_calls` counts
allocations, zeroed allocations and reallocations. `requested_bytes` sums requested
sizes, including replacement sizes on reallocation; it is neither peak live
memory nor RSS. Instrumented timings are intentionally omitted. All unsafe code
in this workspace is limited to measurement FFI and allocator forwarding.
Production denies unsafe code outside the CPU dispatch modules, whose
instruction-set contracts are checked before entry. Buffer accesses remain checked.
Calibration exercises the hooks directly so compiler allocation elision cannot
remove a calibration request. Counts describe the instrumented executable;
compiler allocation elision can affect the compiled allocation behavior.

`python summarize.py <timing.tsv> <allocations.tsv> --output summary.json`
checks that every image has all decoder rows for its mode and reports each corpus
separately. Cycle ratios below 1 favor the native decoder. The JSON retains total
cycles, cycles per pixel, megapixels per second, geometric means of per-image
ratios, and the number of inputs where native wins. Totals represent one decode
of each input; geometric means give each input equal weight.

Add `--baseline <before.tsv>` to compare the native implementation before and
after a change. Both files must contain the same input paths and decoder names.
Compare timing and allocation files separately, using the corresponding baseline.

`cargo run --release --bin stages -- <file.png>` times native CRC, Adler,
inflation, unfiltering/sample conversion and the complete decode separately.
The stage modules compile the production sources directly; unfilter timing also
includes cloning the filtered input. Add `--profile` after the filename to run
500 complete public decodes for an external sampling profiler.

## Encoder workload

The `encode` binary measures encoding already-owned, opaque RGBA8 pixels.
Input PNG decoding, RGBA8 conversion, setting alpha to 255, generated pixels,
verification and encoded fingerprints are outside measurement. Each operation
returns and destroys a fresh PNG `Vec`; the native writer cases collect their
output into that same owned representation. Filtering, compression, PNG
framing/checksums and finalization are included. Filesystem writes, screen
capture, resizing and preview generation are outside this workload.

The modes are:

| Mode | Compression | Filter | API |
| --- | --- | --- | --- |
| `native-buffered`, `native-writer` | Native `Fast` | Adaptive | `encode`, `encode_writer` |
| `native-rle-buffered`, `native-rle-writer` | Native `Rle` | Adaptive | `encode`, `encode_writer` |
| `image-default` | png Fast | Adaptive | `image::codecs::png::PngEncoder::new` |
| `png-fast-adaptive` | png Fast | Adaptive | `png::Encoder` |
| `png-balanced-adaptive` | png Balanced | Adaptive | `png::Encoder` |
| `native-rle-up-buffered`, `native-rle-up-writer` | Native `Rle` | Up | `encode`, `encode_writer` |
| `png-fast-up` | png Fast | Up | `png::Encoder` |

All ten modes run by default. `--modes NAME,...` selects a subset. The Up modes
are explicit filter controls; the native default remains Fast/Adaptive.
`image-default` uses the encoder selected by the pinned image version's PNG
save path, with its filesystem, buffered writer and format dispatch excluded.
Cargo.lock pins image 0.25.10, png 0.18.1 and their dependencies.

Files and directories are accepted recursively. Without paths, the binary
generates flat, gradient, UI and deterministic noise images at 640×360,
1920×1080 and 3840×2160. `--generated` adds those cases to supplied paths.
`--sizes WIDTHxHEIGHT,...` replaces the generated sizes and implies
`--generated`; the recorded corpus adds 17×13 boundary cases to the three
default sizes. `--filter TEXT` selects a case-sensitive substring of the
normalized input name.

Run these examples from this directory:

```powershell
cargo build --release --locked --bin encode --target-dir C:/GitHub/deadsync/target/png-encode-bench
$env:PNG_BENCH_MS = '80'
C:/GitHub/deadsync/target/png-encode-bench/release/encode.exe --sizes 17x13,640x360,1920x1080,3840x2160 > C:/GitHub/deadsync/target/encode-cycles.tsv
C:/GitHub/deadsync/target/png-encode-bench/release/encode.exe --verify-only --sizes 17x13,1920x1080 C:/GitHub/deadsync/save/screenshots
```

Before timing any selected mode for an input, the harness decodes each encoded
PNG with both Rust png and image and requires exact dimensions, RGBA8 format
and pixel values. Rust png also validates the stream through `finish()`.
`--verify-only` performs these checks and emits encoded sizes/fingerprints with
zero timing and allocation columns; it skips CPU affinity, warmup, allocation
calibration and measurement. It still parses `PNG_BENCH_MS`, which must be
positive if set. Failures abort the command and are never omitted from results.

## Encoder measurement procedure

The uninstrumented encoder reports independently selected median wall time and
Windows `QueryThreadCycleTime` cycles from five batches per input/mode. Each
batch lasts at least `PNG_BENCH_MS` milliseconds (default 20). Images below one
million pixels run in groups of eight; larger images run one encoding at a time.
The Windows thread is pinned to logical CPU 0, and mode order rotates between
inputs. Outside Windows, thread cycles are zero and require an external counter.
Run measurements without concurrent builds, tests or other timing jobs.

Keep an immutable before executable and its TSV as the baseline for each change.
Record its source commit/diff, compiler, build flags, executable identity and
input hashes. Build before and after with the same locked dependencies and
release profile, then use the same inputs, selected modes and batch duration.
The retained evidence and baseline identities are linked from
[ENCODE_RESULTS.md](ENCODE_RESULTS.md).

Measure allocations with a separate executable and the same input selection:

```powershell
cargo build --release --locked --features allocations --bin encode --target-dir C:/GitHub/deadsync/target/png-encode-alloc
C:/GitHub/deadsync/target/png-encode-alloc/release/encode.exe --sizes 17x13,640x360,1920x1080,3840x2160 > C:/GitHub/deadsync/target/encode-allocations.tsv
```

This build calibrates the allocator, measures one complete encoding per
input/mode and emits zero timing columns. `alloc_calls` counts allocation,
zeroed-allocation and reallocation requests. `requested_bytes` sums their
requested sizes, including replacement sizes; it does not report peak live
memory or RSS. Input preparation and verification remain outside the counters.

`compare_encode.py` summarizes timing and allocation TSVs separately. Supply
the persistent corpus manifest explicitly:

```powershell
python compare_encode.py C:/GitHub/deadsync/target/encode-cycles.tsv --corpus results/2026-11-20-encode/corpus.json --output encode-summary.json
python compare_encode.py before-cycles.tsv after-cycles.tsv --corpus results/2026-11-20-encode/corpus.json --modes native-buffered,native-writer,image-default,png-fast-adaptive,png-balanced-adaptive --output encode-changes.json
python compare_encode.py before-allocations.tsv after-allocations.tsv --corpus results/2026-11-20-encode/corpus.json --modes native-buffered,native-writer,image-default,png-fast-adaptive,png-balanced-adaptive --output encode-allocation-changes.json
```

The second and third commands select the modes shared with the original
baseline; change or omit `--modes` when both files contain another matching
selection. The manifest groups screenshots and artwork, and generated input
names form their own group. Add manifest entries when measuring other files.
The script accepts UTF-8 and PowerShell UTF-16 TSVs and requires complete
input/mode matrices. Before/after comparisons also require matching dimensions
and encoded sizes/FNV-1a fingerprints by default. These fingerprints detect
output drift in measured cases; they are not cryptographic hashes. Use
`--allow-output-drift` only for an intentional encoded-output change; the JSON
then lists each changed identity.

Reports retain per-group cycle, wall-time, encoded-size and allocation ratios,
aggregate throughput, win counts and every regression. Native/reference and
after/before ratios below one favor native or the changed implementation,
respectively. Compare encoded size alongside cycles and throughput.

`cargo run --release --locked --bin encode_stages -- <file.png>` is a diagnostic
breakdown of native encoding, filtering, compression and checksums. Its stages
share source with production; their separate measurements are not additive and
do not replace the public-API comparisons.
