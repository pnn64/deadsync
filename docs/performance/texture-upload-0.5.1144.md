# Software texture-upload performance, 0.5.1144

Baseline: `0dfc476f4` / 0.5.1143. This pass bumps the workspace patch version exactly once to 0.5.1144, including the matching Cargo.lock entries.

## Three changes

The local `rust-performance.md` guidance M-HOTPATH, M-MEM-REUSE, and M-THROUGHPUT motivates measuring per-pixel work, retaining existing frame storage, and batching adjacent pixels in the software renderer.

1. **Reuse the RGBA image for same-size video updates.** Previously, every successful YUV420 update allocated and zeroed a new RGBA image before dropping the previous frame. Updates now fill existing storage when dimensions and raw byte length match. A 1920x1080 update removes one 8,294,400-byte allocation and the corresponding free. Initial creation and resizing still allocate the required image. Validation precedes mutation; invalid uploads leave pixels, storage, and texture flags unchanged. Images with trailing raw storage retain the previous replacement behavior.
2. **Normalize shared chroma once per 2x2 block.** Four adjacent YUV420 pixels share U/V samples. The writer now normalizes each pair once instead of four times. Per-pixel luma, fused multiply-add nesting, clamping, and rounding remain in the original order. Both creation and updates use this writer. The creation comparison keeps allocation counts identical, measuring the conversion change independently of buffer reuse.
3. **Check alpha in groups of four pixels.** Opacity detection now uses safe native-endian 128-bit loads and a matching alpha-byte mask for each 16-byte group. Remaining complete pixels use the scalar check. Empty dimensions, trailing storage, and partial trailing pixels preserve the old behavior. This reduces per-pixel work for opaque textures and late transparency without adding allocations.

The helpers live in a private module shared by the software backend and its comparison tests. Public signatures, sampler handling, and dependencies are unchanged. These changes apply to CPU software-renderer uploads; GPU backends are outside this pass.

## Validation

The frozen baseline copies the two previous helper bodies from `0dfc476f4`, with visibility and formatting changes only. Tests include calls through the actual public texture API as well as the private production module, compiled unchanged into the integration test for exact helper comparisons.

Six regression/allocation tests cover:

- All 65,536 U/V byte pairs with four varying luma samples each, using three color-conversion coefficient/level presets and byte-for-byte old/new image comparisons.
- Five small rectangular shapes with 64 generated coefficient/level cases each, including NaN, infinities, signed zero, and a subnormal.
- Eight consecutive updates retaining the same pointer, RGBA/YUV format transitions, resizing and transposed dimensions, and images with excess raw storage.
- Zero/odd dimensions, short Y/U/V planes, and huge invalid dimensions, checking error strings and preservation of prior pixels, pointer, and format.
- Every alpha byte at selected four-pixel group/tail boundaries, zero-sized images, and extra raw bytes.
- Zero heap churn through the public same-size update API and the opacity helper, plus a one-allocation/exact-frame-size upper bound for public creation.

Results: **6 tests passed in debug and 6 in release**; the manual benchmark is ignored in ordinary runs. All **14 existing software-renderer library tests passed in release**, including texture opacity and raster-output tests. `cargo check --workspace --bins` and Clippy with `-D clippy::perf` passed; existing non-performance warnings remain. Targeted Rust formatting and `git diff --check` passed.

## Measurement

Windows x64, Intel Xeon E5-2696 v4 @ 2.20 GHz; rustc 1.98.0 (`88d9e12ae`), LLVM 22.1.8. Release profile: opt-level 3, LTO. Old and new execute in the same binary through opaque function pointers with input/output black boxes. Calling-thread cycles come from `QueryThreadCycleTime`.

The tables use medians from three final runs after builds/checks finished, reversing old/new order in the middle run. Each run uses seven timed batches after three warm-up operations. Conversion/update batch iterations are `(1_000_000 / pixels).clamp(2, 2000)`; opacity uses `(4_000_000 / pixels).clamp(8, 20000)`. Inputs are prepared outside timing. Y samples vary deterministically; U/V sample pairs cycle through byte values. Opacity images are fully opaque except for the specified early/late transparent pixel.

