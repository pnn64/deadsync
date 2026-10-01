# Depth-mask preparation performance 0.5.1660

Parent: `0365e65e1` (0.5.1659). Date: 2026-10-01.

Three optimizations apply `rust-performance.md`'s M-HOTPATH, M-MEM-REUSE and
M-THROUGHPUT guidance to noteskin depth-mask compilation:

1. Compare atlas alpha patterns through contiguous RGBA row slices. The old
   validator obtained coordinates, performed two pixel lookups and calculated
   two remainders per pixel. The new validator calculates the reference row
   once per image row and compares only zero/nonzero alpha across cell slices.
   It checks the first pixel before setting up iteration, then skips the
   selected cell's self-comparison. RGB differences and distinct
   nonzero alpha values remain valid; only alpha=0 leaves the mask open.
   The matching 8x4 atlas fixture uses 88.1% to 89.1% fewer
   CPU cycles, with zero allocations in both implementations.
2. Merge transparent runs through sorted previous-row frontiers instead of
   repeatedly searching accumulated rectangle history. Each frontier advances
   once from left to right. Rectangle birth order and vertical merge rules
   remain exact, so triangulation and draw order match the parent. Each row
   normally uses 64 stack indices; two frontiers occupy 1024 bytes on this
   target. More fragmented rows use one reusable, interleaved spill buffer,
   with at most 4096 bytes initially reserved and growth for larger frontiers.
   The 64x64 checker fixture uses 97.9% to 98.1% fewer
   CPU cycles in isolated rectangle extraction.
3. Collect each rectangle's fixed six-vertex array directly into its final
   Arc. The installed Rust standard library specializes this known-length
   iterator to one output allocation. The old path allocated a vertex Vec,
   filled it, then allocated and copied into the Arc. Empty masks share a lazy
   immutable empty vertex array after first use. The small six-vertex fixture
   uses 52.0% to 55.6% fewer CPU cycles;
   owning emission removes one allocation and its matching free. Large isolated
   emission has order-dependent timings, with memory/allocator traffic savings
   but no consistent CPU improvement established by this benchmark.

No dependencies or unsafe production code were added. Mask eligibility,
texture cropping and divisibility checks, depth-command order, invisible
sprite removal, mesh sharing, full-canvas bounds and every emitted vertex
field retain the parent's behavior. The asset-path test initializer is made
visible to these sibling tests so both suites use the same Once initialization.

These are loading/compilation operations. The benchmarks isolate in-memory
validation, extraction, emission and complete mesh construction; PNG decoding,
filesystem lookup, GPU upload, gameplay frame rate, process RSS and peak live
memory were not measured. Requested/freed bytes measure allocator traffic.
The shared empty array retains one 16-byte Arc header for process lifetime on
this target; its first allocation is outside warmed measurement.

## Measurement method

The [raw CSV](mask-preparation-0.5.1660.csv) retains 192 measurements:
16 workloads, two implementations and six serial rounds alternating
old-first/new-first. Each median uses seven timing batches; batch elapsed-time
minima and maxima remain in the CSV. Windows QueryThreadCycleTime counts
calling-thread CPU cycles. Allocation counting uses the existing System
allocator wrapper in a separate operation, disabled during timing.

Environment: Windows x64, Intel Xeon E5-2696 v4 at 2.20 GHz, 22 cores / 44 logical
processors, Rust 1.98.1, LLVM 22.1.8. Release uses opt-level 3 and full LTO with
test unwinding. After compilation and checks finish, each test executable runs
on logical processor 6 (affinity mask 64), with --test-threads=1. The runner
verifies process affinity. Inputs and shared empty storage are warmed first.

One operation produces and destroys one validation result, rectangle list or
mesh. All groups report operations/s in the CSV's units_per_second field.
Allocation accounting includes output destruction. Emission alone also clones
its input rectangle Vec equally in both implementations. Every operation
balances requested and freed bytes, including reallocation traffic.

Four functions are frozen exactly from the parent in
`crates/deadsync-assets/tests/mask_preparation/baseline.rs`: depth-command
parsing, mask application, PNG mask loading and complete cutout construction.
Three isolated stages are extracted from those unchanged bodies: the old alpha
predicate, rectangle construction and vertex emission. An audit checks all
four frozen bodies and all three extracted stages against the parent, confirms
depth/application logic is unchanged, and verifies that loading changes only
its alpha predicate call. Image/script/source types and metadata are shared.

15 of 16 workloads improve CPU cycles and throughput in every
round. All results are retained. Workloads without consistent improvement in
both metrics: `mask_vertices_fragmented`.
Workloads with higher CPU cost in every round: none.
Negative reductions indicate higher measured cost. The early-rejection control
does only enough work to find the first mismatched alpha byte.

