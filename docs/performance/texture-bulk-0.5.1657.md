# Texture bulk operations - 0.5.1657

Parent: `ea7f72dae` (0.5.1656). Date: 2026-10-01.

Three changes apply `rust-performance.md`'s M-HOTPATH, M-THROUGHPUT and
M-MEM-REUSE guidance to texture preparation and sprite initialization:

1. Copy RGBA atlas tiles as complete row slices. The previous generic image
   replacement visited every pixel through image accessors. The existing
   builder validates atlas dimensions and resizes each source before copying,
   so each contiguous source row can be copied directly into its tile. Alpha
   bytes are replaced verbatim, including transparent pixels; neighboring
   tiles and unused atlas space retain their bytes. The isolated 64x64 tile
   fixture uses 81.9% to 86.7% fewer measured CPU cycles.
   Both implementations allocate no copy scratch.
2. Compute one quadrant of each fixed 64px radial mine frame, mirror its RGBA
   bytes horizontally, and copy the complete row to both vertical positions.
   The cached radial geometry is bit-exact under both reflections. This avoids
   repeating layer selection and edge-alpha rounding four times for symmetric
   pixels, while preserving all floating-point operations for the computed
   quadrant. A 256-byte stack row replaces per-pixel output writes. The
   64-color fixture uses 23.3% to 33.6% fewer CPU cycles;
   its sole allocation remains the 1 MiB output image.
3. Freeze animated sprite sources using their existing shared texture key.
   When Arc::get_mut grants exclusive access, replace the value inside that
   Arc; shared or weakly referenced sources get a new Arc so other sprites
   retain their animation.
   Per-freeze allocations fall from three to one for shared sources and from
   three to zero for unique sources. The unique-source fixture uses
   69.7% to 73.0% fewer CPU cycles. Atlas UVs, selected frame
   origins, timing reset and texture-cache reset values remain unchanged.

No production unsafe code or dependencies were added. These paths prepare
assets and initialize sprites. Gameplay frame rate, cold loading, upload,
process RSS and peak live memory were not measured. Owning image operations
still need their output buffers; the new work adds no heap scratch.

## Measurement method

The [raw CSV](texture-bulk-0.5.1657.csv) retains 216 measurements: eighteen
workloads, two implementations and six serial rounds alternating
old-first/new-first. Each median uses seven timing batches; elapsed minimum
and maximum batch values remain in the CSV. Windows QueryThreadCycleTime
measures calling-thread CPU cycles. Allocation counts are collected separately
through the existing counting System allocator, disabled during timing.

Environment: Windows x64, Intel Xeon E5-2696 v4 at 2.20 GHz, 22 cores / 44 logical
processors, Rust 1.98.1, LLVM 22.1.8. Release uses opt-level 3 and full LTO with
test unwinding. Both test executables run serially on logical processor 6
(affinity mask 64), with --test-threads=1, after compiler/check processes finish.
The runner verifies each child process's affinity.

Input palettes, decoded images, PNG files and animation declarations are
created before measurement. Comparisons and warmups precede timing; the
radial profile is initialized. PNG files and metadata are warm in the OS
cache. Fixed-width temporary paths keep allocator byte totals comparable
across rounds. Requested/freed bytes include reallocation sizes and measure
allocator churn, rather than retained memory or peak working set.

The baseline files freeze four exact function bodies and attributes from
ea7f72dae: mine_gradient_texture, model_animation_atlas,
freeze_sprite_animation and the old source_from_plan constructor. A source
audit checks them against the parent. Shared radial geometry, color conversion,
alpha math, sampling and image loading remain unchanged. The production
model_animation_source wrapper is unchanged, including limits and registry
hit behavior. The atlas measurements omit its first-header probe, registry
queries/registration, source-plan construction and upload.

17 of eighteen workloads improve measured CPU cycles and
throughput in every round. All results, including controls, are retained.
Workloads without a consistent improvement in both metrics: `bulk_freeze_atlas_noop`.
Workloads with higher CPU cost in every round: none.
Negative CPU reduction means higher measured cost. File decoding/resizing
can dominate the full atlas operation even when copying itself is much faster.

The already-frozen no-op control has mixed measured CPU changes, from 21.4%
higher cost to 10.8% lower cost. Both variants retain zero allocator churn.
That control does not establish a consistent timing improvement.

## Mirrored mine generation

Throughput is generated pixels/s, including transparent corners. Each timing
batch contains 128 complete owning operations with output destruction.
Palettes of 1/7/64/129 colors exercise small, non-power-of-two, normal and
heap-spilled lengths. The generation benchmark uses varied ordinary RGBA
values. Separate behavior tests cover nonfinite/out-of-range channels.
The full fixture samples a decoded 1024x64 image into 64 colors before
generating its sheet; its sampler is identical in old and new variants.
Texture-key hashing, decoding and registration are outside that fixture.

