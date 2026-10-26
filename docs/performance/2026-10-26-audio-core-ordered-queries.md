# Audio core: ordered playback queries and diagnostic filtering

This batch retains three measured changes: direct lookup of the newest stream interval, direct inverse lookup of the newest music interval, and rejection of unusable diagnostic entries before loading their payloads. The baseline follows `8da49e3d5`. Public method signatures, floating arithmetic for selected results, insertion/coalescing output, queue capacity, diagnostic append order, and returned-event version checks are preserved. Production still forbids unsafe code.

| Final workload | Before CPU cycles/op | After CPU cycles/op | CPU throughput |
| --- | ---: | ---: | ---: |
| Newest stream interval, 1 segment | 12.98 | 9.65 | 1.35x |
| Newest stream interval, 127 segments | 1,078.67 | 9.67 | 111.55x |
| Newest stream interval, 1,023 segments | 8,431.65 | 11.23 | 750.81x |
| Newest music interval, 127 segments | 1,531.47 | 9.68 | 158.13x |
| Newest music interval, 1,023 segments | 12,416.69 | 11.23 | 1,105.67x |
| Collect 64 expired diagnostic entries | 647.43 | 322.62 | 2.01x |
| Collect 64 stale-version entries | 591.41 | 179.59 | 3.29x |
| Collect 64 live entries | 1,126.26 | 1,116.09 | 1.01x |

All measured query operations have zero allocation, reallocation, and free counts before and after. These are conditional query microbenchmarks, not game-wide speedup estimates. The large lookup ratios result from replacing a full scan with one interval check when its ordering guard permits it. The diagnostic gains apply to rejected entries; live entries remain close to baseline.

[Confirmed measurements](2026-10-26-audio-core-query-confirmed.tsv) contain all 66 workloads across six alternating runs of archived before/after executables. [Summary selections](2026-10-26-audio-core-query-summary.json) identify the rows above. Measurement uses `QueryThreadCycleTime`, logical CPU 2, one warmup and eleven timed batches per workload, then medians across runs. Fixture construction is outside timing. Release optimization, LTO, static MSVC runtime, and rustc 1.99.1 settings are unchanged. Manifests record source and binary hashes, compiler, Cargo configuration, lockfile, and commands; final current sources match the `queries-inline` archive.

Each production change was measured separately before the next one: [stream lookup](2026-10-26-audio-core-ordered-lookup.tsv), [inverse lookup](2026-10-26-audio-core-ordered-inverse.tsv), and [diagnostic filtering](2026-10-26-audio-core-filtered-events.tsv). The final refinements and inlining comparison are preserved in [tuned lookups](2026-10-26-audio-core-query-tuned.tsv) and [inline comparison](2026-10-26-audio-core-query-inline.tsv).

`PlaybackPosMap` now maintains two inline flags. One proves that rounded stream intervals are ordered and disjoint; the other proves the same for music intervals with nonnegative slopes. A matching newest interval can then be returned directly with the original arithmetic. Earlier entries cannot contain that query. Unordered maps, reverse music slopes, gaps, extrapolation, and other misses retain the original scan and tie-breaking rules. The inverse shortcut is restricted to multi-segment maps. No query mutates the map, so its shared-reader traits remain intact.

Ordering checks run when a segment is appended. Coalescing keeps the existing segment's left boundary and bypasses the append checks. Cleanup checks the first remaining pair after a partial trim: rounding can change an endpoint even when its integer location is mathematically unchanged. Flags can conservatively stay false until `clear`; no extra queue scan is needed to recover them. This adds two private flags and bounded insertion work while eliminating repeated queue traversal for eligible lookups. It removes the redundant empty-queue checks from both query methods.

The insertion tradeoff is measured separately in [insertion controls](2026-10-26-audio-core-insert-confirmed.tsv). These are secondary wall-clock nanoseconds per clear-and-insert batch, rather than CPU-cycle measurements. For 1,024 contiguous segments, the median was 13,896 -> 11,432 ns; for 1,024 stream-gap segments it was 13,455 -> 16,859 ns. Rate-changing batches also cost more in some cases. The earlier insertion measurements are retained in [initial insertion controls](2026-10-26-audio-core-insert-controls.tsv). Moving checks off the merge path reduced the overhead seen there.

Some query controls also cost more. One-segment inverse queries increased from about 17 to 19 cycles; one-segment misses and first-segment hits in longer maps add roughly 2–4 cycles. Long fallback scans remain near baseline. These costs are recorded alongside the newest-interval improvements. The normal inline hint on `invert` was retained after reducing its newest-interval path from about 22 to 11 cycles in the combined build.

Diagnostic loading now receives the expected sequence and time window. It checks the initial version and timestamp first, then reads payload fields only for an eligible record. Returned records still require the final acquired version to equal the initial acquired version. The original decoded-sequence comparison is preserved, including its behavior beyond sequence `2^63`. This removes the tagged result tuple and the caller's separate sequence/timestamp filtering stages. Timestamp zero, future timestamps, inclusive time boundaries, invalid kinds, in-progress records, and stale records retain their prior results.

Frozen query and diagnostic references are in `tests/support/position_query_reference.rs` and `tests/support/diagnostic_reference.rs`. Differential checks cover overlap priority, gaps, endpoint ties, adjacent floating values, large stream positions, finite extremes, infinities, NaNs, negative and zero slopes, merges, trimming, clear, wrapped deque storage, and release integer-overflow histories. The older insertion reference explicitly disables only the new flags, keeping its reference queries on the original full scan; its queue-building algorithm is unchanged.

Diagnostic checks compare every returned field and append order over empty, full, and wrapped rings; expired, future, zero, stale, invalid-kind, and writing entries; time-window boundaries; and high sequence values. A concurrent producer wraps the ring over 100,000 new events while the reader verifies correlated payload fields and increasing event order. After the producer finishes, its final snapshot must equal the frozen reference.

Final validation passed 129 release tests with eleven benchmarks ignored, 56 debug library tests, and 24 debug query tests. Benchmarks were invoked explicitly. Clippy passed with warnings denied and the same two pre-existing lint categories allowed (`chunks_exact_to_as_chunks` and `too_many_arguments`). Formatting, Python compilation, scoped whitespace checks, binary hashes, and final source hashes passed. Full command output is in [query validation](../testing/2026-10-26-audio-core-query-validation.json).

The [nearest-endpoint rewrite](2026-10-26-audio-core-nearest-endpoint.tsv) and [inverse endpoint-distance rewrite](2026-10-26-audio-core-inverse-endpoint-distance.tsv) passed differential checks but were slower; both were reverted. Their measurements remain available.

To rebuild and repeat the final comparison against the archived baseline:

```powershell
python scripts/benchmark_audio_core.py --build query-current --target query_kernels
python scripts/benchmark_audio_core.py --compare queries-before query-current --runs 6 --output target/audio-core-kernels/query-repeat.tsv
```

The benchmark script verifies source stability during compilation. Source snapshots and executables are archived under `target/audio-core-kernels`; adjacent committed manifests identify them. For a fresh baseline checkout, use the frozen pre-change production sources with the corresponding pre-change harness snapshot. The new harness accesses the new private ordering flags, so it must not be copied unchanged over an older map implementation.