## Alpha-pattern validation

Single is one 64x64 cell and exercises self-comparison elimination. Atlas and
colorful use an 8x4 sheet of 32x32 cells. They share the same zero/nonzero alpha
pattern, while colorful varies RGB and all opaque alpha values. Early/late
mismatch toggle the first/last image pixel's zero/nonzero alpha classification,
using the unchanged second-column/second-row cell as reference. Batches contain
512 operations. All validation operations retain zero allocator churn.

| Workload | ns/op, old -> new (round 1) | Thread cycles/op, old -> new (round 1) | CPU reduction, six rounds | Throughput gain, six rounds | Alloc/realloc/free, old -> new | Requested bytes, old -> new |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `mask_cells_single` | 32,372.9 -> 610.5 | 70,979.1 -> 1,343.2 | 98.0% to 98.3% | 4828.4% to 5814.3% | 0/0/0 -> 0/0/0 | 0 -> 0 |
| `mask_cells_atlas` | 258,199.2 -> 29,855.9 | 565,718.0 -> 64,691.6 | 88.1% to 89.1% | 743.0% to 817.6% | 0/0/0 -> 0/0/0 | 0 -> 0 |
| `mask_cells_colorful` | 265,520.1 -> 29,764.1 | 582,077.1 -> 65,250.7 | 87.0% to 88.8% | 672.0% to 792.4% | 0/0/0 -> 0/0/0 | 0 -> 0 |
| `mask_cells_early_mismatch` | 12.1 -> 4.9 | 28.7 -> 12.9 | 55.1% to 57.6% | 148.0% to 160.0% | 0/0/0 -> 0/0/0 | 0 -> 0 |
| `mask_cells_late_mismatch` | 266,373.2 -> 29,328.3 | 583,648.4 -> 64,271.1 | 88.5% to 89.3% | 765.9% to 835.4% | 0/0/0 -> 0/0/0 | 0 -> 0 |

## Rectangle extraction

Hole64 has one transparent central 32x32 rectangle. Stripes64 has 16 vertical
transparent stripes. Checker64 alternates alpha per pixel across a 64x64 mask,
producing 2048 rectangles without vertical merges. Wide256 is a 256x8 checker
with 1024 rectangles and 128 runs per row, exercising the spill buffer. Batches
contain 256, 128, 16 and 32 operations respectively.

| Workload | ns/op, old -> new (round 1) | Thread cycles/op, old -> new (round 1) | CPU reduction, six rounds | Throughput gain, six rounds | Alloc/realloc/free, old -> new | Requested bytes, old -> new |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `mask_rects_hole64` | 4,659.8 -> 2,255.9 | 10,220.5 -> 4,956.8 | 43.3% to 51.7% | 76.3% to 107.5% | 1/0/1 -> 1/0/1 | 64 -> 64 |
| `mask_rects_stripes64` | 10,862.5 -> 7,843.0 | 23,827.8 -> 17,189.6 | 24.5% to 39.4% | 32.1% to 65.4% | 1/2/1 -> 1/2/1 | 448 -> 448 |
| `mask_rects_checker64` | 928,568.8 -> 17,987.5 | 2,034,696.4 -> 39,660.9 | 97.9% to 98.1% | 4704.4% to 5138.7% | 1/9/1 -> 1/9/1 | 65472 -> 65472 |
| `mask_rects_wide256` | 183,859.4 -> 27,637.5 | 403,296.9 -> 60,774.1 | 83.7% to 85.9% | 515.3% to 608.3% | 1/8/1 -> 2/8/2 | 32704 -> 33728 |

The wide isolated extraction adds one spill allocation, while ordinary
extraction retains the original rectangle Vec's allocation/growth counts.
That spill trades a small amount of scratch memory for avoiding rectangle
history scans. Complete wide mesh construction removes the much larger vertex
staging buffer, so its allocation count stays level and total requested bytes
fall. Frontiers larger than the initial reservation can grow their buffer;
the 641-pixel-wide behavior fixture tests that growth path.

## Vertex emission

Small emits one rectangle's six vertices. Fragmented emits 512 rectangles
(3072 vertices). Empty emits no vertices and retains full-canvas bounds. Input
rectangle cloning and mesh destruction are included equally in both variants.
Batches contain 512 operations. A counted test also moves an existing input
buffer into the new emitter and verifies that only its final output allocates.

