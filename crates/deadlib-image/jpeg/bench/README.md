# JPEG decoder measurements

This isolated workspace compares the native codec with `zune-jpeg 0.5.15`,
`jpeg-decoder 0.3.2` (default features), and `image 0.25.10`. It adds no
production dependencies. Current numbers are in [RESULTS.md](RESULTS.md).

The [105-input qualification archive](results/windows-x86_64-20261119-rgba8/README.md)
pins the corpus, encoder, build identities and before/after measurements.

The default measures native gray/RGB output with the strict input policy.
`--rgba8` measures the proposed asset-loading contract: native
`decode_with_options` with `strict: false`, centered linear interpolation and
direct gray/RGB-to-RGBA8 conversion. The `image` adapter calls its own
`DynamicImage::into_rgba8`, matching the loader's decode and conversion work.
File I/O, signature detection, format retries and color keying are excluded
for all codecs. Every 8-bit DCT process
reconstructs into a bounded MCU-row window; sequential single-scan images decode
straight into it. `PIXELS` diagnostics compare samples with each reference;
snapshot replay (below) compares two native builds byte for byte.

Run from the repository root:

```powershell
cargo build --manifest-path crates/deadlib-image/jpeg/bench/Cargo.toml --release --locked --bin jpeg-decode-bench --target-dir target/jpeg-decode-bench
$env:JPEG_BENCH_MS = '40'
$env:RAYON_NUM_THREADS = '1'
target/jpeg-decode-bench/release/jpeg-decode-bench.exe --rgba8 assets crates/deadlib-image/jpeg/tests/fixtures/sequential crates/deadlib-image/jpeg/tests/fixtures/progressive
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

Pass files or directories, or repeat `--files-from PATH` for UTF-8 lists of paths
relative to the working directory. Inputs are sorted and deduplicated.
`--verify-only` runs untimed acceptance, output-length and reference-difference
checks. `--native-only` times only native after those checks, for individual
optimization comparisons. Neither flag changes the decoder adapter.

Output adapters cover 8-bit gray/RGB and RGBA8. CMYK output from `jpeg-decoder`
is converted to RGB with an explicit subtractive adapter. Higher-precision
inputs belong to the exact native behavior checks below.
The direct `zune-jpeg` adapter requests Luma/RGB and then uses the shared RGBA
conversion; it does not measure zune's direct RGBA output option. The `image`
adapter is the comparison with the current application loading path.
Unsupported reference modes are logged individually as `UNAVAILABLE`; other
available modes still run. Higher-precision inputs are currently reported as
`EXCLUDED` while that output adapter is pending. No timing threshold filters
the input set.

`PIXELS` records maximum sample differences and RMSE against native output.
RGBA8 RMSE includes the constant alpha channel; it is not RGB-only RMSE.
JPEG implementations can differ in legal IDCT and interpolation rounding; these
diagnostics do not establish an accuracy tolerance. The native tests retain
independent coefficient, sample, mathematical-transform, ITU T.83, and C-oracle
fixtures. The separate `native_snapshot` binary captures and verifies original
native results, including input identity, pixels, retained metadata and exact
errors under both input policies and interpolation modes:

```powershell
cargo build --manifest-path crates/deadlib-image/jpeg/bench/Cargo.toml --release --locked --bin native_snapshot --target-dir target/jpeg-decode-bench
# See native_snapshot --help for capture and verify arguments.
```

Use the identical input set and original-source binary for capture. Verification
compares serialized bytes and rejects changed inputs or result fields; reference
rounding differences do not affect this check.
Capture requires a new destination and records a completion manifest. The helper
checks default resource limits; tighter limits and stateful abbreviated streams
remain covered by the crate's integration tests. The snapshot cap limits stored
bytes, not decoder resource limits.

To compare code paths with AVX2 disabled on an AVX2 machine, build with
`python crates/deadlib-image/jpeg/bench/no_avx2.py` and run the printed
directory's `jpeg-decode-bench.exe` with the same arguments. It compiles native
dispatch out and patches a throwaway zune-core copy so `use_avx2()` is false.

Build allocation measurements separately:

```powershell
cargo build --manifest-path crates/deadlib-image/jpeg/bench/Cargo.toml --release --locked --features allocations --bin jpeg-decode-bench --target-dir target/jpeg-alloc-bench
$env:RAYON_NUM_THREADS = '1'
target/jpeg-alloc-bench/release/jpeg-decode-bench.exe --rgba8 assets crates/deadlib-image/jpeg/tests/fixtures/sequential crates/deadlib-image/jpeg/tests/fixtures/progressive
```

The calibrated allocator forwards to `System`. Calls include allocation,
zeroed allocation, and reallocation; requested bytes include replacement sizes
on reallocation and measure neither peak live memory nor RSS. Timings are zero
in the instrumented binary. Every mode receives an untimed warmup, so the
reference global worker pool is initialized before per-decode measurement.

`cargo run --manifest-path crates/deadlib-image/jpeg/bench/Cargo.toml --release --bin stages -- <file.jpg>`
profiles coefficient decoding, reconstruction, color conversion, the pixel API
and zune-jpeg separately. `stages <file.jpg> N` repeats only the pixel API and
`stages <file.jpg> N zune` only zune-jpeg, for sampling profilers. On Windows it also pins the process and reports
process-wide cycles. All benchmark unsafe code is measurement FFI or allocator
forwarding; production denies unsafe outside its existing SIMD dispatch module.
