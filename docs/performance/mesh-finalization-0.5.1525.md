# Borrow mesh payloads during finalization

Baseline: `c901763a1`.

`finish_frame` previously took the first untextured mesh payload out of its slot,
copied its transformed vertices into the frame, and dropped the payload. Other
meshes in the same batch were already borrowed. The first mesh is now borrowed
too, avoiding the ownership move and slot clearing. The existing final
`builder.clear()` releases all source references before finalization returns.

The transform remains a local value, as in the baseline. Borrowing the transform
directly was also tested, but slowed the large-mesh benchmark by about 8-10% on
this compiler/CPU. Keeping the local transform removes that consistent
regression while retaining the benefit for small separate mesh runs.

There is no new cache, storage, floating-point calculation, or batching rule.
The first source reference now drops at final cleanup rather than after its
individual batch. No source reference survives into the next composition call;
reusable source buffers regain the same ownership on return.

## Behavior checks

The integration test compares production finalization with a frozen copy from
the baseline commit. All **850 frame comparisons** pass, including exact
floating-point bits and exact draw commands:

- 0, 1, 2, 17, and 128 meshes; 0, 1, 2, 3, 4, 6, and 63 vertices per mesh.
- Separate batches, groups of four, and a single batch; layer, blend, and camera
  boundaries; incomplete triangles and empty inputs.
- Shared slices and reusable vectors; the translation/reflection fast path and
  general transforms; sprite-run tracking enabled and disabled.
- Signed zero, minimum positive normal, infinities, and NaN in transforms/tints.
- Source reference counts and cleared builder storage after finalization.

The two new comparison tests passed against the unchanged baseline before the
production edit. The full release presentation suite passes. Debug validation
passes all 171 library tests and 135 tests in the new integration target (which
also includes existing source-module tests). Clippy passes with performance
lints denied; existing unrelated warnings remain.

```powershell
cargo test --locked --release -p deadlib-present
cargo test --locked -p deadlib-present --lib --test mesh_finalization
cargo clippy --locked -p deadlib-present --lib -- -D clippy::perf
```

These checks compare backend-visible frame data. No GPU pixel capture was run.

## Before/after measurements

Windows x86-64, Intel Xeon E5-2696 v4, Rust 1.98.1 / LLVM 22.1.8, repository
release profile with LTO. Three paired runs pinned to logical CPU 6, reversing
variant order in run 2. No builds or other tests from this pass ran during the
measurements. The actual finalizer and frozen baseline run through the same
black-boxed function-pointer call in one executable.

Each operation prepares and finalizes 256 mesh payloads. Input headers and
payloads are restored from a template, existing output vectors are cleared,
then `finish_frame` transforms vertices, builds commands, and releases sources.
Preparation and source-reference cleanup are inside timing. Actors' source
geometry stays alive outside timing. Both variants reuse the same preallocated
fixture, including its source and output addresses, after three warmup calls.
Post-benchmark assertions check vertex counts, command counts, and cleanup.

Seven batches contain 8,192 operations each, or 512 for the large-mesh case.
Triangles contain three vertices; large meshes contain 192. Separate batches
change layer/blend/camera between meshes. Groups contain four meshes, and the
single-batch control contains all 256. Fast transforms use the existing
translation/reflection path with unity tint. General transforms rotate meshes
and apply non-unity tint.

Times below are medians of run medians. Cycle reductions are medians of paired
run ratios, so they need not equal ratios of displayed times. Positive values
mean fewer cycles. All times cover one 256-mesh operation.

| Fixture | Before ns/op | After ns/op | Cycle reduction | Paired range |
| --- | ---: | ---: | ---: | ---: |
| Empty meshes, shared | 7,714.5 | 7,242.8 | 6.5% | 6.1% to 9.3% |
| Empty meshes, reusable | 7,645.2 | 7,168.4 | 6.3% | 5.5% to 15.2% |
| Separate triangles, shared | 12,794.7 | 10,754.3 | 15.4% | 14.4% to 18.3% |
| Separate triangles, reusable | 12,951.5 | 10,864.8 | 16.1% | 14.6% to 16.6% |
| General triangles, shared | 12,366.7 | 10,866.3 | 12.1% | 9.2% to 21.2% |
| General triangles, reusable | 11,332.4 | 11,037.5 | 6.2% | 0.9% to 10.3% |
| Groups of four, shared | 10,243.1 | 9,831.6 | 5.1% | -5.3% to 7.9% |
| Groups of four, reusable | 10,330.7 | 10,249.3 | 0.8% | -2.8% to 1.7% |
| Single batch, shared | 9,987.3 | 9,648.4 | 3.3% | 2.9% to 13.4% |
| Single batch, reusable | 10,195.6 | 9,744.0 | 4.1% | -3.4% to 4.5% |
| Large meshes, shared | 116,961.9 | 116,538.5 | 0.3% | -1.3% to 1.9% |
| Large meshes, reusable | 125,576.2 | 124,304.9 | 2.4% | -1.2% to 3.2% |

Separate-triangle throughput increases 18.2% for shared slices and 19.2% for
reusable vectors. Gains shrink as more vertices share one batch. No reliable
gain is claimed for grouped or large meshes. All cases retain zero allocations,
reallocations, and frees per operation; allocation accounting is separate from
timing and excludes fixture construction/destruction.

These are CPU microbenchmarks of frame preparation/finalization, not full actor
composition, game-frame, or GPU timings. Cycle counts use `QueryThreadCycleTime`
and include harness/clock overhead equally in both variants. The
[raw CSV](mesh-finalization-0.5.1525.csv) retains all 72 final measurements,
including timing ranges, less favorable results, and allocation counts.
Throughput units are input meshes per second.

## Reproduce

```powershell
cargo test --locked --release -p deadlib-present --test mesh_finalization --no-run
(Get-Process -Id $PID).ProcessorAffinity = 64
# Run the executable path printed by Cargo:
# <executable> benchmark_mesh_finalization --ignored --nocapture --test-threads=1
# Repeat three times; set DEADSYNC_PERF_REVERSE=1 for the middle run.
```

Capture stderr directly to a file. Final logs are retained locally as
`target/mesh-finalization-pass/sharedfixture1.txt` through `sharedfixture3.txt`.
Earlier experiments used fresh fixtures or separate allocations per variant;
the final measurements use the same warmed allocations for both variants.
