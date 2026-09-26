# Borrow OpenGL camera uniform data

Baseline: `73cf2f40c`.

The main OpenGL draw paths copied each selected camera matrix, converted it to
a temporary column array, and passed that array to the uniform upload. Both
modern and legacy paths now use one small inline selector returning the
matrix's existing column-major slice. This removes six repeated selection/copy
blocks. Missing camera indices still select the
same default projection. Upload locations, transpose flags, and camera-change
tracking are unchanged.

No cache or new storage was added. OpenGL consumes the borrowed data during the
uniform call; the frame and default projection remain alive throughout it.

## Behavior validation

The new unit test checks all 256 camera indices against camera counts 0, 1, 3,
8, 256, and 257, with four fallback matrices. All **6,144 selections** preserve
the expected 16 column values bit-for-bit and point at the selected matrix's
existing storage. Input matrices contain deterministic arbitrary float bits;
fallbacks explicitly cover negative zero, infinities, and a NaN payload.

The existing hidden-window OpenGL pixel test ran against the baseline and the
changed production code. Each run passed **144 full-image comparisons**, covering
interleaved sprites, meshes, and textured meshes; retained/transient geometry;
alpha/additive blending; depth testing; glow; offscreen RGB/RGBA targets; and
base-instance support enabled/disabled. The aggregate before/after pixel
snapshot hashes also match: FNV-1a `d213ec78b2f2b259`.

All eight ordinary crate tests pass in release and debug. Clippy passes with
performance lints denied; existing unrelated warnings remain. Pixel validation
used modern desktop OpenGL on the GPU below; legacy and GLES paths were not
exercised at runtime.

```powershell
cargo test --locked --release -p deadlib-render-backend-gl
cargo test --locked -p deadlib-render-backend-gl
cargo clippy --locked -p deadlib-render-backend-gl --lib -- -D clippy::perf
cargo test --locked --release -p deadlib-render-backend-gl interleaved_vertex_buffers_preserve_pixels -- --ignored --nocapture --test-threads=1
```

## Before/after benchmark

Windows x86-64, Intel Xeon E5-2696 v4, Rust 1.98.1 / LLVM 22.1.8, repository
release profile with LTO. Driver: NVIDIA GeForce GTX 1650 SUPER, OpenGL 4.6.0,
NVIDIA 591.86. Three paired runs pinned to logical CPU 6, reversing variant order
in run 2. No builds or other tests from this pass ran during measurement.

Both variants execute actual `glUniformMatrix4fv` calls through glow, using the
same live context, linked program, uniform location, input matrices, and camera
indices. The baseline freezes the previous selection/copy/upload sequence;
the changed variant calls the production selector. Both use the same
black-boxed function-pointer dispatch. No draw or present is submitted inside
the benchmark.

An operation uploads 256 matrices, cycling indices 0 through 7. The valid case
has eight distinct stored cameras; fallback has none; mixed has four. Each
variant warms up for three operations, then measures seven batches of 8,192
operations. Cycle counts use `QueryThreadCycleTime`. Timing includes loop,
dispatch, driver-call, and clock overhead equally in both variants.

The table reports medians of paired run ratios. Positive cycle reductions mean
less calling-thread CPU work. All measurements, including the slower valid
case in run 3, are retained in the [raw CSV](gl-camera-uniforms-0.5.1525.csv).

| Camera selection | Cycle reduction | Paired range | Throughput increase |
| --- | ---: | ---: | ---: |
| Valid camera | 5.0% | -1.2% to 5.0% | 5.2% |
| Default projection | 6.0% | 4.7% to 12.0% | 6.3% |
| Mixed valid/default | 1.7% | 0.3% to 4.1% | 1.8% |

Median paired elapsed savings are approximately 1.7 ns per valid upload,
1.0 ns per fallback upload, and 0.5 ns per mixed upload. Valid-camera results
vary between runs; this is a small optimization, not evidence of a measurable
frame-rate change. Existing camera tracking already skips unchanged uploads,
so actual frame savings depend on camera/program changes. Other drivers may
produce different results.

Both variants retain zero Rust allocations, reallocations, and frees in every
measured operation. Allocation counting runs separately from timing and excludes
context/fixture setup. It does not count allocations internal to the native
driver. CSV times, cycles, and allocation counts are per 256-upload operation;
throughput units are uploads per second. These are CPU submission measurements,
not GPU execution timings.

## Reproduce

Build once, then run the printed test executable directly to avoid overlapping
build work with measurements:

```powershell
cargo test --locked --release -p deadlib-render-backend-gl --no-run
(Get-Process -Id $PID).ProcessorAffinity = 64
# <executable> benchmark_camera_uniform_upload --ignored --nocapture --test-threads=1
# Repeat three times; set DEADSYNC_PERF_REVERSE=1 for the middle run.
```

Capture stderr directly to a file. Raw benchmark logs and before/after pixel
test logs are retained locally under `target/gl-projection-pass`.