Creation measures conversion plus allocation and destruction of the returned image. Updates measure replacement of an existing image with the frozen conversion versus the production in-place update helper; the existing image is created outside timing. The public wrapper's two format/opacity flag writes are outside this helper comparison. Public API behavior and its zero-allocation update are separately tested. Update gains combine the conversion and storage changes and are not additive with creation gains.

Allocation counts come from a separate operation with thread-local counters. Tracking is disabled during timing, but the same `System` allocator wrapper remains installed for both implementations. Throughput counts output pixels per second for conversion/updates and pixels through the first transparent pixel for opacity. The early-transparency workload therefore counts one useful pixel per operation even though the grouped implementation loads four alpha bytes.

| Workload | ns/op old -> new | Thread cycles/op old -> new | Cycles saved | Million pixels/s old -> new |
|---|---:|---:|---:|---:|
| Create 2x2 (tiny control) | 239.4 -> 265.4 | 524.4 -> 583.3 | -11.2% | 16.705 -> 15.074 |
| Update 2x2 | 222.1 -> 115.8 | 488.3 -> 254.9 | 47.8% | 18.010 -> 34.542 |
| Create 128x72 | 296,571.3 -> 266,146.3 | 649,915.1 -> 583,205.4 | 10.3% | 31.075 -> 34.628 |
| Update 128x72 | 295,724.1 -> 259,922.2 | 647,978.2 -> 569,830.1 | 12.1% | 31.164 -> 35.457 |
| Create 640x360 | 7,369,375.0 -> 6,555,450.0 | 16,144,828.2 -> 14,366,769.0 | 11.0% | 31.265 -> 35.146 |
| Update 640x360 | 7,210,850.0 -> 6,437,650.0 | 15,761,745.5 -> 14,023,580.8 | 11.0% | 31.952 -> 35.789 |
| Create 1920x1080 | 69,452,650.0 -> 61,987,300.0 | 152,101,974.0 -> 135,730,457.0 | 10.8% | 29.856 -> 33.452 |
| Update 1920x1080 | 68,772,250.0 -> 57,935,050.0 | 150,628,144.5 -> 126,912,156.5 | 15.7% | 30.152 -> 35.792 |
| Opacity: 3 opaque pixels | 5.7 -> 4.6 | 12.5 -> 10.2 | 18.4% | 527.704 -> 657.174 |
| Opacity: 128x72 opaque | 5,709.9 -> 2,174.9 | 12,527.7 -> 4,776.4 | 61.9% | 1614.037 -> 4237.466 |
| Opacity: 1920x1080 opaque | 1,302,187.5 -> 492,550.0 | 2,830,754.2 -> 1,074,205.6 | 62.1% | 1592.397 -> 4209.928 |
| Opacity: first pixel transparent | 3.6 -> 3.3 | 8.0 -> 7.4 | 7.5% | 278.940 -> 298.954 |
| Opacity: last pixel transparent | 1,256,075.0 -> 494,437.5 | 2,741,363.0 -> 1,084,357.4 | 60.4% | 1650.857 -> 4193.857 |

## Allocation churn

Counts include allocations, reallocations, frees, and requested bytes per operation. Allocated and freed byte totals match in every case and were identical across all three runs. Storage retained before and after an update is not counted as churn.

| Workload | Allocations / reallocations / frees old -> new | Allocated and freed bytes/op old -> new |
|---|---:|---:|
| Create 2x2 (tiny control) | 1/0/1 -> 1/0/1 | 16 -> 16 |
| Update 2x2 | 1/0/1 -> 0/0/0 | 16 -> 0 |
| Create 128x72 | 1/0/1 -> 1/0/1 | 36,864 -> 36,864 |
| Update 128x72 | 1/0/1 -> 0/0/0 | 36,864 -> 0 |
| Create 640x360 | 1/0/1 -> 1/0/1 | 921,600 -> 921,600 |
| Update 640x360 | 1/0/1 -> 0/0/0 | 921,600 -> 0 |
| Create 1920x1080 | 1/0/1 -> 1/0/1 | 8,294,400 -> 8,294,400 |
| Update 1920x1080 | 1/0/1 -> 0/0/0 | 8,294,400 -> 0 |
| Opacity: 3 opaque pixels | 0/0/0 -> 0/0/0 | 0 -> 0 |
| Opacity: 128x72 opaque | 0/0/0 -> 0/0/0 | 0 -> 0 |
| Opacity: 1920x1080 opaque | 0/0/0 -> 0/0/0 | 0 -> 0 |
| Opacity: first pixel transparent | 0/0/0 -> 0/0/0 | 0 -> 0 |
| Opacity: last pixel transparent | 0/0/0 -> 0/0/0 | 0 -> 0 |

