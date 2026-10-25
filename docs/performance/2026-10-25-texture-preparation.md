# Texture preparation performance pass

Date: 2026-10-25. Starting commit: `551c81c23`. Branch: `optimize/texture-kernels-20261025`.

## Three optimizations

1. **Packed luminance.** Read and compose complete RGBA words in safe Rust instead of extracting pixel wrappers and assigning byte channels. Integer coefficients, division, alpha preservation and alphamap precedence are unchanged. Release assembly processes four pixels per SSE2 iteration (`pand`, `paddd`, `pmuludq` and shifts). No CPU feature requirement was added.
2. **Stream vertical resize weights.** Return coordinate weights as an iterator, collecting only the horizontal weights reused on every row. Vertical weights flow directly into row interpolation. Remove one vector allocation, its materialization pass and subsequent traversal per resize step, preserving float operation order and fixed-point interpolation. The four-step resize reduces allocations from 13 to 9, including the benchmark input clone.
3. **Borrow PNG metadata.** Use the reader's palette/transparency slices instead of cloning them before decoding. Indexed palettes are installed before IDAT, duplicate palettes are rejected, and indexed tRNS after IDAT is rejected by png 0.18.1. These values cannot change during a successful first-frame decode. Nonindexed paths never use the copies. Remove one or two allocations per decode, including 1,024 requested bytes for indexed PNGs with transparency.

The production algorithms remove **three net lines** (34 added / 37 removed), excluding three lines registering tests. No new production types, caches, dependencies, unsafe blocks, version changes or root manifest/lockfile changes. Two historical benchmark lines explicitly collect the now-lazy helper to retain the old materialized inputs. `preparation_original.rs` freezes the starting implementation for differential tests.

## Measurement

Rustc 1.99.1 (58cf8f9c3), LLVM 23.1.1, x86_64-pc-windows-msvc, Intel Xeon E5-2696 v4. Release opt-level 3, LTO disabled identically; repository static CRT retained; no native CPU flags. Each benchmark pins only its own thread to logical CPU 2 without changing process priority.

Windows QueryThreadCycleTime measures consumed thread CPU cycles. Instant nanoseconds are secondary data. Each process runs 12 batches, discards its first, and reports the middle of the remaining 11 sorted by cycles. The starting/final production sources were separately compiled with identical benchmark sources. Four process pairs alternate order. Tables report median process medians and the range of paired CPU throughput ratios; above 1 is faster. Function pointers and inputs are black-boxed. Fixed call/measurement overhead is included. These are microbenchmarks, not whole-game frame-rate measurements.

Luminance repeatedly transforms one preallocated image; arithmetic/hint selection have no pixel-dependent branches. Image creation/cloning/destruction are excluded. Resize includes input cloning, all resize steps and output destruction. PNG includes opening/reading a cached file and output destruction; fixture generation is excluded. Separate allocation probes use the same boundaries and the existing thread-local System wrapper. Requested bytes include realloc requests, not allocator bookkeeping or RSS. Final PNG fixture paths use fixed-width process IDs to prevent path-length allocation noise.

## Complete CPU results

