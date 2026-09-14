# Image loading — 0.5.1229

Baseline: `12a1090ed` (0.5.1228). The supplied `rust-performance.md` recommends
measuring CPU and allocation costs (M-HOTPATH), reusing owned storage rather
than copying it (M-MEM-REUSE), and increasing useful work per CPU cycle
(M-THROUGHPUT). This pass addresses three costs in image loading and preview
preparation.

## Changes

1. **Banner creation consumes decoded RGBA storage.** The image returned by the
   decoder is already owned. `into_rgba8()` transfers its pixel buffer when it
   is RGBA8, avoiding the former clone followed by destruction. Other decoded
   pixel types still convert to the same RGBA8 values; video poster loading
   retains its existing path.
2. **Image format fallback reuses the open file and read buffer.** When decoding
   by the recognized extension fails, rewind the existing buffered reader and
   retry format guessing. This removes the second file open, second read-buffer
   allocation, and temporary extension copies. A failed rewind retains the
   original reopen fallback. Unknown extensions keep the original reader setup
   and error reporting. The decoder's default limits and mismatch warning remain.
3. **Workshop previews resize a borrowed sprite frame.** Crop coordinates form
   a view over the decoded image instead of allocating and copying a full first
   frame. The same thumbnail sampler produces the 60×60 preview. Filename rules,
   first-frame selection, color metadata, and image limits are preserved.

The changes affect cache rebuilds, texture/image loading, and Workshop preview
compilation. They do not make image decoding allocation-free. RGBA ownership
transfer itself has zero heap churn; decoding and the final preview still need
owned buffers.

## Results

| Complete operation and fixture | Fewer thread cycles | Throughput gain | Allocation/free calls old → new | Allocated/freed bytes old → new |
|---|---:|---:|---:|---:|
| Create a 1024×512 banner (2 MiB pixels) | 6.3% | 1.07x | 12 → 10 | 4,616,336 → 2,519,141 |
| Load a 320×80 PNG named `.jpg` | 32.7% | 1.48x | 16 → 12 | 536,733 → 528,377 |
| Create a 1024×512 `1x1` sheet preview | 10.1% | 1.11x | 19 → 18 | 4,630,785 → 2,533,633 |

Banner creation eliminates one image-sized temporary buffer. Requested-byte
churn falls 45.4% for the 2 MiB fixture and 18.9% for the 320×80 fixture.
The isolated ownership transfer replaces one allocation/free with zero and
keeps the exact pixel address. Its measured work becomes a small fixed-cost
move, so very large isolated speedup ratios mostly express removal of the
copy; they are not whole-loader speedups.

Format fallback saves 8,356 requested bytes for each measured mismatching PNG,
including the 8 KiB read buffer. A one-pixel mismatch uses 39.7% fewer cycles;
malformed PNG handling uses 44.1% fewer. The large 2 MiB mismatch is dominated
by decoding and improves only 1.2%. Recognized-extension success paths save
one temporary allocation, with measured cycle reductions of 0.7–4.8%.

The isolated 1024×512 frame crop/resample uses 47.6% fewer cycles, improves
throughput 1.91x, and reduces requested bytes from 2,111,552 to 14,400. Only
the final preview allocation remains. The `2x1` sheet fixture
(1024×512 source) eliminates a 1 MiB frame copy, lowers complete operation
churn 29.3%, and uses 4.8% fewer cycles. A `1x1` sheet eliminates the full
2 MiB copy, lowering complete operation churn 45.3%.

Controls and limits: unknown-extension successful loads retain their allocation
counters and use 0.4–4.0% more median cycles; no improvement is claimed there.
Unknown-extension invalid input uses 1.5% more cycles. Plain thumbnails retain
their allocation counters; their small timing differences are not attributed
to the crop change. Some small-frame cases vary around baseline: the 64×64
`2x1` case uses 0.7% more cycles, the 320×160 `16x8` case uses 0.9% more, and
the 1024×512 `16x8` case is effectively unchanged. They still remove the frame
allocation. The one-pixel banner's timing is effectively unchanged.