| Workload | ns/op, old -> new (round 1) | Thread cycles/op, old -> new (round 1) | CPU reduction, six rounds | Throughput gain, six rounds | Alloc/realloc/free, old -> new | Requested bytes, old -> new |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `mask_vertices_small` | 349.0 -> 156.2 | 768.7 -> 345.1 | 52.0% to 55.6% | 109.0% to 125.3% | 3/0/3 -> 2/0/2 | 512 -> 272 |
| `mask_vertices_fragmented` | 42,427.3 -> 29,102.0 | 92,981.8 -> 63,780.2 | -34.5% to 31.4% | -25.7% to 45.8% | 3/0/3 -> 2/0/2 | 253968 -> 131088 |
| `mask_vertices_empty` | 77.9 -> 24.4 | 175.8 -> 56.2 | 65.3% to 69.7% | 197.9% to 237.9% | 1/0/1 -> 0/0/0 | 16 -> 0 |

For fragmented emission, old-first rounds show CPU reductions of
23.9% to 31.4%; new-first rounds show
-34.5% to 0.4%. Allocation counts and bytes remain
identical across orders: 3 allocations/frees become 2, and requested/freed
bytes fall from 253,968 to 131,088 (48.4%). These measurements establish the
memory-traffic reduction and an order effect in isolated timing; they do not
isolate its cause or establish a consistent CPU improvement for that fixture.

## Complete cutout meshes

Hole64, checker64 and wide256 use the extraction fixtures above and include
all vertex preparation/destruction. Opaque64 has no transparent pixels and
uses the shared empty vertex array. Batches contain 256, 16, 32 and 256
operations respectively. These meshes preserve exact vertex order and bits.

| Workload | ns/op, old -> new (round 1) | Thread cycles/op, old -> new (round 1) | CPU reduction, six rounds | Throughput gain, six rounds | Alloc/realloc/free, old -> new | Requested bytes, old -> new |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `mask_full_hole64` | 5,634.0 -> 2,409.4 | 12,354.6 -> 5,293.7 | 51.7% to 60.1% | 106.5% to 152.1% | 3/0/3 -> 2/0/2 | 560 -> 320 |
| `mask_full_checker64` | 1,370,468.8 -> 134,900.0 | 3,003,583.2 -> 295,913.4 | 89.1% to 90.7% | 817.0% to 981.2% | 3/9/3 -> 2/9/2 | 1048528 -> 557008 |
| `mask_full_wide256` | 258,778.1 -> 67,159.4 | 566,673.6 -> 147,298.2 | 72.5% to 74.3% | 263.3% to 288.8% | 3/8/3 -> 3/8/3 | 524240 -> 279504 |
| `mask_full_opaque64` | 5,827.7 -> 2,286.7 | 12,788.4 -> 5,023.6 | 59.0% to 67.8% | 144.5% to 210.0% | 1/0/1 -> 0/0/0 | 16 -> 0 |

## Behavior and validation

Seven new ordinary tests cover all 65,536 four-by-four alpha masks at a nonzero
image origin, comparing rectangle coordinates and exact order. Larger masks
exercise empty rows, long-lived rectangles, holes, stripes, irregular patterns,
64/65-run stack boundaries, spill reuse and spill growth. Mesh comparisons
check vertex fields and bounds by f32 bits, including zero, negative and extreme
canvas sizes. Alpha validation checks RGB differences, alpha=1, early/late
mismatches, aligned/unaligned origins, empty images and unused trailing storage.

PNG-loading comparisons cover valid crops, negative/zero/out-of-bounds regions,
divisibility rejection, alpha disagreement, custom UVs, model and moving-texture
rejection. Complete application comparisons check NoEffect removal, depth clear
order, filter eligibility, rotation compatibility and shared retained meshes.
The existing nested compiled/cache noteskin cutout integration test also passes.
Allocation assertions verify direct output construction, shared empty output,
zero-churn alpha validation and reduced complete owning churn for ordinary masks.

Validation commands:

```powershell
cargo test -p deadsync-assets --lib mask_preparation --locked --offline
cargo test -p deadsync-assets -p deadsync-noteskin --lib --release --locked --offline
cargo check --workspace --locked --offline
cargo clippy -p deadsync-assets --lib --locked --offline -- -D clippy::perf -A clippy::large_enum_variant
rustfmt --edition 2024 --check crates/deadsync-assets/src/noteskin/mask.rs crates/deadsync-assets/src/noteskin/mod.rs
git diff --check
```

The release suites pass 187 asset tests and 321 noteskin tests (508 total), with
25 manual performance tests ignored. Workspace checking, performance lints,
formatting and frozen/extracted-source auditing pass. This pass uses complete
library suites and the standard workspace check; the previously reported
safe_parsing integration-target error is unchanged and --all-targets was not
repeated. Cargo.toml advances exactly once from 0.5.1659 to 0.5.1660; Cargo.lock
updates the three workspace-version packages. All four excluded files remain
outside this commit.
