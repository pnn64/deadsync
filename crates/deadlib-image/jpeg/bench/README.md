# JPEG decoder measurements (in progress)

This isolated development workspace compares the native codec with `zune-jpeg
0.5.15`, `jpeg-decoder 0.3.2` (default features), and `image 0.25.10`. It adds no
production dependencies. The performance goal remains open.

`native` now measures the public pixel-output `decode` API. Sequential 8-bit
single-scan inputs use bounded MCU-row storage; other modes preserve the native
reconstruction path. Pixel snapshots verify equivalence with the original
coefficient/plane/conversion pipeline.

Run from the repository root:

```powershell
cargo build --manifest-path crates/deadlib-image/jpeg/bench/Cargo.toml --release --locked --bin jpeg-decode-bench --target-dir target/jpeg-decode-bench
$env:JPEG_BENCH_MS = '40'
$env:RAYON_NUM_THREADS = '1'
target/jpeg-decode-bench/release/jpeg-decode-bench.exe assets crates/deadlib-image/jpeg/tests/fixtures/sequential crates/deadlib-image/jpeg/tests/fixtures/progressive
```

The Windows process is pinned to logical CPU 0. `process_cycles` uses
`QueryProcessCycleTime`, including reference worker threads. Disabling Rayon in
`jpeg-decoder` still leaves its alternate worker implementation; measuring only
the calling thread would omit work. The current benchmark enables the default
Rayon implementation with one worker. `zune-jpeg` retains its default SIMD
features. Other platforms report zero cycles. Wall-time and CPU-cycle medians
are selected independently across five batches per file/mode. Each batch lasts
at least `JPEG_BENCH_MS` milliseconds (default 20), in groups of eight decodes
or one for images over four million pixels. Destruction is included.

Current output adapters cover 8-bit gray/RGB. CMYK output from `jpeg-decoder`
is converted to RGB with an explicit subtractive adapter. Higher-precision
adapters and a broader independent image corpus remain work in progress.
Unsupported reference modes are logged individually as `UNAVAILABLE`; other
available modes still run. Higher-precision inputs are currently reported as
`EXCLUDED` while that output adapter is pending. No timing threshold filters
the input set.

`PIXELS` records maximum sample differences and RMSE against native output.
JPEG implementations can differ in legal IDCT and interpolation rounding; these
diagnostics do not establish an accuracy tolerance. The native tests retain
independent coefficient, sample, mathematical-transform, ITU T.83, and C-oracle
fixtures. Optimized output is additionally compared byte for byte with snapshots
from the original native pipeline:

```powershell
$env:JPEG_SNAPSHOT_DIR = 'target/jpeg-original-pixels'
# Run an original-source binary with the exact corpus arguments above.
# For the optimized binary, unset JPEG_SNAPSHOT_DIR and set:
$env:JPEG_BASELINE_DIR = 'target/jpeg-original-pixels'
```

Use the identical sorted input set for capture and replay; snapshots are indexed
by file ordinal. This verifies unchanged native pixels independently of the
rounding differences between reference implementations.

Build allocation measurements separately:

```powershell
cargo build --manifest-path crates/deadlib-image/jpeg/bench/Cargo.toml --release --locked --features allocations --bin jpeg-decode-bench --target-dir target/jpeg-alloc-bench
$env:RAYON_NUM_THREADS = '1'
target/jpeg-alloc-bench/release/jpeg-decode-bench.exe assets crates/deadlib-image/jpeg/tests/fixtures/sequential crates/deadlib-image/jpeg/tests/fixtures/progressive
```

The calibrated allocator forwards to `System`. Calls include allocation,
zeroed allocation, and reallocation; requested bytes include replacement sizes
on reallocation and measure neither peak live memory nor RSS. Timings are zero
in the instrumented binary. Every mode receives an untimed warmup, so the
reference global worker pool is initialized before per-decode measurement.

`cargo run --manifest-path crates/deadlib-image/jpeg/bench/Cargo.toml --release --bin stages -- <file.jpg>`
profiles coefficient decoding, reconstruction, color conversion, and the full
native pipeline separately. On Windows it also pins the process and reports
process-wide cycles. All benchmark unsafe code is measurement FFI or allocator
forwarding; production still forbids unsafe code.