[Raw measurements: 330 rows, 33 workloads, five processes per crate, both variants](image-loading-0.5.1229.csv).

### Timing and throughput

Negative reductions mean more cycles. All throughput values are operations/s.
The `rgba_owned` rows isolate ownership transfer, and the `thumbnail_frame`
rows isolate crop/resampling; other rows include file I/O and image decoding.

| Workload | ns old → new | Thread cycles old → new | Fewer cycles | Operations/s old → new |
|---|---:|---:|---:|---:|
| `fallback/correct/1x1` | 89,181.2 → 85,006.2 | 195,437.3 → 186,108.6 | 4.8% | 11,213.1 → 11,763.8 |
| `fallback/mismatch/1x1` | 183,525.0 → 110,550.0 | 401,849.6 → 242,163.3 | 39.7% | 5,448.8 → 9,045.7 |
| `fallback/unknown/1x1` | 92,543.8 → 93,281.2 | 202,090.9 → 204,299.6 | -1.1% | 10,805.7 → 10,720.3 |
| `banner/1x1` | 83,668.8 → 82,475.0 | 182,267.3 → 180,662.2 | 0.9% | 11,951.9 → 12,124.9 |
| `fallback/correct/320x80` | 695,400.0 → 682,706.2 | 1,520,572.6 → 1,484,368.8 | 2.4% | 1,438.0 → 1,464.8 |
| `fallback/mismatch/320x80` | 856,693.8 → 580,418.8 | 1,876,354.4 → 1,263,140.3 | 32.7% | 1,167.3 → 1,722.9 |
| `fallback/unknown/320x80` | 566,675.0 → 591,118.8 | 1,230,763.9 → 1,279,767.2 | -4.0% | 1,764.7 → 1,691.7 |
| `banner/320x80` | 719,750.0 → 682,456.2 | 1,565,405.4 → 1,464,599.9 | 6.4% | 1,389.4 → 1,465.3 |
| `fallback/correct/1024x512` | 8,380,081.2 → 8,338,887.5 | 18,274,266.7 → 18,153,088.7 | 0.7% | 119.3 → 119.9 |
| `fallback/mismatch/1024x512` | 9,067,025.0 → 9,016,712.5 | 19,862,349.1 → 19,631,544.8 | 1.2% | 110.3 → 110.9 |
| `fallback/unknown/1024x512` | 8,207,787.5 → 8,271,462.5 | 17,938,047.5 → 18,015,421.2 | -0.4% | 121.8 → 120.9 |
| `banner/1024x512` | 8,946,525.0 → 8,377,718.8 | 19,454,573.3 → 18,235,964.2 | 6.3% | 111.8 → 119.4 |
| `fallback/invalid.png` | 157,637.5 → 89,025.0 | 345,589.0 → 193,091.4 | 44.1% | 6,343.7 → 11,232.8 |
| `fallback/unknown.data` | 76,212.5 → 75,737.5 | 163,431.4 → 165,914.6 | -1.5% | 13,121.2 → 13,203.5 |
| `rgba_owned/1x1` | 103.1 → 46.9 | 1,365.0 → 1,262.1 | 7.5% | 9,696,969.7 → 21,333,333.3 |
| `rgba_owned/320x80` | 9,256.2 → 40.6 | 21,661.9 → 1,269.0 | 94.1% | 108,035.1 → 24,615,384.6 |
| `rgba_owned/1024x512` | 720,031.2 → 62.5 | 1,573,108.6 → 2,195.0 | 99.9% | 1,388.8 → 16,000,000.0 |
| `rgba_owned/2048x2048` | 5,273,165.6 → 84.4 | 11,529,381.5 → 2,147.0 | 99.981% | 189.6 → 11,851,851.9 |
| `thumbnail/plain/64x64` | 378,506.2 → 372,818.8 | 824,304.8 → 805,249.4 | 2.3% | 2,642.0 → 2,682.3 |
| `thumbnail/sheet1x1/64x64` | 397,100.0 → 380,450.0 | 863,485.5 → 828,790.8 | 4.0% | 2,518.3 → 2,628.5 |
| `thumbnail/sheet2x1/64x64` | 378,381.2 → 379,625.0 | 826,294.0 → 831,836.4 | -0.7% | 2,642.8 → 2,634.2 |
| `thumbnail/sheet16x8/64x64` | 364,156.2 → 350,318.8 | 792,943.8 → 746,752.7 | 5.8% | 2,746.1 → 2,854.5 |
| `thumbnail/plain/320x160` | 1,221,593.8 → 1,195,968.8 | 2,649,653.1 → 2,593,653.2 | 2.1% | 818.6 → 836.1 |
| `thumbnail/sheet1x1/320x160` | 1,267,912.5 → 1,207,012.5 | 2,756,988.8 → 2,617,510.1 | 5.1% | 788.7 → 828.5 |
| `thumbnail/sheet2x1/320x160` | 1,145,193.8 → 1,109,143.8 | 2,506,950.7 → 2,425,708.2 | 3.2% | 873.2 → 901.6 |
| `thumbnail/sheet16x8/320x160` | 1,038,737.5 → 1,044,175.0 | 2,261,138.1 → 2,281,620.2 | -0.9% | 962.7 → 957.7 |
| `thumbnail/plain/1024x512` | 9,391,225.0 → 9,385,200.0 | 20,574,270.2 → 20,443,722.6 | 0.6% | 106.5 → 106.6 |
| `thumbnail/sheet1x1/1024x512` | 10,666,518.8 → 9,585,081.2 | 23,346,376.9 → 20,991,018.6 | 10.1% | 93.8 → 104.3 |
| `thumbnail/sheet2x1/1024x512` | 9,543,106.2 → 9,138,293.8 | 20,880,980.6 → 19,872,528.6 | 4.8% | 104.8 → 109.4 |
| `thumbnail/sheet16x8/1024x512` | 8,450,862.5 → 8,488,825.0 | 18,508,981.0 → 18,517,514.1 | -0.0% | 118.3 → 117.8 |
| `thumbnail_frame/64x64` | 91,756.2 → 89,481.2 | 201,103.2 → 196,246.8 | 2.4% | 10,898.4 → 11,175.5 |
| `thumbnail_frame/256x256` | 314,375.0 → 251,475.0 | 689,435.8 → 549,751.4 | 20.3% | 3,180.9 → 3,976.5 |
| `thumbnail_frame/1024x512` | 2,342,381.2 → 1,225,200.0 | 5,104,061.1 → 2,673,098.4 | 47.6% | 426.9 → 816.2 |

