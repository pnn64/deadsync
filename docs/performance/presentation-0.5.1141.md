# Presentation performance, 0.5.1141

Baseline: `9e6ab0553` / 0.5.1140. The patch version increases exactly once to 0.5.1141.

This pass applies `rust-performance.md` guidance M-HOTPATH (measure CPU and memory work), M-MEM-REUSE (retain caller-owned storage), and M-INITIAL-CAPACITY (reserve known sizes). It retains three optimizations:

1. **Reuse neighboring line normals.** The gameplay life graph's mesh builder carries each normalized segment and endpoint into the next join. It preserves the original endpoint calculations, floating-point evaluation order, clipping, miter limits, and transparent fringe vertices. Warm full-length benchmarks exercise this change without allocation or buffer growth.
2. **Append initialized vertices when a retained line grows.** The reusable builder reserves the same maximum capacity, overwrites existing vertices, and appends generated segments directly. It avoids initializing space that will immediately be overwritten or discarded. Fully initialized buffers keep a direct slice-writing path. Ownership checks continue to protect frames held by the renderer. In the 128-to-4,096-point regrowth fixture, this removes 1,714,176 bytes of redundant zero-fill per operation (calculated from vertex counts, not a hardware memory-traffic measurement).
3. **Refresh font caches in existing ASCII tables.** Chain keys and glyph tables are computed one font at a time instead of retaining two intermediate vectors and a newly allocated table for every font. A stack-backed name list handles up to 32 fonts; larger sets reserve exactly one name buffer. The existing tables stay at the same addresses. Unchanged refreshes at or below 32 fonts have no heap churn, while larger sets only allocate and free the name list. Glyph/default/fallback semantics and deterministic tag ordering are unchanged.

The active callers are `refresh_density_graph_meshes_for_player` in the Simply Love gameplay screen and font registration in `deadlib-assets::FontStore` / theme resources. Font refresh savings apply to loading and registration, not every rendered frame. The immutable line API remains covered by parity tests; its allocation policy is unchanged.

## Measurement

Windows x64, Intel Xeon E5-2696 v4 @ 2.20 GHz; rustc 1.98.0 (`88d9e12ae`), LLVM 22.1.8, release `opt-level=3`, LTO enabled. Frozen old function bodies are checked into the integration test and verified against the baseline commit. Both versions run in the same executable through opaque function pointers, with input/output black boxes.

Each row is the median of three runs, each containing seven timing batches after warm-up; the middle run reverses old/new ordering. Line batches contain 200 operations and font batches 100. Windows `QueryThreadCycleTime` measures calling-thread cycles separately from elapsed time. Allocation counters run in a separate operation, so allocator tracking is excluded from timing. Fixture construction and font-map cloning are outside measurements. New-line timing includes destruction; regrowth includes both the small and full update. Compilation and other validation finish before these final measurements.

Throughput units are emitted line segments (both updates for regrowth) or 128 ASCII slots per font. Inputs are deterministic synthetic fixtures. These are operation-level CPU/throughput results, not an end-to-end FPS or resident-memory claim.

| Workload | us/op, old -> new | Thread cycles/op, old -> new | Cycles saved | Million units/s, old -> new |
|---|---:|---:|---:|---:|
| Warm line, 2 points (control) | 0.077 -> 0.062 | 177.8 -> 142.7 | 19.7% | 12.903 -> 16.129 |
| New line, 2 points (control) | 0.589 -> 0.629 | 1,301.6 -> 1,388.3 | -6.7% | 1.698 -> 1.590 |
| Warm line, 256 points | 16.661 -> 13.910 | 36,540.2 -> 30,523.7 | 16.5% | 15.305 -> 18.332 |
| New line, 256 points | 18.758 -> 17.047 | 41,139.8 -> 37,381.9 | 9.1% | 13.595 -> 14.959 |
| Warm line, 4,096 points | 250.679 -> 238.588 | 548,124.4 -> 521,522.1 | 4.9% | 16.336 -> 17.163 |
| New line, 4,096 points | 967.912 -> 772.135 | 2,119,053.0 -> 1,689,200.7 | 20.3% | 4.231 -> 5.303 |
| Retained line, 128 then 4,096 points | 356.773 -> 260.752 | 781,001.9 -> 570,944.8 | 26.9% | 11.834 -> 16.192 |
| Retained line, 4,096 points / 255 emitted segments | 194.133 -> 86.662 | 424,991.5 -> 189,943.2 | 55.3% | 1.314 -> 2.942 |
| Refresh 1 font (control) | 4.808 -> 5.780 | 10,529.4 -> 12,702.5 | -20.6% | 26.622 -> 22.145 |
| Refresh 24 fonts | 174.875 -> 165.628 | 382,944.1 -> 362,754.5 | 5.3% | 17.567 -> 18.548 |
| Refresh 96 fonts | 1,053.257 -> 641.618 | 2,306,295.3 -> 1,406,481.4 | 39.0% | 11.667 -> 19.152 |

Every case had zero reallocations. Allocation and free counts match, as do allocated and freed byte totals, for these steady-state/creation-and-drop fixtures:

| Workload | Allocations and frees/op, old -> new | Allocated and freed bytes/op, old -> new |
|---|---:|---:|
| Warm line, 2 points (control) | 0 -> 0 | 0 -> 0 |
| New line, 2 points (control) | 2 -> 2 | 472 -> 472 |
| Warm line, 256 points | 0 -> 0 | 0 -> 0 |
| New line, 256 points | 2 -> 2 | 110,200 -> 110,200 |
| Warm line, 4,096 points | 0 -> 0 | 0 -> 0 |
| New line, 4,096 points | 2 -> 2 | 1,769,080 -> 1,769,080 |
| Retained line, 128 then 4,096 points | 0 -> 0 | 0 -> 0 |
| Retained line, 4,096 points / 255 emitted segments | 0 -> 0 | 0 -> 0 |
| Refresh 1 font (control) | 4 -> 0 | 11,376 -> 0 |
| Refresh 24 fonts | 27 -> 0 | 271,872 -> 0 |
| Refresh 96 fonts | 99 -> 1 | 1,087,488 -> 1,536 |

Persistent mesh capacity and glyph-table payload sizes are unchanged. Font refresh replaces per-font temporary heap tables with one bounded 128-glyph stack temporary; it does not eliminate glyph reference-count operations. The counting allocator delegates to `System` in this test executable; production allocator configuration is unchanged. Timings can vary with CPU/cache state.

The 256-point warm mesh used 16.5% fewer thread cycles, regrowth 26.9% fewer, and the sparse fixture 55.3% fewer. Refreshing 24 fonts removed all 27 allocations and 271,872 allocated bytes; refreshing 96 fonts cut allocated bytes by 99.86% and thread cycles by 39.0%.

The controls are mixed: creating a two-point line was 6.7% slower in cycles (40 ns more elapsed time), and refreshing a single font was 20.6% slower (0.972 us more), despite removing its four allocations. Allocation-heavy small timings varied considerably across runs. The change is retained for the larger gameplay graphs and multi-font registration sets; it does not claim a speedup for every input. Warm two-point updates improved. The per-run median ranges are recorded below to make the variation visible.

## Behavior and validation

The seven new regression tests compare complete vertex positions/colors by float bits, font tables, resolved glyphs, logical widths, and chain keys against the frozen implementation. They cover empty/single-point inputs, fractional clipping, vertical/reversing joins, repeated/tiny segments, thickness/feather extremes, signed zero, nonfinite reusable geometry, grow/shrink sequences, and shared-frame immutability. Font cases cover missing/replaced glyphs and defaults, missing/removed fallbacks, tag wraparound, resolvable cycles, and 32/33-font spill boundaries. Allocation assertions verify zero churn in retained mesh updates and small font refreshes, and one precisely sized allocation for large font-name lists.

- `cargo test -p deadlib-present --all-targets -- --test-threads=1`: 315 passed, 9 manual benchmarks ignored.
- `cargo test -p deadlib-present --release --lib --test presentation_perf -- --test-threads=1`: 170 passed, 1 manual benchmark ignored.
- `cargo test -p deadlib-assets --lib -- --test-threads=1`: 73 passed, 1 manual benchmark ignored.
- `cargo test -p deadsync-theme-simply-love --lib density -- --test-threads=1`: 16 passed.
- `cargo clippy -p deadlib-present --all-targets -- -D clippy::perf`: passed; non-performance warnings remain.
- `cargo check --workspace --bins`: passed.
- Changed regions/new test files pass rustfmt; `git diff --check` passes. Baseline copies match the old bodies apart from rustfmt whitespace. Source hashes remained unchanged during the three final benchmark runs.

## Reproduce

The old implementations and fixtures live in `crates/deadlib-present/tests/presentation_perf*`; the shared timing/counter helper is `tests/support/perf.rs`. No excluded local files are required to run them.

```powershell
cargo test -p deadlib-present --release --test presentation_perf presentation_bench -- --ignored --nocapture --test-threads=1
$env:DEADSYNC_PERF_REVERSE = '1'
cargo test -p deadlib-present --release --test presentation_perf presentation_bench -- --ignored --nocapture --test-threads=1
Remove-Item Env:DEADSYNC_PERF_REVERSE
cargo test -p deadlib-present --release --test presentation_perf presentation_bench -- --ignored --nocapture --test-threads=1
```

## Run-to-run variation

Ranges of the three per-run median elapsed times (each run already contains seven samples):

| Workload | Old us/op range | New us/op range |
|---|---:|---:|
| `line_warm_2` | 0.077 - 0.104 | 0.060 - 0.069 |
| `line_create_2` | 0.348 - 0.723 | 0.314 - 0.679 |
| `line_warm_256` | 16.558 - 16.703 | 13.384 - 15.307 |
| `line_create_256` | 18.511 - 19.570 | 16.765 - 18.811 |
| `line_warm_4096` | 249.696 - 251.053 | 236.224 - 246.905 |
| `line_create_4096` | 947.914 - 977.609 | 760.241 - 783.636 |
| `line_regrow_4096` | 352.945 - 361.434 | 257.863 - 262.422 |
| `line_sparse_4096` | 191.188 - 199.993 | 84.808 - 87.477 |
| `font_refresh_1` | 4.557 - 4.850 | 5.185 - 6.402 |
| `font_refresh_24` | 169.973 - 176.180 | 156.554 - 167.756 |
| `font_refresh_96` | 691.291 - 1,071.757 | 636.201 - 652.914 |