| Workload | Before cycles/op | After cycles/op | CPU throughput | Paired range |
| --- | ---: | ---: | ---: | --- |
| `luma/alpha/1` | 10.77 | 12.03 | 0.896x | 0.474-1.201 |
| `luma/alpha/1024` | 3,381,877.75 | 1,779,733.44 | 1.900x | 1.821-2.083 |
| `luma/alpha/16` | 836.21 | 413.82 | 2.021x | 1.722-2.723 |
| `luma/alpha/256` | 205,243.50 | 106,987.95 | 1.918x | 1.668-2.277 |
| `luma/both/1` | 10.79 | 12.14 | 0.889x | 0.462-1.046 |
| `luma/both/1024` | 3,362,541.12 | 1,727,492.44 | 1.946x | 1.804-2.191 |
| `luma/both/16` | 885.19 | 418.11 | 2.117x | 1.874-2.230 |
| `luma/both/256` | 214,733.20 | 105,491.70 | 2.036x | 1.916-2.120 |
| `luma/gray/1` | 11.23 | 13.96 | 0.804x | 0.438-1.099 |
| `luma/gray/1024` | 3,464,443.84 | 2,381,698.31 | 1.455x | 1.395-1.646 |
| `luma/gray/16` | 865.54 | 608.92 | 1.421x | 0.959-1.640 |
| `luma/gray/256` | 206,692.18 | 147,847.89 | 1.398x | 1.294-1.648 |
| `luma/none/1` | 4.55 | 7.13 | 0.638x | 0.382-0.792 |
| `luma/none/1024` | 4.42 | 6.90 | 0.641x | 0.585-0.694 |
| `luma/none/16` | 4.65 | 6.76 | 0.688x | 0.334-0.767 |
| `luma/none/256` | 4.30 | 6.60 | 0.652x | 0.601-0.710 |
| `png/indexed=false/trns=false/16` | 296,903.40 | 301,216.55 | 0.986x | 0.954-1.004 |
| `png/indexed=false/trns=false/256` | 858,003.57 | 796,135.27 | 1.078x | 0.985-1.295 |
| `png/indexed=false/trns=true/16` | 233,927.73 | 221,568.78 | 1.056x | 0.971-1.134 |
| `png/indexed=false/trns=true/256` | 1,074,215.44 | 1,124,655.43 | 0.955x | 0.948-1.037 |
| `png/indexed=true/trns=false/16` | 216,738.68 | 214,365.90 | 1.011x | 1.004-1.026 |
| `png/indexed=true/trns=false/256` | 466,435.29 | 452,360.96 | 1.031x | 0.986-1.203 |
| `png/indexed=true/trns=true/16` | 221,314.17 | 215,276.83 | 1.028x | 0.997-1.060 |
| `png/indexed=true/trns=true/256` | 481,184.61 | 449,375.77 | 1.071x | 0.988-1.159 |
| `zoom/1024x512/64x32` | 5,263,150.38 | 5,169,660.56 | 1.018x | 0.996-1.078 |
| `zoom/16x16/32x32` | 23,928.93 | 21,624.17 | 1.107x | 1.042-1.168 |
| `zoom/1x256/2x512` | 35,493.85 | 30,222.41 | 1.174x | 1.154-1.498 |
| `zoom/256x256/128x128` | 423,422.33 | 382,890.33 | 1.106x | 1.019-1.319 |
| `zoom/256x256/512x512` | 5,956,959.11 | 5,791,408.07 | 1.029x | 1.021-1.170 |

One-pixel and no-hint luminance calls are slower by a few CPU cycles because the packed loop saves more registers in its prologue; all controls remain in the table. Larger luminance cases improve consistently. Tall/narrow resizing also improves consistently. Several other resize timings overlap noise despite deterministic allocation savings. PNG decode/file-I/O costs dominate the removed copies, so no general PNG CPU speedup is established. PNG is retained for eliminating allocations and copying. Lower pooled medians and pair ranges crossing 1.0 are visible above; no universal speedup is claimed.

## Allocations

All 16 luminance cases have zero allocation churn before/after. Other cases include output destruction, so frees equal allocations. Reallocation counts do not change. Counts and requested/peak bytes are identical across all four final runs for each variant.

| Workload | Allocations before/after | Reallocations | Requested bytes before/after | Peak bytes before/after |
| --- | ---: | ---: | ---: | ---: |
| `png/indexed=false/trns=false/16` | 11 / 10 | 4 | 42,278 / 41,510 | 40,184 / 39,416 |
| `png/indexed=false/trns=false/256` | 11 / 10 | 4 | 785,702 / 784,934 | 686,072 / 685,304 |
| `png/indexed=false/trns=true/16` | 13 / 11 | 4 | 41,527 / 40,756 | 39,433 / 38,662 |
| `png/indexed=false/trns=true/256` | 13 / 11 | 4 | 589,111 / 588,340 | 489,481 / 488,710 |
| `png/indexed=true/trns=false/16` | 11 / 10 | 4 | 41,254 / 40,486 | 39,672 / 38,904 |
| `png/indexed=true/trns=false/256` | 11 / 10 | 5 | 590,118 / 589,350 | 489,976 / 489,208 |
| `png/indexed=true/trns=true/16` | 13 / 11 | 4 | 41,766 / 40,742 | 40,184 / 39,160 |
| `png/indexed=true/trns=true/256` | 13 / 11 | 5 | 590,630 / 589,606 | 490,488 / 489,464 |
| `zoom/1024x512/64x32` | 13 / 9 | 0 | 2,810,752 / 2,804,992 | 2,630,656 / 2,627,584 |
| `zoom/16x16/32x32` | 4 / 3 | 0 | 5,888 / 5,504 | 5,888 / 5,504 |
| `zoom/1x256/2x512` | 4 / 3 | 0 | 11,288 / 5,144 | 11,288 / 5,144 |
| `zoom/256x256/128x128` | 4 / 3 | 0 | 330,752 / 329,216 | 330,752 / 329,216 |
| `zoom/256x256/512x512` | 4 / 3 | 0 | 1,323,008 / 1,316,864 | 1,323,008 / 1,316,864 |