### Allocation churn

| Workload | Allocations/frees old → new | Reallocations old → new | Allocated/freed bytes old → new |
|---|---:|---:|---:|
| `fallback/correct/1x1` | 11 → 10 | 1 → 1 | 37,270 → 37,227 |
| `fallback/mismatch/1x1` | 16 → 12 | 1 → 1 | 45,730 → 37,374 |
| `fallback/unknown/1x1` | 13 → 13 | 1 → 1 | 37,282 → 37,282 |
| `banner/1x1` | 12 → 10 | 1 → 1 | 37,272 → 37,225 |
| `fallback/correct/320x80` | 11 → 10 | 1 → 1 | 439,186 → 439,143 |
| `fallback/mismatch/320x80` | 16 → 12 | 1 → 1 | 536,733 → 528,377 |
| `fallback/unknown/320x80` | 13 → 13 | 1 → 1 | 439,198 → 439,198 |
| `banner/320x80` | 12 → 10 | 1 → 1 | 541,584 → 439,141 |
| `fallback/correct/1024x512` | 11 → 10 | 1 → 1 | 2,519,186 → 2,519,143 |
| `fallback/mismatch/1024x512` | 16 → 12 | 1 → 1 | 4,350,658 → 4,342,302 |
| `fallback/unknown/1024x512` | 13 → 13 | 1 → 1 | 2,519,198 → 2,519,198 |
| `banner/1024x512` | 12 → 10 | 1 → 1 | 4,616,336 → 2,519,141 |
| `fallback/invalid.png` | 21 → 13 | 0 → 0 | 55,933 → 47,567 |
| `fallback/unknown.data` | 10 → 9 | 0 → 0 | 8,382 → 8,378 |
| `rgba_owned/1x1` | 1 → 0 | 0 → 0 | 4 → 0 |
| `rgba_owned/320x80` | 1 → 0 | 0 → 0 | 102,400 → 0 |
| `rgba_owned/1024x512` | 1 → 0 | 0 → 0 | 2,097,152 → 0 |
| `rgba_owned/2048x2048` | 1 → 0 | 0 → 0 | 16,777,216 → 0 |
| `thumbnail/plain/64x64` | 15 → 15 | 1 → 1 | 109,533 → 109,533 |
| `thumbnail/sheet1x1/64x64` | 19 → 18 | 1 → 1 | 125,953 → 109,569 |
| `thumbnail/sheet2x1/64x64` | 19 → 18 | 1 → 1 | 117,761 → 109,569 |
| `thumbnail/sheet16x8/64x64` | 19 → 18 | 1 → 1 | 109,700 → 109,572 |
| `thumbnail/plain/320x160` | 15 → 15 | 1 → 1 | 641,245 → 641,245 |
| `thumbnail/sheet1x1/320x160` | 19 → 18 | 1 → 1 | 846,081 → 641,281 |
| `thumbnail/sheet2x1/320x160` | 19 → 18 | 1 → 1 | 743,681 → 641,281 |
| `thumbnail/sheet16x8/320x160` | 19 → 18 | 1 → 1 | 642,884 → 641,284 |
| `thumbnail/plain/1024x512` | 15 → 15 | 1 → 1 | 2,533,597 → 2,533,597 |
| `thumbnail/sheet1x1/1024x512` | 19 → 18 | 1 → 1 | 4,630,785 → 2,533,633 |
| `thumbnail/sheet2x1/1024x512` | 19 → 18 | 1 → 1 | 3,582,209 → 2,533,633 |
| `thumbnail/sheet16x8/1024x512` | 19 → 18 | 1 → 1 | 2,550,020 → 2,533,636 |
| `thumbnail_frame/64x64` | 2 → 1 | 0 → 0 | 30,784 → 14,400 |
| `thumbnail_frame/256x256` | 2 → 1 | 0 → 0 | 276,544 → 14,400 |
| `thumbnail_frame/1024x512` | 2 → 1 | 0 → 0 | 2,111,552 → 14,400 |