| Workload | ns/op, old -> new (run 1) | Thread cycles/op, old -> new (run 1) | CPU reduction, six runs | Throughput gain, six runs | Alloc/realloc/free, old -> new | Requested bytes, old -> new |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `bulk_gradient_1` | 10,215.6 -> 2,663.3 | 22,404.4 -> 5,804.7 | 67.9% to 74.1% | 211.9% to 283.6% | 1/0/1 -> 1/0/1 | 16384 -> 16384 |
| `bulk_gradient_7` | 80,225.8 -> 20,128.1 | 175,925.8 -> 44,162.4 | 62.5% to 74.9% | 166.7% to 298.6% | 1/0/1 -> 1/0/1 | 114688 -> 114688 |
| `bulk_gradient_64` | 1,315,235.2 -> 874,350.0 | 2,877,394.7 -> 1,912,016.5 | 23.3% to 33.6% | 30.2% to 50.4% | 1/0/1 -> 1/0/1 | 1048576 -> 1048576 |
| `bulk_gradient_129` | 2,503,686.7 -> 1,658,540.6 | 5,481,959.4 -> 3,632,076.8 | 28.4% to 33.7% | 39.8% to 51.0% | 2/0/2 -> 2/0/2 | 2115584 -> 2115584 |
| `bulk_gradient_full` | 1,428,113.3 -> 994,406.2 | 3,126,901.0 -> 2,178,700.5 | 23.1% to 30.3% | 30.0% to 43.6% | 2/0/2 -> 2/0/2 | 1049600 -> 1049600 |

## Atlas row copying

The isolated copy fixture fills sixteen 64x64 or 256x128 tiles per operation
in a reusable atlas. Inputs vary all RGBA bytes; both variants overwrite the
same positions. Throughput is copied pixels/s, with 128 operations per batch.
Input/output buffer allocation is outside timing and counting, isolating
the production copy helper against the prior imageops::replace call.

| Workload | ns/op, old -> new (run 1) | Thread cycles/op, old -> new (run 1) | CPU reduction, six runs | Throughput gain, six runs | Alloc/realloc/free, old -> new | Requested bytes, old -> new |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `bulk_copy_64x64` | 101,568.8 -> 13,532.0 | 222,542.1 -> 29,675.4 | 81.9% to 86.7% | 457.6% to 650.6% | 0/0/0 -> 0/0/0 | 0 -> 0 |
| `bulk_copy_256x128` | 759,032.8 -> 186,668.8 | 1,663,022.9 -> 408,973.1 | 73.6% to 75.4% | 278.6% to 306.6% | 0/0/0 -> 0/0/0 | 0 -> 0 |

Full atlas operations include warm file reads, PNG decode, RGBA conversion,
triangle resizing, tile copying and owning output destruction. Their existing
consecutive-frame reuse is present in both variants. Throughput is authored
frame pixels/s, excluding unused tiles; batches contain 32 operations.
Single/unique cases use 64x64 sources; repeat uses two runs of eight identical
paths. Resize-repeat changes the second source's dimensions. Alternating
switches paths every frame; large-repeat uses 256x128 tiles with a differently
sized second source. Allocations/bytes are unchanged by row copying.

| Workload | ns/op, old -> new (run 1) | Thread cycles/op, old -> new (run 1) | CPU reduction, six runs | Throughput gain, six runs | Alloc/realloc/free, old -> new | Requested bytes, old -> new |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `bulk_atlas_single` | 316,968.8 -> 302,215.6 | 694,415.7 -> 662,307.0 | 1.6% to 5.6% | 0.9% to 7.7% | 11/1/11 -> 11/1/11 | 111515 -> 111515 |
| `bulk_atlas_unique` | 1,634,337.5 -> 1,565,809.4 | 3,513,948.1 -> 3,427,725.7 | 2.1% to 5.3% | 1.2% to 5.5% | 81/8/81 -> 81/8/81 | 908504 -> 908504 |
| `bulk_atlas_repeat` | 499,225.0 -> 411,946.9 | 1,093,082.5 -> 899,360.0 | 15.3% to 18.5% | 17.8% to 23.2% | 21/2/21 -> 21/2/21 | 452406 -> 452406 |
| `bulk_atlas_resize_repeat` | 606,793.8 -> 532,306.2 | 1,328,777.6 -> 1,164,598.3 | 8.5% to 14.4% | 9.5% to 17.2% | 25/3/25 -> 25/3/25 | 504826 -> 504826 |
| `bulk_atlas_alternating` | 4,093,859.4 -> 4,043,221.9 | 8,960,641.6 -> 8,851,522.6 | 0.1% to 2.5% | 0.2% to 2.8% | 193/24/193 -> 193/24/193 | 2203600 -> 2203600 |
| `bulk_atlas_large_repeat` | 4,030,646.9 -> 3,486,937.5 | 8,819,290.6 -> 7,628,180.6 | 9.2% to 13.5% | 9.8% to 15.6% | 25/4/25 -> 25/4/25 | 3671162 -> 3671162 |

