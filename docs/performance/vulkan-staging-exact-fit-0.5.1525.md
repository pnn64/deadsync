# Vulkan staging-buffer exact-fit shortcut

Baseline: `22ce642b5`.

`best_fit_staging_index` now returns immediately when the pool's first buffer
exactly matches the upload size. No later buffer can be a better fit, and the
old search selects the first entry on ties. All other requests use the existing
best-fit search. The production change adds one guard; it adds no cache, storage,
allocation, or GPU command. This applies to texture uploads, including video
frames, when retired staging buffers are reused.

## Behavior

The test harness calls the production functions and retains the previous search
as a reference. It compares exact selected indices for 156,248 combinations:
pool lengths 0–6, capacities including zero and `u64::MAX`, exact and oversized
fits, misses, and duplicate sizes. Another test compares 1,024 remove/reinsert
operations, including pool order, byte accounting, and zero allocation churn.
The same ten unit tests passed before and after the change in release mode.
All ten also passed in debug mode after the change. Clippy passed with
performance warnings denied; existing non-performance warnings remain.

```powershell
cargo test --locked --release -p deadlib-render-backend-vulkan --lib
cargo test --locked -p deadlib-render-backend-vulkan --lib
cargo clippy --locked -p deadlib-render-backend-vulkan --lib -- -D clippy::perf
```

The graphics-device test remains ignored. No rendered-pixel or GPU-time
measurement was made: the regression checks that the same staging allocation
is selected, while upload contents and GPU command recording are unchanged.

## CPU measurements

Windows x86-64, Intel Xeon E5-2696 v4, Rust 1.98.1 / LLVM 22.1.8. Repository
release profile with full LTO. Three runs on logical CPU 6, alternating which
variant executes first. Each measurement has seven batches of 262,144
operations; input/output goes through `black_box`. Allocation counting is
separate from timing. No builds or other tests from this pass ran concurrently.

The reuse fixtures include selecting a buffer, removing it with `swap_remove`,
updating the byte count, and returning it to the pool. They use inert Vulkan
handles and do not upload pixels. The video fixture reuses three equally sized
3,110,400-byte buffers (the payload size of a 1920×1080 YUV420 frame). The mixed
fixture alternates 4/8/16 KiB requests among six buffers. The oversized fixture
requests 4 KiB from a full 32-entry pool of larger buffers.

| Reuse fixture | Before ns/op | After ns/op | Median paired time reduction | Paired range |
| --- | ---: | ---: | ---: | ---: |
| Video-sized buffers | 11.7 | 7.1 | 36.6% | 33.3% to 44.1% |
| Mixed sizes | 11.8 | 9.2 | 21.6% | 8.5% to 24.6% |
| All oversized | 31.6 | 32.6 | -3.0% | -3.2% to 1.3% |
| No fitting buffer | 9.0 | 9.6 | -3.3% | -6.7% to 7.3% |

Times are medians of run medians; reductions are medians of paired reductions.
For video/mixed reuse, calling-thread cycle reductions were 36.7%/19.7% and
throughput increased 57.6%/26.9%. Allocation, reallocation, and free counts were
zero for every fixture, before and after.

The isolated search with three equal buffers fell from 5.8 to 1.6 ns; with 32
equal buffers it fell from 27.8 to 1.4 ns. Full-search cases gain nothing from
the shortcut and can pay a small extra comparison cost. The reuse benchmarks
show about 1 ns extra for the oversized fixture. These are small CPU savings
per upload, not a measured improvement in frame rate or complete video upload
time. All fixtures and timing ranges are retained in the
[raw results](vulkan-staging-exact-fit-0.5.1525.csv).

Reproduce after compilation finishes:

```powershell
cargo test --locked --release -p deadlib-render-backend-vulkan --lib --no-run
(Get-Process -Id $PID).ProcessorAffinity = 64
cargo test --locked --release -p deadlib-render-backend-vulkan --lib staging_tests:: -- --ignored --nocapture --test-threads=1
# Set DEADSYNC_PERF_REVERSE=1 to run the changed functions first.
```
