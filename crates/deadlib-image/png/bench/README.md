# Decoder measurements

This development-only workspace compares `deadlib-image-png`, `png 0.18.1` and
`image 0.25.10`. It does not add production dependencies. Build in release mode;
the lock file pins the reference implementations and their dependencies.

Retained measurements and remaining performance gaps are in [RESULTS.md](RESULTS.md).

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
checks that every image has all four decoder rows and reports each corpus
separately. Cycle ratios below 1 favor the native decoder. The JSON retains total
cycles as well as geometric means of per-image ratios.

`cargo run --release --bin stages -- <file.png>` times native CRC, Adler,
inflation, unfiltering/sample conversion and the complete decode separately.
The stage modules compile the production sources directly; unfilter timing also
includes cloning the filtered input. Add `--profile` after the filename to run
500 complete public decodes for an external sampling profiler.
