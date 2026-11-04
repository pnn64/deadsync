# Stream segment scans and combined outputs — 2026-11-04

Baseline: `6ce445f9e4aad1d9d0d8f9828eb76c15667b90b9`. Three safe Rust optimizations in two existing `stream.rs` bodies reduce in-memory stream analysis work:

1. The segment scanner calls the existing run-state machine once at the start of a contiguous qualifying run, then advances directly to its end. A one-measure run retains the end already written by `record`, avoiding another store. The first nonqualifying measure is skipped after that scan.
2. When counter/ZMod thresholds and initial capacities match, one segment list is emitted, and the other is copied in bulk. Reserving the emitted list's final capacity preserves both returned capacities while avoiding incremental growth of the copied list. Unequal initial capacities retain the original push path.
3. Differing thresholds select their order once, then test the higher threshold first. A measure meeting it qualifies for both streams, eliminating the second threshold comparison. Both callback orders remain counter first, then ZMod.

The row classifier is restored after slower experiments. There are no new production helpers, fields, types, dependencies, public signatures or unsafe blocks. The two bodies add 39 lines overall, including comments/spacing; another four lines register the test module. The extra loop branches replace repeated per-measure bookkeeping and comparisons rather than adding an abstraction layer.

## Final measurements

Windows x86-64, Intel64 Family 6 Model 79 Stepping 1, rustc 1.99.1 / LLVM 23.1.1. Release kernels use opt-level 3, fat LTO and the static CRT. Five fresh processes alternate original/current samples on CPU 2 for twelve rounds, discard the first and retain the median of eleven. Each final sample performs 10,000 row/segment/output fixtures, or 1,000 full-parser fixtures. Inputs are prepared outside timing and black-boxed. The reference borrows the unchanged public segment/output types so both implementations use the same data layout.

The cycle columns are independent process medians; throughput is the median of paired original/current ratios. They need not divide to the same result. TSVs also retain paired nanoseconds. These are repeated, cache-resident kernel measurements; no whole-chart-loading or frame-rate gain is claimed.

| Workload / 2,048 measures per fixture | Original cycles | Current cycles | Paired throughput |
| --- | ---: | ---: | ---: |
| `outputs/all_constant/2048` | 10337.66 | 7592.20 | 1.367× |
| `outputs/all_variable/2048` | 5361.46 | 2474.93 | 2.239× |
| `outputs/alternating_variable/2048` | 20857.50 | 12700.09 | 1.719× |
| `outputs/different_threshold/2048` | 18048.69 | 15635.36 | 1.076× |
| `outputs/higher_counter_all/2048` | 8988.92 | 6910.34 | 1.309× |
| `outputs/higher_counter_runs/2048` | 11153.01 | 10185.90 | 1.131× |
| `outputs/runs_constant/2048` | 13721.74 | 10789.94 | 1.254× |
| `outputs/runs_variable/2048` | 8898.09 | 4996.57 | 1.781× |
| `segments/all/2048` | 4717.98 | 1761.16 | 2.694× |
| `segments/alternating/2048` | 7879.15 | 7049.97 | 1.129× |
| `segments/none/2048` | 2070.57 | 1395.84 | 1.471× |
| `segments/runs/2048` | 5035.33 | 1963.30 | 2.593× |

The segment fixtures improve 1.129×–2.694×. Matching combined-output fixtures improve 1.254×–2.239×. The differing-threshold fixtures improve 1.076×–1.309×; the all-qualified higher-counter fixture isolates the threshold dispatch from the run scanner and matching-list copy.

Identical-source control medians span 0.786×–1.096× overall. Segment controls span 0.994×–1.014×; differing-threshold output controls span 0.966×–1.012×. Matching-output controls are slower on the current side (0.854×–0.953×). An unchanged eight-lane first-byte helper reaches 0.786× in the control, exposing substantial placement/measurement bias for that tiny case. Full parser controls span 0.983×–1.059×. Small differences are inconclusive; there is no performance claim for restored row classification or density parsing. Raw per-process ranges are retained in the checks JSON.

## Allocations and capacities

The alternating variable-BPM output fixture reduces reallocations **16 → 8** and allocated/freed bytes **32,704 → 24,544**. It retains two allocations, two frees and the 16,384-byte peak. The mixed-run variable fixture reduces reallocations **8 → 4** and allocated/freed bytes **1,984 → 1,504**, retaining two allocations, two frees and the 1,024-byte peak. Returned vector capacities match the original in both cases. Other measured allocation counters do not increase; all twelve row-only fixtures remain allocation-free.

