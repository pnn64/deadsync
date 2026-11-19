# GIF decoder measurements

This development-only workspace compares `deadlib-image-gif`, `gif 0.14.2`,
`gif-dispose 6.0.0`, and `image 0.25.10`. It adds no production dependencies.
The lock file pins the reference implementations and their dependencies.

Retained measurements, scope, and known exceptions are in [RESULTS.md](RESULTS.md).

Run from the repository root:

```powershell
cargo run --manifest-path crates/deadlib-image/gif/bench/Cargo.toml --release --locked --bin generate -- target/gif-generated-20261117
cargo build --manifest-path crates/deadlib-image/gif/bench/Cargo.toml --release --locked --target-dir target/gif-decode-bench
$env:GIF_BENCH_MS = '80'
target/gif-decode-bench/release/gif-decode-bench.exe assets crates/deadlib-image/gif/tests/fixtures/image-gif/samples target/gif-generated-20261117
```

`generate` writes 128 reproducible inputs using both the native and Rust `gif`
encoders. They cover all power-of-two palette sizes, tiny images, flat colors,
bands, random data, interlacing, one- and seven-byte image sub-blocks, local
palettes, transparent frames, rectangles, and all defined disposal methods.
Every generated file is decoded and checked against its intended indexed frames.

Arguments accept GIF files or directories, searched recursively and deduplicated.
Default native decoding remains strict. Add `--compatibility` to use
`DecodeOptions { strict: false, .. }` for verification and all three native
measurement modes. This accepts the documented compatibility cases without
changing strict benchmark comparisons or the archived measurements.

For a correctness-only corpus check, use:

```powershell
cargo run --release --locked --manifest-path crates/deadlib-image/gif/bench/Cargo.toml --bin gif-decode-bench -- --verify-only --compatibility assets crates/deadlib-image/gif/tests/fixtures/image-gif/samples
```

`--verify-only` checks every indexed frame, palette, rectangle, control, rendered
RGBA canvas and delay against the reference decoders, then exercises both native
streaming modes with frame-buffer recycling. It reports the verified file and
frame counts without timings or allocator measurements. Every rejection,
unsupported case or comparison failure aborts verification; no inputs are
silently excluded. Both native streaming modes submit trailing bytes after the
End event so they are charged to the input budget in compatibility mode and
rejected in strict mode.

Before measuring each input, the harness compares indexed frames, palettes,
controls, and complete animation canvases against the reference decoders. Inputs
rejected by the native decoder under the selected policy, inputs without image
frames, and non-image graphics requiring a caller-supplied renderer are reported
as `EXCLUDED` on stderr. Reference decode failures or output differences abort
the run. Successful verification reports
the file, frame count, and total decoded frame pixels on stderr.
An absent graphic control extension uses each API's default disposal: native
preserves its absence, while `gif` returns `Keep`. Explicit disposal values are
compared exactly, and composited pixels are compared in either case.

The seven modes decode every frame, including output destruction:

| Mode | Output and ownership |
| --- | --- |
| `native-retained` | Native whole-file decode, retaining frames and extension metadata. |
| `gif-retained` | Indexed `gif` iterator, moving owned frames into retained storage and copying the global palette. |
| `native-stream` | Native frame events, recycling frame buffers after consumption. |
| `gif-stream` | Indexed `gif` decode through `read_into_buffer`, reusing caller-owned pixel storage. |
| `native-rgba` | Native streaming decode and transparent-background compositing into a reusable canvas. |
| `gif-rgba` | Indexed `gif` decode through `read_into_buffer` and `gif-dispose` compositing, reusing pixel and canvas buffers. |
| `image-rgba` | `image` animation iterator, consuming each owned RGBA frame. |

Retention and output ownership differ between these APIs. In particular, the
native decoder retains extension metadata that the reference frame loop does
not retain, and `image` returns owned frames while the compositor modes expose
borrowed canvases. Compare the indexed and composited groups separately.

The uninstrumented binary reports TSV rows containing median wall time and
Windows `QueryThreadCycleTime` CPU cycles over five batches per file and mode.
Each batch lasts at least `GIF_BENCH_MS` milliseconds (default 20), in groups of
eight decodes, or one decode for inputs exceeding four million frame pixels.
The Windows thread is pinned to logical CPU 0, and mode order rotates between
files. Wall-time and cycle medians are selected independently. Other platforms
report zero in the cycle column. Repeat measurements without concurrent builds
or tests before drawing performance conclusions.

Build a separate binary for allocation measurements:

```powershell
cargo build --manifest-path crates/deadlib-image/gif/bench/Cargo.toml --release --locked --features allocations --target-dir target/gif-alloc-bench
target/gif-alloc-bench/release/gif-decode-bench.exe assets crates/deadlib-image/gif/tests/fixtures/image-gif/samples target/gif-generated-20261117
```

Its calibrated allocator forwards requests to `System`. `alloc_calls` counts
allocations, zeroed allocations, and reallocations. `requested_bytes` sums
requested sizes, including replacement sizes on reallocation; it measures
neither peak live memory nor RSS. Timing columns are zero in this build. The
measurement-only unsafe code handles Windows FFI and allocator forwarding;
production GIF decoding remains safe Rust.

`python crates/deadlib-image/gif/bench/summarize.py timings.tsv allocations.tsv --output summary.json`
requires all seven modes and exactly matched input sets. It reports geometric
mean native/reference ratios, totals, and every individual loss, both overall
and separately for repository assets, upstream fixtures, and generated images.
