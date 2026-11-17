# GIF decoder measurements

This development-only workspace compares `deadlib-image-gif`, `gif 0.14.2`,
`gif-dispose 6.0.0`, and `image 0.25.10`. It adds no production dependencies.
The lock file pins the reference implementations and their dependencies.

Run from the repository root:

```powershell
cargo build --manifest-path crates/deadlib-image/gif/bench/Cargo.toml --release --locked --target-dir target/gif-decode-bench
$env:GIF_BENCH_MS = '80'
target/gif-decode-bench/release/gif-decode-bench.exe assets crates/deadlib-image/gif/tests/fixtures/image-gif/samples
```

Arguments accept GIF files or directories, searched recursively. Before measuring
each input, the harness compares indexed frames, palettes, controls, and complete
animation canvases against the reference decoders. Inputs rejected by the native
strict decoder, inputs without image frames, and non-image graphics requiring a
caller-supplied renderer are reported as `EXCLUDED` on stderr. Reference decode
failures or output differences abort the run. Successful verification reports
the file, frame count, and total decoded frame pixels on stderr.
An absent graphic control extension uses each API's default disposal: native
preserves its absence, while `gif` returns `Keep`. Explicit disposal values are
compared exactly, and composited pixels are compared in either case.

The seven modes decode every frame, including output destruction:

| Mode | Output and ownership |
| --- | --- |
| `native-retained` | Native whole-file decode, retaining frames and extension metadata. |
| `gif-retained` | Indexed `gif` decode, cloning frames and the global palette into owned storage. |
| `native-stream` | Native frame events, recycling frame buffers after consumption. |
| `gif-stream` | Indexed `gif` frames, consuming borrowed buffers before the next frame. |
| `native-rgba` | Native streaming decode and transparent-background compositing into a reusable canvas. |
| `gif-rgba` | Indexed `gif` decode and `gif-dispose` compositing into a reusable canvas. |
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
target/gif-alloc-bench/release/gif-decode-bench.exe assets crates/deadlib-image/gif/tests/fixtures/image-gif/samples
```

Its calibrated allocator forwards requests to `System`. `alloc_calls` counts
allocations, zeroed allocations, and reallocations. `requested_bytes` sums
requested sizes, including replacement sizes on reallocation; it measures
neither peak live memory nor RSS. Timing columns are zero in this build. The
measurement-only unsafe code handles Windows FFI and allocator forwarding;
production GIF decoding remains safe Rust.

This harness establishes a comparison workflow. No performance result or
optimization claim is recorded yet.