## Frozen sprite source reuse

Shared-source operations clone, freeze and destroy 64 sprite slots per call;
fixture creation is outside timing. A held template keeps the original
animation alive, exercising the shared-source branch. Short/long keys use
16/1024 bytes; indexed uses an explicit frame permutation. Atlas-noop freezes
an already static source as a control. Batches contain 256 calls; throughput
is frozen slots/s. Counts include destruction of complete owning results.

Unique-source operations use 64 freshly created, uniquely owned slots per
call. Setup and final slot destruction are outside timing and counting,
using measure_sampled_with_setup; batches contain 128 calls. Freezing itself
releases the old animation payloads. For that workload, requested/freed byte
totals are intentionally asymmetric per 64 slots: the old operation requests
19,968 bytes and frees 24,064; the new one requests zero and frees 4,096.
New unique-source freezing allocates zero bytes;
it still frees the no-longer-needed frame-duration buffers. The shared-source
workloads measure complete owning lifetimes separately.

| Workload | ns/op, old -> new (run 1) | Thread cycles/op, old -> new (run 1) | CPU reduction, six runs | Throughput gain, six runs | Alloc/realloc/free, old -> new | Requested bytes, old -> new |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `bulk_freeze_short` | 18,715.2 -> 10,902.0 | 41,054.2 -> 23,922.9 | 36.6% to 43.4% | 57.9% to 77.8% | 192/0/192 -> 64/0/64 | 19968 -> 16896 |
| `bulk_freeze_long` | 25,812.1 -> 12,036.7 | 56,532.4 -> 26,387.2 | 47.3% to 60.2% | 90.2% to 152.3% | 192/0/192 -> 64/0/64 | 148992 -> 16896 |
| `bulk_freeze_indexed` | 22,675.0 -> 13,170.7 | 49,737.3 -> 28,896.0 | 38.0% to 45.6% | 60.8% to 82.0% | 192/0/192 -> 64/0/64 | 19968 -> 16896 |
| `bulk_freeze_atlas_noop` | 5,618.4 -> 5,617.6 | 12,334.0 -> 12,129.1 | -21.4% to 10.8% | -17.6% to 12.1% | 0/0/0 -> 0/0/0 | 0 -> 0 |
| `bulk_freeze_unique` | 21,346.1 -> 5,409.4 | 48,672.4 -> 13,346.6 | 69.7% to 73.0% | 265.5% to 309.7% | 192/0/256 -> 0/0/64 | 19968 -> 0 |

## Behavior and validation

Nine new ordinary tests verify:

- Every generated RGBA byte for twelve palette lengths (1/2/3/7/16/31/32/33/
  63/64/65/129) and thirteen color patterns. Values cover finite channels,
  signed zero, subnormals, out-of-range values, infinities and NaN payloads.
  Empty palettes still panic. All cached geometry/edge-alpha bits match
  under horizontal and vertical reflection.
- One output allocation and no heap scratch for normal mine palettes.
- Atlas tile order, transparent alpha replacement, triangle resizing and
  unused tile padding for tiny, odd, square and rectangular images. Direct
  copies at origin, interior and bottom/right edges preserve surrounding
  pixels without allocator churn. Missing/corrupt errors remain identical;
  changed files are reread across builds.
- Frozen selected frames and UV float bits for identity, explicit-empty and
  indexed frame lists; positive/negative origins and mirrored definitions.
  Texture-cache reset values match the parent. Shared sources retain their
  animation, texture-key storage is reused, repeated freezing is a no-op,
  and unique sources keep their Arc address without allocating.

Validation passed: nine new debug tests; the full release library suites
(166 assets + 321 noteskin = 487 tests, 22 manual benchmarks ignored);
cargo check --workspace --offline --locked; performance Clippy on both
libraries; rustfmt on all six changed Rust files; frozen-source audit;
git diff --check. Clippy uses -D clippy::perf -A clippy::large_enum_variant
for the existing large enum and retains existing style warnings.

Reproduce behavior checks and benchmarks from PowerShell:

```powershell
cargo test -p deadsync-noteskin -p deadsync-assets --lib --release --offline --locked
cargo check --workspace --offline --locked
cargo clippy -p deadsync-noteskin -p deadsync-assets --lib --offline --locked --no-deps -- -D clippy::perf -A clippy::large_enum_variant
$env:DEADSYNC_PERF_ORDER = 'old-first' # alternate with new-first in later rounds
# Run after building, serially, on one core for comparable paired measurements.
cargo test -p deadsync-noteskin --lib --release --offline --locked benchmark_gradient_symmetry -- --ignored --nocapture --test-threads=1
cargo test -p deadsync-assets --lib --release --offline --locked benchmark_texture_bulk -- --ignored --nocapture --test-threads=1
```