Counters are per operation and identical across the five processes. Frees
equal allocations, and freed bytes equal allocated bytes in every row.
Reallocation bytes are included in the byte totals.

## Method

- Windows x86-64; Intel Xeon E5-2696 v4 @ 2.20 GHz, 44 logical processors.
  Rust 1.98.0 (`88d9e12ae`, 2026-08-18), LLVM 22.1.8. Workspace release profile:
  optimization level 3 and full LTO.
- Frozen baseline function bodies are checked against the parent commit after
  whitespace normalization. The old banner builder calls the old image loader.
  The isolated frame baseline reproduces the original crop-copy-resize sequence.
- Each crate's old and new implementations run in the same release test binary
  with the shared system allocator wrapper. There are five fresh processes per
  crate, ten serial processes in total, pinned to logical CPU 4. Old/new order
  alternates between processes. No build or test runs concurrently with the
  recorded benchmarks. Source and executable hashes are checked after recording.
- Every variant has three warmups and seven batches of 16 operations. Isolated
  RGBA conversion uses 32 operations per batch. Results are medians of the five
  process medians. The CSV retains each process's time range, thread cycles,
  throughput, allocation/free calls, reallocations, and requested bytes.
- CPU cycles use Windows `QueryThreadCycleTime`. Time and cycle medians are
  calculated independently. Throughput is completed operations/images per second.
