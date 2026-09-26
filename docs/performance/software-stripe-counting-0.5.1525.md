# Software stripe counting

Baseline: `f9d68fa34`. `StripeBins::build` previously visited every covered
stripe to count memberships and then visited them all again to insert items.
Counting now records only each range's start and end. The existing prefix scan
recovers coverage counts and item offsets in place. The insertion pass, item
order, storage, and rasterization are unchanged. No cache or allocation is added.

## Behavior checks

The new headless harness compiles the actual backend source. An independent
ordered-membership oracle checks 72 successive frame configurations, including
all five prepared object kinds, height zero, partial stripes, offscreen ranges,
empty frames, and buffer growth/shrinkage. It also checks zero allocation churn
on warmed rebuilds. Explicit boundary cases cover empty ranges, row 32, and the
last partial stripe. Existing tests compare pixels and vertex counts with and
without bins, direct/staged meshes, transparency, additive blending, glow,
clipping, and offscreen rendering.

The initial release harness passed 16 test executions on both versions. The
final debug crate suite passed 72 test executions (including shared tests reused
by integration harnesses), with three manual benchmarks ignored. The final
release harness passed 17 test executions, with its manual benchmark ignored.
Clippy's performance lint passes; existing non-performance warnings remain.

```powershell
cargo test --locked -p deadlib-render-backend-software
cargo test --locked --release -p deadlib-render-backend-software --test stripe_bins
cargo clippy --locked -p deadlib-render-backend-software --lib -- -D clippy::perf
```

## Measurements

Windows x86-64, Intel Xeon E5-2696 v4, Rust 1.98.1 / LLVM 22.1.8. Both versions
used the repository release profile, including full LTO, and identical benchmark
code. Saved before/after executables were run sequentially in three pairs;
the last pair inherited affinity to logical CPU 6. No builds or other tests
from this pass ran concurrently with timing. Each invocation reports the median
of seven batches of 8,192 builds, after warmup. Inputs and output buffers pass
through `black_box`. Allocation counting runs separately from timing.

These fixtures isolate stripe construction with prepared sprites; they do not
measure preparation, rasterization, GPU backends, or game frame rate. Mixed
fixtures cycle through spans of 1, 32, 128, and full-screen rows. Except for
empty/small, each build contains 1,024 sprites.

| Fixture | Before ns/build | After ns/build | Median paired reduction | Range across three pairs |
| --- | ---: | ---: | ---: | ---: |
| Empty, 1080 rows | 78.6 | 30.2 | 60.7% | 48.5% to 64.1% |
| 16 sprites, 32-row spans | 227.1 | 153.2 | 35.5% | 9.8% to 44.3% |
| One-row spans | 8,269.2 | 6,993.7 | 20.2% | -7.9% to 21.9% |
| 128-row spans (4–5 stripes) | 14,341.8 | 11,956.5 | 16.6% | 5.7% to 37.7% |
| Full-height spans, 1080 rows | 456,713.9 | 438,809.3 | 8.0% | -14.8% to 12.2% |
| Mixed, 1080 rows | 21,153.1 | 15,795.0 | 14.9% | 13.7% to 25.3% |
| Mixed, 2160 rows | 33,703.1 | 27,235.2 | 13.1% | 7.5% to 19.2% |

Times are medians of invocation medians; reductions are medians of paired
reductions, so they need not equal the reduction between the two time columns.
For the 1080/2160-row mixed fixtures, median paired calling-thread cycle savings
were 14.2%/13.1%, and throughput increased 17.5%/15.1%.
Every measurement had zero allocations, reallocations, and frees per warmed
build. Full-height and one-row results varied enough to include regressions;
there is no consistent gain claim for those cases. Short empty/small batches
are especially sensitive to timing noise. All runs are retained in the
[raw results](software-stripe-counting-0.5.1525.csv), including calling-thread
cycle counts, sample ranges, and throughput in sprites/second (builds/second
for the empty fixture).

To reproduce, copy the new harness into a baseline checkout before compiling,
save its executable, then compile and run the changed version with the same
toolchain and flags. For either version:

```powershell
cargo test --locked --release -p deadlib-render-backend-software --test stripe_bins benchmark_stripe_build -- --ignored --nocapture --test-threads=1
```
