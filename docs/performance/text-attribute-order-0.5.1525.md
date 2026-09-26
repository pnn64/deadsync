# Remove text-attribute event tie-break comparisons

Baseline: `2ea0521e9`.

`deadlib-present::TextAttrCursor` sorts attribute start and end events before
walking visible glyphs. Previously each sort key included the original attribute
index as a tie-breaker. Events with the same boundary are consumed together
before a color is returned. The cursor's existing `active_max` selects the
highest original attribute index, independently preserving the rule that the
last matching attribute in the caller's slice wins.

Both sorts now compare only the boundary. This removes the extra index
comparison on ties and simplifies the sort keys. Event processing, original
attribute indices, range arithmetic, and color selection are unchanged. Scratch
storage and allocation behavior are also unchanged.

## Behavior checks

The new integration test includes the production composition source and the
baseline constructor in one executable. It compares both cursors against a
simple reverse scan of the caller's attributes, checking every color component
by its exact floating-point bits.

- 224 traces / 39,704 character queries across 0, 1, 8, 9, 32, 128, and 257 attributes.
- Ordered, overlapping, grouped equal-boundary, and identical ranges.
- Rotated/reversed input order, varying character skips, zero-length ranges,
  saturating end positions near `usize::MAX`, signed zero, and corner colors.
- Reused scratch storage across sizes and arrangements.

The integration suite passes before and after the edit in release: 134 active
tests, including existing composition tests for attributed text vertices and
texture batches. The changed code also passes 171 library tests plus the
integration suite in debug. Many integration tests repeat the library's source
unit tests; these counts are not distinct coverage totals. The manual benchmark
is ignored by default and was run separately. Formatting and performance Clippy
checks pass; unrelated existing style warnings remain.

```powershell
cargo test --locked --release -p deadlib-present --test attribute_order
cargo test --locked -p deadlib-present --lib --test attribute_order
cargo clippy --locked -p deadlib-present --lib --test attribute_order -- -D clippy::perf
```

No live GPU rendering or non-Windows runtime test was performed. The changed
code only orders CPU-side attribute events; exact output colors are checked.

## Before/after benchmark

Intel Xeon E5-2696 v4, Windows x86-64, Rust 1.98.1 / LLVM 22.1.8, repository
release profile with LTO. Three paired runs pinned to logical CPU 6, reversing
variant order in run 2. No builds or other tests from this pass ran during
measurement. Both implementations use equivalent black-boxed constructor
function pointers in the same executable and the same production cursor walk.

One operation constructs a cursor over the listed number of attributes and
queries 64 increasing character indices (`0, 4, ..., 252`). This measures both
sorting and event processing, including the possible cost of different removal
orders among equal-end events. Input construction is excluded; retained scratch
capacity is warmed before timing. Seven batches of 8,192 operations follow
three warmup operations. Windows `QueryThreadCycleTime` provides thread cycle
counts; allocation accounting is measured separately.

| Case | Median ns/op before | After | Median paired cycle reduction | Paired cycle reduction range | Median throughput increase |
| --- | ---: | ---: | ---: | ---: | ---: |
| 8 ordered attributes | 302.1 | 291.9 | 1.7% | 1.0 to 5.9% | 1.7% |
| 8 overlapping | 401.1 | 372.7 | 5.1% | 0.5 to 11.4% | 5.2% |
| 32 overlapping | 1619.9 | 1384.1 | 14.7% | 13.6 to 18.7% | 17.0% |
| 32 in tied groups | 1548.5 | 1349.8 | 12.8% | 12.7 to 13.9% | 14.7% |
| 128 identical ranges | 3833.6 | 3733.5 | 2.7% | 2.0 to 2.8% | 2.7% |
| 128 overlapping | 6995.7 | 5686.6 | 18.8% | 17.2 to 19.1% | 23.0% |

Times are medians across runs. Percentage columns are medians of paired run
ratios, so they need not equal ratios of the displayed time medians. Every case
had zero allocations, reallocations, frees, allocated bytes, and freed bytes
per warmed operation before and after. All samples are in the
[raw CSV](text-attribute-order-0.5.1525.csv).

These are synthetic cursor workloads, not whole-frame or FPS measurements.
The larger cases save about 0.10 to 1.31 microseconds per cursor operation; the
small ordered case has a much smaller, noise-sensitive difference. Text without
attributes does not use this sorting path.

## Reproduce

```powershell
cargo test --locked --release -p deadlib-present --test attribute_order --no-run
(Get-Process -Id $PID).ProcessorAffinity = 64
# Run the printed executable directly:
# <executable> benchmark_attribute_order --ignored --nocapture --test-threads=1
# Repeat three times; set DEADSYNC_PERF_REVERSE=1 for the middle run.
```

Capture stderr directly. Local logs are retained under
`target/attribute-order-pass/run1.txt` through `run3.txt`.
