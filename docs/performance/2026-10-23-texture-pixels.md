# Texture decoding and resize data paths

Three safe Rust changes reduce texture preparation work while preserving decoded
pixels, color keying, canvas offsets, and fixed-point resize rounding. Baseline:
`458c71c88`. Application version: `0.5.1873` → `0.5.1874`.

1. **PNG RGBA buffer ownership.** Move the decoder's RGBA8 buffer into
   `RgbaImage`, removing a second image allocation, its zero fill, and the entire
   pixel conversion/copy pass. The decoder still strips 16-bit channels and
   expands transparency before this decision; color keying follows it as before.
2. **GIF row writes.** Expand palette indices directly into contiguous canvas
   rows. This removes per-pixel division, remainder, and coordinate-based writes.
   The validated frame origin selects the destination rows and column offset;
   untouched canvas pixels retain their transparent initialization.
3. **Resize row addressing.** Resolve the upper and lower source rows once per
   output row and write through the destination row slice. This replaces five
   coordinate-based pixel accesses per output pixel. Axis generation, every
   integer multiply/add/shift, horizontal flooring, vertical rounding, and the
   sequence of resize passes remain unchanged.

These changes follow `M-HOTPATH`, `M-MEM-REUSE`, and `M-THROUGHPUT` in
[rust-performance.md](../../rust-performance.md). There are no new production
helpers, caches, dependencies, or `unsafe` blocks. The asset and presentation
source audit found no existing production `unsafe` in these paths to replace.
The benchmark uses documented Windows FFI for cycle counting and CPU affinity,
and the repository's existing allocation counter.

## Measurements

The table pools the final three confirmation runs: 27 samples per implementation
per workload, 486 samples in total. Values are medians per decoded image or
complete resize. Cycle and rate columns pool all samples; reductions and
throughput multipliers are medians of the 27 matched sample pairs, so they can
differ from ratios of the pooled medians. Every large workload improves in all
three runs. Small images have more variable results because file opening and
scheduling are a larger part of their cost; the unchanged baseline comparison
also varied by about 5%.

| Workload | Thread cycles before | After | Paired cycle reduction | Images/s before | After | Paired throughput | Allocations before → after | Allocated bytes before → after |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| RGBA PNG 64×64 | 450,140 | 407,579 | 7.2% | 4,868.4 | 5,373.2 | 1.08× | 9 → 8 | 110,670 → 94,286 |
| RGBA PNG 512×512 | 4,556,611 | 2,894,173 | 37.7% | 480.1 | 755.8 | 1.60× | 9 → 8 | 2,518,350 → 1,469,774 |
| RGBA PNG 1024×1024 | 19,060,381 | 10,996,039 | 38.3% | 114.8 | 198.7 | 1.61× | 9 → 8 | 8,809,806 → 4,615,502 |
| GIF 64×64 frame with offset | 415,481 | 390,205 | 10.1% | 5,274.0 | 5,621.0 | 1.11× | 12 → 12 | 68,964 → 68,964 |
| GIF 512×512 frame with offset | 8,279,617 | 5,880,877 | 28.6% | 264.1 | 371.6 | 1.40× | 12 → 12 | 1,368,164 → 1,368,164 |
| GIF 1024×1024 frame with offset | 26,776,190 | 19,855,290 | 31.7% | 81.7 | 110.2 | 1.46× | 12 → 12 | 5,310,564 → 5,310,564 |
| Resize 64×64 → 128×128 | 509,086 | 417,774 | 16.6% | 4,293.5 | 5,249.1 | 1.20× | 3 → 3 | 68,608 → 68,608 |
| Resize 512×512 → 256×256 | 2,216,313 | 1,926,881 | 15.6% | 990.0 | 1,136.7 | 1.18× | 3 → 3 | 268,288 → 268,288 |
| Resize 256×256 → 1024×1024 | 40,733,910 | 35,248,407 | 14.4% | 53.7 | 62.1 | 1.17× | 6 → 6 | 5,279,744 → 5,279,744 |

PNG saves exactly one allocation of `width * height * 4` bytes: 16 KiB, 1 MiB,
and 4 MiB respectively. PNG retains one decoder reallocation per image; GIF and
resize perform none. GIF and resize allocation traffic is unchanged.

The initial unchanged benchmark was recorded before any production edits. PNG,
GIF, and resize were then changed and measured separately, with additional
unchanged GIF and resize measurements immediately before their edits. The
[JSON evidence](2026-10-23-texture-pixels.json) retains all twelve campaigns,
including the unchanged comparisons and every preliminary sample. The final
three runs follow the test-only Clippy cleanup; production pixel code is
unchanged. The evidence also contains source/binary hashes and the range of CPU
speedups across confirmation runs. The three frozen functions were verified
against the baseline after identical formatting.

Environment and limits:

- Intel Xeon E5-2696 v4, Windows x86_64, Rust 1.99.1, release opt-level 3,
  disabled LTO, static MSVC runtime; benchmark worker pinned to CPU 2.
- `QueryThreadCycleTime` records scheduled thread cycles including kernel
  execution. These are not retired-instruction or user-only PMU measurements.
- Each run alternates original/current order for ten rounds and discards the
  first round. Samples contain 20 decodes or 10 complete resizes. Fixture
  generation and resize input cloning are outside measurement; file reading,
  output construction, and output destruction are inside it.
- PNG inputs are deterministic pseudorandom RGBA images. GIF inputs use a
  deterministic 256-color indexed pattern, transparency, and a frame offset in
  a larger canvas. The filesystem is warm. These are targeted texture-processing
  measurements, not whole-game frame-rate or cold-start claims.
- Allocation counts cover current-thread Rust `System` allocation traffic;
  native allocations and retained RSS are excluded. Counter overhead is shared
  by both implementations. Temporary path lengths explain two-byte differences
  between some preliminary campaigns; paired variants use the same path.
- Builds and measurements ran sequentially. Other desktop activity was not
  controlled, and no statistical confidence claim is made for the small cases.

## Correctness and reproduction

The new comparisons cover 46 committed PNG/GIF files with color keying both on
and off, generated images of four dimensions, GIF offsets, truncated PNG errors,
and 49 resize dimension combinations including one-pixel axes, odd dimensions,
mixed axes, and multiple resize passes. All output bytes and dimensions match
the frozen originals. Existing tests also verify native bitmap captures,
16-bit PNG stripping, hidden-alpha cleanup, and texture hint combinations.

The asset and version suites pass **103 tests**, with four opt-in benchmarks
ignored during the ordinary test run. The new benchmark was run explicitly.
Clippy passes with four existing production warnings (one nested condition, two
constant-size chunk iterators, and an eight-argument sprite function) plus the
two chunk-iterator warnings copied into the frozen baseline. The new
implementation and benchmark introduce no other warnings. New test files pass
rustfmt; the diff passes whitespace checks.

```powershell
cargo test -p deadlib-assets -p deadsync-version --release --locked --offline -j 2 --config profile.release.lto=false -- --test-threads=1
cargo clippy -p deadlib-assets --release --locked --offline -j 2 --config profile.release.lto=false --no-deps --all-targets
cargo test -p deadlib-assets --release --locked --offline -j 2 --config profile.release.lto=false --lib benchmark_texture_pixels -- --ignored --nocapture --test-threads=1
```

The benchmark emits one `PIXEL_BENCH` JSON line per workload. Run it three times
to reproduce the confirmation sample count. Set `DEADSYNC_PIXEL_BENCH` to `png`,
`gif`, or `zoom` to isolate a change. Complete local logs are under
`target/texture-pixels`.
