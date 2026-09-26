# Remove the repeated draw-order check from fallback sorting

Baseline: `e02f2d180`.

Frame composition checks whether draw headers are ordered by `(z, order)` before
calling its sorter. The general fallback sorter repeated that full-key check
while scanning the Z range. Its sorted-input early return could not be reached
through the production caller, which had already found an inversion.

The fallback scan now tracks only Z ordering and its minimum/maximum. These
remain necessary to choose comparison sorting or bucket sorting. The change
removes full-key comparisons, order-field loads and the redundant early return.
It adds no cache, allocation, representation, or traversal pass.

The general fallback runs when the first sparse-layer attempt encounters more
than 64 distinct layers or nonmonotonic draw order within a layer. Already-sorted
frames and the usual small set of ordered layers keep their existing paths.

## Behavior checks

The integration suite includes production composition code and a frozen copy
of the original sorting entrypoint and fallback. Unchanged bucket helpers and
draw types are shared. The tests exercise composition's sorted-input guard
before either implementation.

630 cases compare every resulting `DrawItem` exactly, including payload index,
texture handle, camera, blend, kind, Z and order. Cases cover:

- 0, 1, 2, 17, 64, 65, 257, 1,024 and 4,096 draws.
- 1, 2, 7, 64, 65, 128 and 257 layers, including wide signed Z ranges.
- Sorted input, interleaved layers, reversed order within layers and ordered Z
  with reversed draw order.
- Both unique and tied ordering keys, preserving the baseline's actual
  permutation of equal-key draws, not just final sortedness.
- Scratch reuse across changing sizes and arrangements, plus allocation checks
  on warmed dense, wide and unordered cases.

The 139 active integration tests passed before and after the edit. The suite
also covers expanded draw sequences, state boundaries and sprite gathering.
Debug validation passed 171 library tests and all 139 integration tests; many
source unit tests are repeated in the integration executable. Formatting, diff
checks and performance Clippy checks passed. Existing style warnings remain.
No live GPU pixel capture was performed; the change operates only on integer
draw headers, whose complete resulting sequence was compared exactly.

## Before/after benchmark

Intel Xeon E5-2696 v4, Windows x86-64, Rust 1.98.1 / LLVM 22.1.8, repository
release profile with LTO. Three paired runs pinned to logical CPU 6, reversing
variant order in run 2. No builds or other tests from this pass ran during
measurement. Both variants use equivalent black-boxed sort function pointers
in one executable. Windows `QueryThreadCycleTime` supplies thread cycles.

Each operation restores the original draw headers into retained storage and
performs composition's sortedness check and complete sorting path. Restoration
is included equally; fixture creation and scratch growth are excluded. Seven
batches of 2,048 operations follow warmup. Allocation accounting runs separately.
Throughput counts draw headers processed per second.

| Case | Before ns/op | After ns/op | Median paired cycle reduction | Reduction range | Median throughput increase |
| --- | ---: | ---: | ---: | ---: | ---: |
| sorted1024 | 1956.0 | 1789.5 | 2.5% | -5.8 to 8.3% | 2.5% |
| sparse7_1024 | 10552.9 | 10687.1 | -1.2% | -2.1 to 2.7% | -1.3% |
| dense65_1024 | 12140.1 | 11381.9 | 11.8% | 6.2 to 13.0% | 13.4% |
| dense128_4096 | 51732.4 | 47961.0 | 8.9% | 6.5 to 9.7% | 9.8% |
| wide65_1024 | 26200.1 | 24975.0 | 4.7% | 3.6 to 7.2% | 4.9% |
| unordered7_1024 | 28096.6 | 29628.1 | 1.4% | -8.0 to 3.1% | 1.3% |
| unordered128_4096 | 143030.0 | 133877.2 | 5.3% | -1.0 to 15.4% | 5.5% |
| ordered_z_1024 | 28800.3 | 28477.1 | 2.3% | 1.1 to 6.8% | 2.4% |

Times are medians across runs; percentages are medians of paired ratios, so
they need not equal ratios of displayed time medians. Negative reductions mean
a slower candidate. An additional executable built before the edit compared
the two unchanged implementations: every case differed by less than 2% in
thread cycles. That control and all final samples are in the
[raw CSV](draw-order-recheck-0.5.1525.csv).

Dense fallback cases and the wide-layer case improved in every paired run.
The arbitrary within-layer-order cases are noisier and do not establish a
consistent improvement. The sorted and small-layer paths are controls with no
algorithmic change; their timing variation is not an optimization claim.
All warmed operations used zero allocations, reallocations, frees, allocated
bytes and freed bytes before and after. These are synthetic CPU sorting
measurements, not full-frame/FPS improvements.

## Reproduce

```powershell
cargo test --locked --release -p deadlib-present --test z_batching
cargo test --locked -p deadlib-present --lib --test z_batching
cargo clippy --locked -p deadlib-present --lib --test z_batching -- -D clippy::perf
(Get-Process -Id $PID).ProcessorAffinity = 64
# Run the printed release test executable directly:
# <executable> benchmark_draw_order_recheck --ignored --nocapture --test-threads=1
# Repeat three times; set DEADSYNC_PERF_REVERSE=1 for the middle run.
```

Local raw logs and the unchanged baseline executable are retained under
`target/draw-order-recheck-pass`.