- File fixtures contain deterministic RGBA patterns with opaque, translucent,
  and transparent pixels. Encoding, fixture setup, harness path construction,
  and equality checks are outside the measured operation. Normal operation
  measurements include destruction of the result.
- The isolated conversion fixture is an already-owned decoded image. Its setup
  clone and final destruction are outside timing and allocation counting for
  both variants. The old conversion's replacement allocation and old-buffer
  destruction are counted; the new conversion retains that same allocation.
  Per-operation clock overhead is included equally. Frame-only measurements
  start from the same borrowed decoded image and include preview destruction.
- Disk files are warm in the OS cache. These measurements describe serial
  operations, not cold-storage latency or whole-application startup time.
  Allocation bytes measure churn, not peak RSS or committed memory. Hardware
  cache misses, power, disk throughput, and total CPU across worker threads
  were not measured.

## Behavioral verification

- `cargo test -p deadlib-assets -p deadsync-noteskin --locked --no-fail-fast`:
  **376 tests passed**, 13 manual or fixture-dependent tests ignored.
- Targeted release behavior tests: three image-loading tests and three
  thumbnail-loading tests passed; their two manual benchmarks were ignored.
- Format fallback comparisons cover PNG, JPEG, BMP, TGA, TIFF, WebP, and GIF
  bytes under matching, mismatching, uppercase, unknown, missing, and empty
  extensions. Both warning modes produce the same image results. Empty,
  malformed, header-only, and truncated inputs preserve error debug/display
  text; missing files and directories preserve their errors too.
- Banner comparisons cover eight PNG color types and ten in-memory image
  variants, including 16-bit and floating-point conversion. RGBA ownership
  transfer retains the exact pixel pointer and has zero heap churn. Missing
  image and video paths retain their errors. A separate allocation assertion
  uses the same current file loader on both sides to isolate the clone removal.
- Thumbnail comparisons cover empty/tiny/large images, non-divisible frame
  sizes, clamped regions, grayscale/RGB/16-bit sources, first-frame selection,
  uppercase sheet separators, malformed dimensions, overflow, logical
  resolution tags, and multiple conflicting sheet tags. Invalid files and
  dimensions above the 8192-pixel limits retain their errors. Allocation
  assertions cover both complete sheet loads and isolated frame resampling.
- `cargo check --locked` passed for the application.
- `cargo clippy -p deadlib-assets -p deadsync-noteskin --lib --locked -- -D clippy::perf`
  passed; existing warnings remain. Changed Rust files pass formatting checks,
  and the diff passes whitespace checks.
- These checks ran on Windows x86-64 with regular disk files. Non-seekable
  inputs and concurrent file replacement were not exercised. The reuse path
  intentionally retries against the same open file. An existing file symlink
  test could not exercise its assertions because this account lacks Windows
  symlink privilege; its directory-junction companion passed.
- DeadSync is bumped exactly once from 0.5.1228 to 0.5.1229 in `Cargo.toml` and
  the three workspace-version package entries in `Cargo.lock`.

Reproduce with:

```powershell
cargo test -p deadlib-assets -p deadsync-noteskin --locked --no-fail-fast
cargo test -p deadlib-assets -p deadsync-noteskin --release --lib --locked --no-run
cargo test -p deadlib-assets --release --lib --locked benchmark_image_loading -- --ignored --nocapture --test-threads=1
cargo test -p deadsync-noteskin --release --lib --locked benchmark_thumbnail_loading -- --ignored --nocapture --test-threads=1
```

For stable repeated measurements, invoke the two built release test executables
directly in five fresh processes each, pinned to one CPU. Set
`DEADSYNC_PERF_REVERSE=1` for every second process. Building each crate separately
can select a different dependency feature union; the recorded executables came
from the combined two-package build above.