## Variation and limits

For 128x72 through 1920x1080 images, creation used 10.3-11.0% fewer median thread cycles and updates used 11.0-15.7% fewer. Full opacity scans and late transparency used 60.4-62.1% fewer cycles. A same-size 1080p update has no temporary allocation; it still retains and writes its 8,294,400-byte output image.

The tiny 2x2 creation control regressed from 239.4 to 265.4 ns/op (11.2% more cycles). This is retained as a measured regression; this pass does not claim a universal speedup. Early-transparency results varied at a few nanoseconds, including slower new timings in two individual runs, despite a faster final median. The ranges below show variation rather than confidence intervals.

| Workload | Range of run medians, ns/op old | Range of run medians, ns/op new |
|---|---:|---:|
| `create_2x2` | 236.8..250.4 | 262.3..266.9 |
| `update_2x2` | 205.8..242.6 | 115.1..120.7 |
| `create_128x72` | 294,750.9..297,934.3 | 257,782.4..269,574.1 |
| `update_128x72` | 292,800.0..303,109.3 | 258,426.9..263,963.0 |
| `create_640x360` | 7,261,000.0..7,513,775.0 | 6,543,325.0..6,597,975.0 |
| `update_640x360` | 7,194,675.0..7,379,925.0 | 6,362,200.0..6,441,550.0 |
| `create_1920x1080` | 65,676,350.0..70,467,750.0 | 60,683,750.0..62,741,950.0 |
| `update_1920x1080` | 67,934,300.0..69,385,200.0 | 57,368,750.0..58,660,050.0 |
| `opacity_tiny` | 5.3..6.0 | 3.9..4.6 |
| `opacity_small` | 5,198.4..5,975.1 | 2,129.3..2,393.1 |
| `opacity_full` | 1,242,400.0..1,484,887.5 | 489,800.0..629,962.5 |
| `opacity_early` | 2.8..4.2 | 3.2..4.1 |
| `opacity_late` | 1,229,337.5..1,271,362.5 | 491,212.5..622,625.0 |

These are synthetic operation-level comparisons on one CPU/compiler configuration. They measure CPU work, throughput, and allocation churn, not process RSS, peak resident memory, end-to-end frame rate, or GPU performance. Initial creation and dimension changes still allocate; stable-size updates are the allocation-free case. CPU architecture, resolution, and where transparency first occurs affect the benefit.

## Reproduce

Run from the repository root with other builds and benchmarks idle:

```powershell
cargo test -p deadlib-render-backend-software --test texture_upload -- --test-threads=1
cargo test -p deadlib-render-backend-software --release --test texture_upload -- --test-threads=1
cargo test -p deadlib-render-backend-software --release --lib -- --test-threads=1
cargo clippy -p deadlib-render-backend-software --lib --test texture_upload -- -D clippy::perf
cargo check --workspace --bins

Remove-Item Env:DEADSYNC_PERF_REVERSE -ErrorAction SilentlyContinue
cargo test -p deadlib-render-backend-software --release --test texture_upload texture_upload_bench -- --ignored --nocapture --test-threads=1
$env:DEADSYNC_PERF_REVERSE = '1'
cargo test -p deadlib-render-backend-software --release --test texture_upload texture_upload_bench -- --ignored --nocapture --test-threads=1
Remove-Item Env:DEADSYNC_PERF_REVERSE
cargo test -p deadlib-render-backend-software --release --test texture_upload texture_upload_bench -- --ignored --nocapture --test-threads=1
```

Take the median of the three reported medians for each timing metric. The shared helper reports zero cycles on non-Windows platforms where this counter is unavailable; that value must not be interpreted as a measured improvement.