## Isolated before/after checks

Each change was benchmarked immediately before and after implementation, before proceeding to the next. All 58 initial observations are retained in [the stage TSV](2026-10-25-texture-preparation-stages.tsv). Initial single-process examples:

- 256px alpha-map: 223,985.10 -> 108,008.63 cycles, zero allocations.
- Tall 1x256 -> 2x512 resize: 34,783.91 -> 34,029.34 cycles, 4 -> 3 allocations.
- Transparent indexed 16px PNG: 220,964.07 -> 220,195.81 cycles, 13 -> 11 allocations.

The repeated experiment above supports the final CPU conclusions. Initial no-hint luminance probes used fewer iterations at large sizes; final identical harnesses use 100,000 for every no-hint control. Initial PNG requested bytes can differ by two bytes with process-ID path length; the final fixed-width paths eliminate this artifact.

## Correctness

- Starting and final archived unit binaries: **91 passed, 0 failed, 8 ignored each**.
- Final `cargo test -p deadlib-assets --tests`: **100 passed, 0 failed, 10 ignored** across unit and integration tests.
- Exhaustive exact comparison of all 16,777,216 RGB triples for all four hint combinations, with varying alpha; trailing raw storage/incomplete chunks also match the starting implementation.
- Exact resize comparison for 216 source/destination pairs, including one-pixel axes, odd dimensions and multistep resizes. Existing fixed-point/native preparation tests pass.
- Generated RGB/indexed PNGs with/without transparency, both key settings, and truncations preserve pixels/errors. All **39 native PNG fixtures** match the starting decoder for both key settings, including packed indices, 16-bit sources, interlacing and keying. Existing native bitmap observation and texture cleanup integration tests pass.
- Clippy completes with repository warnings and one warning in the frozen baseline's original `chunks_exact(3)` loop; no new production lint warnings in the changed algorithms.
- Assembly inspection confirms SSE2 packed luminance. All production changes are safe Rust: intermediates are at most 25,500 and explicit little-endian conversion preserves byte layout on other architectures. Platform benchmark FFI and allocator instrumentation are reused unchanged.

No new whole-song native capture was recorded; texture behavior is checked through existing native observations and exact differential tests. Neither excluded Lua crate was edited or included in the commit. Existing checkout edits remain outside this separate worktree. The commit is local; nothing was pushed or published.

## Reproduction and evidence

```powershell
cargo test --release --locked --config profile.release.lto=false -p deadlib-assets --tests
cargo clippy --release --locked --config profile.release.lto=false -p deadlib-assets --lib --tests
cargo test --release --locked --config profile.release.lto=false -p deadlib-assets --lib --no-run
# Archive the executable Cargo prints for each production version, then:
python scripts/benchmark_texture_preparation.py --before target/preparation/before-final.exe --after target/preparation/after-final.exe --output docs/performance/2026-10-25-texture-preparation.tsv --runs 4
```

Build before by combining the final new benchmark/frozen-original files with starting `lib.rs`, `decode.rs` and `texture_pixels_original.rs`; append only the `preparation_perf` test module declaration to starting `decode.rs`. Restore those three final files and rebuild after with identical Cargo options. Use the repository's `tests/fixtures` submodule for native fixtures; this worktree temporarily linked the original fixture data during verification.

The [repeated TSV](2026-10-25-texture-preparation.tsv) contains 232 process medians and allocation probes. Per-batch raw samples are not captured by the reused helper. Ignored local evidence lives under `target/preparation`: archived executables, each process/stage log, baseline/final unit logs, Cargo test/Clippy logs and extracted luminance assembly. Shared Cargo artifacts are under `C:/GitHub/deadsync/target`.

Measured executable SHA-256:

- before: `a8851be992efde7e716fa5fb4d1b6be876ab7c169e411fa2fd1177565db748f5`
- after: `1dc2ed8e3e730eac8c7144f2fa8727fdeb54b135b99459d1ad20947ea2d2ccd2`
