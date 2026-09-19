# Remove the duplicate software near-plane visibility check

Baseline: `adfd8870b`. `project_tmesh_polygon` now always delegates to
`clip_tmesh_near`, which already handles fully visible, fully rejected, and
partially clipped triangles. This removes the caller's second visibility test
and its separate borrowed/owned polygon paths. Clipping arithmetic, vertex
order, interpolation, and projection are unchanged; no cache or allocation is
added. This affects software mesh preparation and its direct rasterization path.

The frozen projection function in `tests/near_clip/baseline.rs` exercises the
same unchanged clipping helper. The regression compares every returned float's
bits, including unused output slots, across 1,536 depth/matrix combinations:
near-plane boundaries, fully visible/rejected and straddling triangles, extreme
finite values, infinities, NaNs, perspective, and zero-w projection. It passes in
debug and release. Existing tests also compare direct/staged rasterization,
striped rendering, clipping, and output pixels.

Validation:

```powershell
cargo test --locked -p deadlib-render-core --features test-util -p deadlib-present -p deadlib-render-backend-software
cargo test --locked --release -p deadlib-render-backend-software --test near_clip near_clip_projection
cargo clippy --locked -p deadlib-present -p deadlib-render-backend-software --lib -- -D clippy::perf
```

The combined debug run passed 673 test executions (including existing tests
reused by integration harnesses), with 15 ignored/manual tests and doc examples.
The focused release comparison passed. Clippy's performance lint passes;
existing non-performance warnings remain. The subsequent presentation-only
guard revision was checked again with 302 passing test executions and its
release comparison. GPU backends were reviewed but not changed or benchmarked.

## Measurements

Same machine, compiler, release flags, affinity, and sampling method as
[`mesh-visibility-0.5.1368.md`](mesh-visibility-0.5.1368.md). Each operation
projects 256 triangles. The four fixtures put zero, one, two, or all three
vertices outside the near plane. Matrix/vertex inputs and outputs pass through
`black_box`. Each timed batch has 1,024 operations; old/new run first in
alternating invocations. No builds or other test jobs from this review ran
alongside the five final benchmark invocations.

| Fixture | Old median ns / 256 | New median ns / 256 | Median paired time reduction | Range across five pairs |
| --- | ---: | ---: | ---: | ---: |
| Fully visible | 11,184.3 | 10,830.8 | 3.7% | 0.2% to 11.3% |
| One vertex outside | 21,017.3 | 19,116.2 | 9.2% | 8.9% to 11.3% |
| Two vertices outside | 20,602.8 | 18,997.4 | 7.5% | -52.8% to 8.8% |
| Fully behind | 10,532.4 | 10,434.4 | 0.9% | 0.0% to 2.3% |

Times are medians of invocation medians; percentages are medians of paired
reductions. Calling-thread cycle reductions were 2.4%, 9.1%, 7.7%, and 1.2%.
All variants recorded zero allocations, reallocations, and frees. Per-run
medians and seven-sample ranges are in
[`near-clip-0.5.1368.csv`](near-clip-0.5.1368.csv).

The one-outside fixture consistently saves about 7 ns per triangle here.
Results for the other fixtures are less conclusive: the two-outside fixture
contains a large adverse timing excursion, and visible/behind savings are
small. All five invocations are included; no outlier was discarded. Earlier
exploratory runs also showed substantial timing variation. These measurements
support the removal of duplicate clipping work, not a general percentage gain
for software rendering or game frame rate. Rasterization and presentation
costs are outside this microbenchmark.

Reproduce after builds finish:

```powershell
cargo test --locked --release -p deadlib-render-backend-software --test near_clip --no-run
(Get-Process -Id $PID).ProcessorAffinity = 4
cargo test --locked --release -p deadlib-render-backend-software --test near_clip near_clip_bench -- --ignored --nocapture --test-threads=1
# Repeat five times, setting this only for invocations 2 and 4:
$env:DEADSYNC_PERF_REVERSE = '1'
```