The bulk-copy path creates no intermediate collection. Both destination vectors still exist because they are separately returned. With equal initial capacities, the original identical pushes would give both vectors the same growth sequence; the new reserve uses the actual final capacity of the emitted vector. Its destination is still empty at that point, so `reserve_exact` requests precisely that capacity. The explicit unequal-capacity fallback preserves independently supplied capacities too. Stream segments are Copy values.

## Exact behavior and validation

The scanner retains the original 31-bit length assertion and `StreamRuns` state machine. After the first qualifying index, every following qualifying index would only increment its end. Scanning to the first failure and assigning that end once gives the same state. The failed index makes no state change and may be skipped. The cursor can reach length + 1; the existing length < 2^31 check keeps that addition within usize on the supported 32/64-bit targets. Segment emission order, omitted one-measure gaps, initial/trailing breaks and lazy allocation remain unchanged.

For differing thresholds, qualifying for the higher threshold implies qualifying for the lower one. The two specialized loop orders preserve the original callbacks, index order and floating-point total updates. No floating-point expression or accumulation order is changed.

- Four focused tests pass in release and debug. They compare **1,514,240 row cases**, including every byte at every lane position and every adjacent byte pair; **57,337** exhaustive short-pattern/threshold segment cases; **42,768** public output cases plus **100** private capacity combinations; and 12,000 generated chart/lane pairs plus eight full charts. All returned segment fields and vector capacities are checked; output totals compare floating-point bits. Public output tests bound all six allocation counters.
- The ten latest stages, including debug/release and both controls, share an exact **26,202,720-byte** output trace, SHA-256 `98d1bd6c1d72372a9e4389a55c7f37caf19c2742c282e523f2f0494b6f1e0186`. The first six exploratory stages use the earlier corpus without the hundred private-capacity combinations; their own trace hashes are retained.
- Full rules release suite: **137 passed**, eight benchmark/trace tests ignored. All four new correctness tests appear in that full-suite log. A normal release library check passes with before/after fingerprints matching the final production module. Focused Clippy passes with warnings denied against the final measured source context.
- The audit verifies all sixteen kernel stages: complete generated compiler context, frozen reference bodies, unchanged public-type declarations, source/binary/log/trace hashes, every raw timing row and allocation counter. Replacing the two changed bodies and removing the test declaration reproduces the baseline source. Formatting, Python compilation and whitespace checks pass.

Full testing reuses the verified isolated dependency workspace from the earlier chart work, overlays current source, and runs only the rules package. No external JSON fixture is required for this package. Temporary build files stay on D:, and the actual test binary retains the normal host temporary directory. The main checkout is read only.

## Discarded experiments and reproduction

Direct `matches!` classification was slower than the lookup table (0.492×–0.784× on several empty/last-lane helper cases). Packed-byte tests improved isolated wide-row helpers but slowed whole density parsing to 0.894× / 0.931× for eight/ten lanes; they were discarded. The original row body is byte-for-byte restored apart from line-ending normalization.

The first two-position run scanner reached 2.360× on a single long run but slowed alternating segments to 0.673×. Reusing the existing run state, then avoiding its redundant short-run end store, removed that regression. The final alternating result is 1.129×. The intermediate differing-threshold output slowdown was removed by the final higher-threshold dispatch.

Final measurements use longer samples and shared unchanged public types; earlier snapshots retain their original separate reference types and shorter samples. A test-only byte-array spelling triggered Clippy's byte-string lint; it was corrected before the final source snapshots and validation.

```powershell
$env:DEADSYNC_STREAM_SCANS_ARCHIVE = 'D:\deadsync-stream-scans-repro'
python scripts/benchmark_stream_scans.py control --baseline 6ce445f9e4aad1d9d0d8f9828eb76c15667b90b9 --reference control --runs 5
python scripts/benchmark_stream_scans.py final --reference control --runs 5
python scripts/benchmark_stream_scans.py baseline-debug --baseline 6ce445f9e4aad1d9d0d8f9828eb76c15667b90b9 --debug
python scripts/benchmark_stream_scans.py checked-debug --reference baseline-debug --debug
python scripts/benchmark_stream_scans.py full-release --full
```

Snapshots, binaries, raw run logs and traces are retained under `D:\deadsync-stream-scans-20261104`. Checked-in manifests, TSVs and checks JSON identify the artifacts and sources.
