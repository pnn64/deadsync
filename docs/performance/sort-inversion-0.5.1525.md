# Skip sorting fallback scans after a within-layer order inversion

Baseline: `3c4a40d1d`.

The composition sorter collects a small set of Z layers and checks that each
layer's draw orders increase. An inversion already proves that bucket sorting
cannot produce the required `(z, order)` sequence. Previously, that failure
entered the general fallback, which scanned Z bounds and could recollect and
recount layers before choosing comparison sorting anyway.

The sorter now calls the same comparison sort immediately at the detected
inversion. Folding the collector into its sole caller removes the boolean
handoff that conflated an order inversion with exceeding the 64-layer limit.
The latter still uses the existing general fallback. Ordered-layer bucket
sorting and composition's sorted-input guard retain their existing behavior.
No cache, allocation, additional traversal, or persistent state was added.

This benefits frames with an inversion detected among the first 64 layers;
it does not establish a benefit for ordinary monotonic actor output. One
reachable case is a range of shadows at the minimum signed Z: subtracting one
saturates, leaving the shadows on the source layer with reused source orders.

## Behavior checks

The integration test includes production composition code and a frozen copy
of the baseline entrypoint and collector. Unchanged general fallback and
bucket helpers are shared. Both variants run through composition's sortedness
guard. Tests compare every resulting draw header exactly, including equal-key
permutations, payload indices, textures, cameras, blend modes and draw kinds.

- 882 cases cover empty and singleton inputs through 4,096 draws; 1 through
  257 layers; dense and wide signed Z ranges; early and late inversions;
  sorted Z with descending order; ties; and scratch reuse across these paths.
- 16 additional cases use the production shadow builder at minimum, adjacent,
  zero and maximum Z, checking exact source/shadow ordering after sorting.
- Allocation assertions warm both swapped scratch buffers, then repeatedly
  restore and sort inputs without allocating, reallocating or freeing memory.
- The existing 630-case draw-order comparison and expanded draw/batching
  compatibility tests also pass.

The 141 active integration tests passed in release mode before and after the
production edit; adding the shadow-builder test brings the final total to 142.
Debug validation passed 171 library tests and all 142 integration tests (many
source unit tests are repeated in the integration executable). Formatting,
diff checks and performance Clippy checks passed; other style warnings remain.

No live GPU pixel capture was performed. This change only orders integer draw
headers; tests compare their complete sequence, and do not change payloads or
the finalizer that consumes them.

## Before/after benchmark

Intel Xeon E5-2696 v4, Windows x86-64, Rust 1.98.1 / LLVM 22.1.8, repository
release profile with LTO. Three paired runs pinned to logical CPU 6, reversing
variant order in run 2. No builds or other tests from this pass ran during
measurement. Both variants use equivalent black-boxed sort function pointers
in one executable. Windows `QueryThreadCycleTime` supplies thread cycles.

Each operation restores the original draw headers into retained storage and
performs the sortedness guard and complete sort. Restoration is included
equally; fixture creation and scratch growth are excluded. Both variants reuse
the same draw and scratch allocations. Seven batches follow warmup; each batch
has `min(16_777_216 / draw_count, 65_536)` operations. Allocation accounting runs
separately. Throughput counts draw headers processed per second.

| Case | Before ns/op | After ns/op | Median paired cycle reduction | Reduction range | Median throughput increase |
| --- | ---: | ---: | ---: | ---: | ---: |
| sorted1024 | 2430.5 | 2483.9 | -1.3% | -2.1 to -0.1% | -1.3% |
| sparse7_1024 | 11532.0 | 11460.7 | -0.6% | -1.4 to 2.5% | -0.6% |
| dense65_1024 | 11973.5 | 11892.2 | -0.2% | -0.7 to 0.7% | -0.2% |
| dense128_4096 | 57288.8 | 57362.9 | -0.1% | -0.8 to 0.0% | -0.1% |
| unordered7_64 | 1330.2 | 1105.0 | 16.3% | 15.3 to 16.9% | 19.4% |
| unordered7_1024 | 34334.3 | 30938.1 | 9.7% | 8.2 to 11.4% | 11.0% |
| unordered7_4096 | 159743.1 | 148406.5 | 7.1% | 6.9 to 7.6% | 7.6% |
| wide_unordered7 | 35240.4 | 30831.2 | 12.6% | 12.4 to 15.6% | 14.3% |
| ordered_z1024 | 29939.9 | 29894.3 | 2.6% | -1.7 to 3.8% | 2.7% |
| late7_1024 | 32899.0 | 30340.9 | 9.6% | 7.8 to 14.1% | 10.7% |
| unordered128 | 164034.1 | 163303.3 | -0.3% | -0.3 to 0.4% | -0.3% |
| saturated_shadows64 | 811.6 | 730.6 | 9.9% | 9.5 to 9.9% | 11.0% |
| saturated_shadows1024 | 25412.4 | 24002.2 | 5.4% | 1.6 to 5.5% | 5.7% |

Times are medians across runs; percentages are medians of paired ratios, so
they need not equal ratios of displayed time medians. Negative reductions mean
a slower candidate. `unordered` cases reverse each layer's draw orders;
`late` puts an inversion at the last draw. The shadow cases use the production
shadow builder, appending a shadow range after every four source draws.

The five seven-layer inversion cases and both shadow cases improved in every
final paired run. The already-Z-sorted inversion case had a mixed result and
does not establish a consistent improvement. Controls show no algorithmic gain:
their median cycle differences were -0.1% to -1.3%. Sorted input never invokes
the edited function; its small measured difference reflects benchmark variation.
The 128-layer unordered case reaches the layer limit before detecting its first
order inversion, so it also retains the original fallback.

All warmed operations used zero allocations, reallocations, frees, allocated
bytes and freed bytes before and after. These are synthetic CPU sorting
measurements, not full-frame/FPS improvements.

The [raw CSV](sort-inversion-0.5.1525.csv) includes all 166 reported samples:
22 from an executable built before the production edit (both variants unchanged),
66 initial paired measurements, and 78 final measurements shown above. The first
two groups used 4,096 operations per batch. The unchanged-code control varied by
up to about 13% in thread cycles; the initial candidate runs also had noisy
controls. Final measurements use the longer batches documented above and add
the shadow cases; the production algorithm was unchanged between the initial
and final candidate runs.

## Reproduce

```powershell
cargo test --locked --release -p deadlib-present --test z_batching
cargo test --locked -p deadlib-present --lib --test z_batching
cargo clippy --locked -p deadlib-present --lib --test z_batching -- -D clippy::perf
(Get-Process -Id $PID).ProcessorAffinity = 64
# Run the printed release test executable directly:
# <executable> benchmark_sort_inversion --ignored --nocapture --test-threads=1
# Repeat three times; set DEADSYNC_PERF_REVERSE=1 for the middle run.
```

Local raw logs and the unchanged baseline executable are retained under
`target/sort-inversion-pass`.
