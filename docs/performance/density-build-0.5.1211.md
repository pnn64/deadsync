# Density graph construction - 0.5.1211

Baseline: `7619f9cb22f619cc4dc7598b5a3bd06bce9ff665` / 0.5.1210.
This pass applies `rust-performance.md` guidance on measuring hot paths
(M-HOTPATH), reserving enough capacity (M-INITIAL-CAPACITY), and reusing
allocations (M-MEM-REUSE) to three operations in the shared density module:

1. **Borrow temporary columns when building one-shot meshes.** Previously,
   the builder created and immediately discarded an `Arc` column cache.
   A private borrowed view now reads the temporary columns directly. This
   removes one allocation, one free and a column copy. The temporary vector
   is still shrunk to a boxed slice before allocating vertices, releasing
   capacity discarded by plateau compression. Persistent caches keep their
   existing ownership and storage. Empty and single-measure builders also
   return before computing colors.
2. **Reuse rounded heights for repeated density values.** Repeated f32
   density bits reuse the preceding rounded height. Colors are calculated
   only after deciding that a column survives plateau compression. Changes
   in density retain the original arithmetic; values that round to equal
   heights retain the same preceding colors and column positions.
3. **Append complete vertex segments and reuse growing buffers.** New meshes
   append six vertices per segment with one capacity check. Growing reusable
   meshes clear and refill retained storage without zero-initializing vertices
   that are immediately overwritten. Stable or shrinking meshes keep the
   existing overwrite path. Shared and weak owners still force replacement.

These paths serve gameplay graph construction and scrolling, evaluation,
and practice. Cache construction and one-shot graphs run when graphs are
loaded or rescaled; reusable updates serve changing visible windows.

For 4,096 inputs, one-shot varied graphs use **47.0% fewer
thread cycles**, flat cache construction uses **42.4% fewer**,
and the reusable growing-window sequence uses **21.5% fewer**.
Nonempty one-shot builds drop from **3 allocations/frees to 2**. Warmed
updates keep **zero allocation churn** when the existing capacity suffices.
These are synthetic function benchmarks, not whole-game CPU or FPS results.

## Measurement

Windows x86-64, Intel Xeon E5-2696 v4 at 2.20 GHz, Rust 1.98.0
(`88d9e12ae`, 2026-08-18), repository release profile (opt-level 3, LTO).
The integration test invokes the public production API and the entire frozen
parent density module in the same executable. The baseline was compared
with its parent source after normalizing whitespace and its module comment;
its algorithms and ownership operations are unchanged.

Five complete runs alternate old-first/new-first, using three warmups and
seven timing samples per measurement. Tables show medians of the five
per-run medians. Windows `QueryThreadCycleTime` measures calling-thread CPU
cycles. Allocation tracking is a separate operation outside timing samples.
The [CSV](density-build-0.5.1211.csv) contains all 340 measurements, including
wall-time ranges, throughput, allocation/reallocation/free counts, and
requested/freed bytes. The final runs are collected after this pass's builds
and checks. The machine is shared; processor state, heap state and background load vary.
Very short controls are especially sensitive to measurement overhead.

Inputs are prepared outside measurement and passed through black boxes.
Owning builders include destruction of their returned buffers and caches.
Cached mesh/update cases build caches before timing. Cold updates start with
no mesh; shared updates retain a previous frame, clone its owner, then replace
and destroy the new mesh inside measurement. Steady and growing updates
retain their warmed buffers outside timing and allocation tracking.

## Results

29 of 34 workloads have lower median thread cycles.
Negative reductions indicate slower results; all controls are retained.

| Workload | Old ns/op | New ns/op | Thread cycles old/new | Cycle reduction | Throughput old/new (M units/s) |
|---|---:|---:|---:|---:|---:|
| `one_shot_flat_0` | 17.2 | 13.9 | 40.3 / 33.0 | 18.1% | 58.182 / 72.113 |
| `cache_flat_0` | 16.8 | 13.9 | 39.4 / 33.0 | 16.2% | 59.535 / 72.113 |
| `one_shot_blocks_0` | 17.2 | 13.9 | 39.9 / 33.0 | 17.3% | 58.182 / 72.113 |
| `cache_blocks_0` | 16.8 | 13.9 | 39.4 / 33.0 | 16.2% | 59.535 / 72.113 |
| `one_shot_varied_0` | 17.0 | 13.9 | 39.9 / 33.0 | 17.3% | 58.851 / 72.113 |
| `cache_varied_0` | 16.4 | 13.5 | 38.2 / 32.2 | 15.7% | 60.952 / 74.203 |
| `one_shot_flat_1` | 16.8 | 13.5 | 39.0 / 31.7 | 18.7% | 59.535 / 74.203 |
| `cache_flat_1` | 16.4 | 13.5 | 38.2 / 31.7 | 17.0% | 60.952 / 74.203 |
| `one_shot_blocks_1` | 16.6 | 13.5 | 39.0 / 32.2 | 17.4% | 60.235 / 74.203 |
| `cache_blocks_1` | 16.2 | 13.5 | 38.2 / 32.2 | 15.7% | 61.687 / 74.203 |
| `one_shot_varied_1` | 16.6 | 13.5 | 39.0 / 31.7 | 18.7% | 60.235 / 74.203 |
| `cache_varied_1` | 15.8 | 13.5 | 37.3 / 31.7 | 15.0% | 63.210 / 74.203 |
| `one_shot_flat_64` | 925.6 | 445.1 | 2,033.8 / 979.2 | 51.9% | 69.145 / 143.782 |
| `cache_flat_64` | 561.3 | 425.4 | 1,234.3 / 922.6 | 25.3% | 114.015 / 150.450 |
| `one_shot_blocks_64` | 1,030.9 | 500.4 | 2,264.9 / 1,100.9 | 51.4% | 62.084 / 127.900 |
| `cache_blocks_64` | 513.7 | 431.1 | 1,130.1 / 948.3 | 16.1% | 124.593 / 148.473 |
| `one_shot_varied_64` | 2,377.7 | 1,616.4 | 5,221.7 / 3,513.3 | 32.7% | 26.916 / 39.594 |
| `cache_varied_64` | 937.1 | 1,036.1 | 2,052.2 / 2,271.3 | -10.7% | 68.295 / 61.768 |
| `one_shot_flat_4096` | 28,015.6 | 15,617.2 | 60,568.3 / 34,238.6 | 43.5% | 146.204 / 262.275 |
| `cache_flat_4096` | 26,704.7 | 15,376.6 | 58,555.1 / 33,727.5 | 42.4% | 153.381 / 266.379 |
| `one_shot_blocks_4096` | 68,225.0 | 76,632.8 | 149,606.4 / 167,053.2 | -11.7% | 60.037 / 53.450 |
| `cache_blocks_4096` | 29,126.6 | 20,032.8 | 63,884.8 / 43,951.5 | 31.2% | 140.628 / 204.465 |
| `one_shot_varied_4096` | 377,604.7 | 199,651.6 | 826,194.6 / 437,662.4 | 47.0% | 10.847 / 20.516 |
| `cache_varied_4096` | 92,176.6 | 95,595.3 | 201,820.0 / 209,440.7 | -3.8% | 44.436 / 42.847 |
| `cached_mesh_64` | 1,215.6 | 637.1 | 2,670.9 / 1,401.0 | 47.5% | 52.648 / 100.454 |
| `reusable_cold_64` | 788.9 | 671.3 | 1,734.1 / 1,476.1 | 14.9% | 81.129 / 95.339 |
| `reusable_steady_64` | 449.2 | 445.9 | 976.6 / 981.3 | -0.5% | 142.470 / 143.530 |
| `reusable_growing_64` | 1,051.4 | 929.1 | 2,299.2 / 2,035.1 | 11.5% | 100.821 / 114.089 |
| `reusable_shared_64` | 1,034.6 | 673.4 | 2,273.5 / 1,471.8 | 35.3% | 61.861 / 95.035 |
| `cached_mesh_4096` | 119,809.4 | 66,767.2 | 261,585.7 / 146,300.2 | 44.1% | 34.188 / 61.347 |
| `reusable_cold_4096` | 92,964.1 | 61,740.6 | 203,850.3 / 135,400.6 | 33.6% | 44.060 / 66.342 |
| `reusable_steady_4096` | 48,573.4 | 50,134.4 | 105,418.3 / 110,003.8 | -4.3% | 84.326 / 81.700 |
| `reusable_growing_4096` | 105,035.9 | 82,698.4 | 228,660.7 / 179,410.4 | 21.5% | 63.388 / 80.509 |
| `reusable_shared_4096` | 88,246.9 | 61,710.9 | 193,256.0 / 134,618.7 | 30.3% | 46.415 / 66.374 |

Allocation/free counts and requested/freed bytes match within each owning
operation. No workload adds allocation churn. Cache construction and
cached/reusable mesh storage counts are unchanged. One-shot counts are:

| Workload | Allocations/frees old/new | Reallocations old/new | Requested/freed bytes old/new |
|---|---:|---:|---:|
| `one_shot_flat_64` | 3 / 2 | 1 / 1 | 2,008 / 1,920 |
| `one_shot_blocks_64` | 3 / 2 | 1 / 1 | 2,776 / 2,592 |
| `one_shot_varied_64` | 3 / 2 | 0 / 0 | 12,352 / 10,776 |
| `one_shot_flat_4096` | 3 / 2 | 1 / 1 | 98,776 / 98,688 |
| `one_shot_blocks_4096` | 3 / 2 | 1 / 1 | 196,312 / 184,032 |
| `one_shot_varied_4096` | 3 / 2 | 0 / 0 | 786,496 / 688,152 |

Higher cycle medians occurred in these cases:

- `cache_varied_64`: 10.7% more cycles; 937.1 -> 1,036.1 ns/op.
- `one_shot_blocks_4096`: 11.7% more cycles; 68,225.0 -> 76,632.8 ns/op.
- `cache_varied_4096`: 3.8% more cycles; 92,176.6 -> 95,595.3 ns/op.
- `reusable_steady_64`: 0.5% more cycles; 449.2 -> 445.9 ns/op.
- `reusable_steady_4096`: 4.3% more cycles; 48,573.4 -> 50,134.4 ns/op.

Large repeated-block one-shot graphs remain a performance tradeoff despite
their lower allocation churn. The original shrink step is retained to
release spare column capacity before mesh allocation. Fewer allocations do
not guarantee less CPU time for every graph shape. No universal speedup is claimed.

## Workloads and storage limits

Builder fixtures use 0, 1, 64 or 4,096 measures. Flat density is 8; blocks
cycle 0/8/12/16 in runs of 16 measures; varied density is
`(index * 17) % 31 + 1`. Timestamps increase by one second, the peak is 32,
the graph is 854 by 64 pixels, desaturation is 0.5 and alpha is 0.65.
The graph's end time equals the measure count. Builders use a full visible
window. Iterations per timing sample are 512 for up to 64 inputs, otherwise
64. Throughput counts input measures, or one operation for empty inputs.

Cached/reusable workloads use the varied fixtures at 64 and 4,096 measures.
Full graphs contain one segment per measure, including the closing column.
Steady updates overwrite the same full window. Growing updates process
widths 106.75, 427 and 854 in sequence, repeatedly shrinking and growing a
buffer with sufficient capacity. Throughput counts actual segment writes
across all three windows. Clipped windows exercise endpoint interpolation.
These are scaling and compatibility fixtures, not a measured distribution
of real charts or of how often gameplay shares a previous frame.

One-shot output still needs owned storage: two allocation/free calls, plus
the existing shrinking reallocation for compressed columns. The temporary
column reservation is at most `input measures + 1`; it is shrunk before
allocating exactly six vertices per visible segment. Persistent caches
remain immutable, chart-scoped `Arc` columns. Reusable vectors retain their
high-water capacity until the graph is cleared or released; cold, shared or
capacity-exceeding updates can allocate. Empty windows still clear the mesh.
There is no new global state, unbounded cache or unsafe production code.
Requested bytes measure allocator churn, not RSS or peak live process memory.
This pass does not claim measured reductions in RSS or frame latency.

## Behavior and validation

Five new tests compare the public implementation with the frozen parent:

- Every vertex position and color, cache presence and one-shot/cached
  equivalence across empty/small/large graphs, plateaus, zero prefixes,
  rounded-height ties, duplicate timestamps, clipping, missing/short time
  arrays, invalid dimensions, and nonfinite density/color inputs.
- Finite values, infinities and signed zeros match float bits. Arithmetic
  NaNs match by classification; their sign/payload is not asserted.
- Reusable and fixed meshes preserve vertices through growth, shrinkage,
  empty windows, re-entry and initial buffers ending within a segment.
- Retained previous frames remain unchanged, and weak ownership forces
  replacement just as shared strong ownership does.
- One-shot graphs allocate/free fewer buffers and bytes than the parent
  without adding reallocations. Warmed growing/shrinking updates have zero
  allocations, reallocations, frees or requested bytes and retain capacity.

Validation commands and their results:

- New integration suite: **5 passed**, one manual benchmark ignored, in
  both debug and release.
- Theme library: **1,279 passed**, five pre-existing ignored tests, serial.
- Strict theme performance Clippy found the pre-existing `large_enum_variant`
  at `src/effects.rs:928`. That entire file matches the parent. A second run
  with performance warnings enabled and JSON diagnostics audited found
  exactly that warning and no findings in changed code. No lint allowances
  were added to source.
- `cargo check -p deadsync`: passed.
- Scoped rustfmt, `git diff --check` and the frozen-baseline audit: passed.
- Version audit: exactly 0.5.1210 -> 0.5.1211 in Cargo.toml and the three
  corresponding Cargo.lock entries. No dependency changes.

## Reproduce

```powershell
cargo test -p deadsync-theme-simply-love --test density_build
cargo test -p deadsync-theme-simply-love --release --test density_build
cargo test -p deadsync-theme-simply-love --lib -- --test-threads=1
cargo clippy -p deadsync-theme-simply-love --lib --test density_build --no-deps -- -A clippy::all -W clippy::perf
cargo check -p deadsync
cargo test -p deadsync-theme-simply-love --release --test density_build benchmark_density_build -- --ignored --test-threads=1 --nocapture
```

Repeat the benchmark five times, setting `DEADSYNC_PERF_REVERSE=1` for runs
2 and 4 and removing it for runs 1, 3 and 5. The existing allocator/timer in
`tests/support/perf.rs` supplies counters. Non-Windows runs report zero for
unavailable thread cycles; those zeros are not measured CPU results.
