# Chart edit filtering, comparison and reference collection

Baseline runtime source: `2080a074c`; measured on 2026-10-26. Three retained changes improve short-name comparison **1.440x**, standard-chart filtering **12.443x**, and five-edit reference collection **1.288x** in isolated comparisons. The full before/after tables, allocation counters and regressions follow. These are warmed kernels and public chart-selection operations, with no whole-screen or gameplay-latency claim.

The production-source diff is **37 inserted / 14 removed lines**, including four lines registering the test module. Only three runtime function bodies change. Whole-string ASCII prescans, unnecessary full chart-type comparisons, and early heap growth for short reference lists are removed. Public signatures and stable sort keys stay unchanged.

## Retained changes and exactness

1. `lowercase_cmp` compares the ASCII prefix as it reads it, returning at the first differing lowercase byte. It no longer scans both complete strings for ASCII first. At the first non-ASCII byte it falls back to the original per-character Unicode lowercase iterators over the remaining suffixes. Every skipped byte was ASCII, so both slice offsets are valid UTF-8 boundaries. If either string ends within the shared ASCII prefix, length ordering gives the original prefix ordering. Unicode expansions remain in the fallback, including U+0130, U+212A and Greek variants.
2. `is_edit_chart` first rejects unequal chart-type lengths, then folds the four bytes of the fixed `edit` marker with `0x20`, then performs the original chart-type equality check. OR-ing the case bit can match an ASCII lowercase letter only from that letter or its uppercase form; non-ASCII bytes retain their high bit. Length and exact-byte matching preserve rejection of padded, punctuated, longer and Unicode lookalike difficulty names. The type-length guard preserves the original cheap rejection of unequal-length types.
3. `edit_charts_sorted` returns empty/single results directly; for two or more matches it collects borrowed chart references into the existing SmallVec type with eight inline slots, runs the unchanged stable slice sort, then returns `into_vec()`. Up to eight references stay on the stack during collection and sorting, and conversion allocates only the final result length. Larger lists spill into growable storage automatically. This removes early vector growth without another scan, chart copies or a new helper. Both the filter and stable comparator remain the same, so all reference identities and tie orders are preserved. The iterator is consumed once, including on the direct-return paths. Empty results still allocate nothing.

All changed production code is safe Rust. There is no new dependency, public type or persistent state. The background movie-cycle runtime is unchanged: a branch-based wraparound candidate failed to show a reliable improvement in actual path-cloning workloads and was rejected. An index-tiebroken unstable-sort candidate halved requested memory on large lists but increased CPU work roughly 2-3.6x in initial paired samples with duplicate keys; it was also rejected. An exact-count reservation candidate reduced requested bytes but its second scan consistently slowed medium/large reference lists, so it was rejected as well. Screening artifacts remain under `target/chart-shortcuts/attempt1/`, `attempt2/` and `attempt3/`.

## Consecutive before/after measurements

Eight fresh processes per version alternate execution order. Cycles are median process medians per complete fixture; throughput is median-before / median-after. Paired ranges include every process pair.

### Incremental comparison

| Fixture | Before cycles | After cycles | CPU throughput | Paired range |
| --- | ---: | ---: | ---: | ---: |
| `compare/early` | 30,694.85 | 2,453.09 | 12.513x | 11.550-14.974x |
| `compare/equal` | 18,732.10 | 18,725.24 | 1.000x | 0.909-1.249x |
| `compare/late` | 127,692.40 | 91,764.73 | 1.392x | 1.383-1.600x |
| `compare/unicode` | 77,594.11 | 47,808.12 | 1.623x | 1.534-2.040x |
| `compare/short` | 3,554.87 | 2,468.52 | 1.440x | 1.242-1.695x |

[Complete step data](2026-10-26-chart-compare.tsv).

### Fixed edit predicate

| Fixture | Before cycles | After cycles | CPU throughput | Paired range |
| --- | ---: | ---: | ---: | ---: |
| `filter/standard` | 386.06 | 31.02 | 12.443x | 10.748-15.492x |
| `filter/wrong` | 26.84 | 26.79 | 1.002x | 0.849-1.472x |
| `filter/one` | 240.13 | 55.03 | 4.363x | 3.255-5.224x |
| `filter/mixed` | 2,536.20 | 967.92 | 2.620x | 2.558-3.212x |
| `filter/edits` | 1,360.57 | 1,223.01 | 1.112x | 1.074-1.182x |
| `filter/five` | 216.34 | 202.25 | 1.070x | 0.984-1.211x |
| `filter/eight` | 358.24 | 322.55 | 1.111x | 1.034-1.164x |
| `filter/unicode` | 1,362.87 | 1,215.02 | 1.122x | 0.980-1.193x |
| `filter/large` | 126,679.00 | 132,564.38 | 0.956x | 0.928-1.066x |
| `filter/huge` | 501,132.13 | 524,083.75 | 0.956x | 0.913-0.995x |

[Complete step data](2026-10-26-chart-filter.tsv).

### Inline reference collection

| Fixture | Before cycles | After cycles | CPU throughput | Paired range |
| --- | ---: | ---: | ---: | ---: |
| `refs/standard` | 42.88 | 49.73 | 0.862x | 0.742-1.000x |
| `refs/wrong` | 37.73 | 45.45 | 0.830x | 0.750-0.923x |
| `refs/one` | 204.93 | 222.93 | 0.919x | 0.801-1.085x |
| `refs/mixed` | 7,097.73 | 7,926.01 | 0.895x | 0.765-1.096x |
| `refs/edits` | 7,580.47 | 6,867.95 | 1.104x | 0.998-1.184x |
| `refs/five` | 613.05 | 475.87 | 1.288x | 1.150-1.401x |
| `refs/eight` | 774.25 | 643.92 | 1.202x | 1.111-1.632x |
| `refs/unicode` | 21,165.46 | 20,327.76 | 1.041x | 0.870-1.151x |
| `refs/large` | 1,376,621.62 | 1,387,171.38 | 0.992x | 0.932-1.117x |
| `refs/huge` | 5,867,550.31 | 5,836,628.31 | 1.005x | 0.971-1.123x |

[Complete step data](2026-10-26-chart-collect.tsv).

## Original/final measurements

Twelve fresh processes per version compare all three changes with the original runtime. [Complete data](2026-10-26-chart-final.tsv).

| Fixture | Before cycles | After cycles | CPU throughput | Before -> after ns | Paired range |
| --- | ---: | ---: | ---: | ---: | ---: |
| `compare/early` | 34,229.14 | 2,818.34 | 12.145x | 15,602.74 -> 1,278.90 | 9.218-14.996x |
| `compare/equal` | 20,135.69 | 24,103.84 | 0.835x | 9,185.55 -> 10,988.28 | 0.737-1.147x |
| `compare/late` | 141,963.35 | 115,515.31 | 1.229x | 64,779.29 -> 52,698.83 | 1.085-1.712x |
| `compare/unicode` | 89,579.15 | 59,223.85 | 1.513x | 40,837.89 -> 27,026.17 | 1.276-2.032x |
| `compare/short` | 4,015.31 | 2,900.66 | 1.384x | 1,823.83 -> 1,316.40 | 1.009-1.786x |
| `filter/standard` | 437.12 | 38.91 | 11.236x | 199.17 -> 17.77 | 6.204-14.506x |
| `refs/standard` | 418.42 | 48.88 | 8.561x | 185.94 -> 17.19 | 7.484-10.815x |
| `sort/standard` | 431.28 | 44.59 | 9.672x | 191.41 -> 15.23 | 8.655-12.198x |
| `filter/wrong` | 33.44 | 31.25 | 1.070x | 14.89 -> 13.89 | 0.882-1.916x |
| `refs/wrong` | 66.88 | 42.88 | 1.560x | 25.39 -> 14.84 | 1.321-1.913x |
| `sort/wrong` | 78.88 | 36.02 | 2.190x | 31.25 -> 11.72 | 1.782-2.684x |
| `filter/one` | 254.17 | 64.73 | 3.926x | 116.34 -> 29.12 | 3.050-6.176x |
| `refs/one` | 378.12 | 207.50 | 1.822x | 167.19 -> 89.45 | 1.551-2.204x |
| `sort/one` | 412.42 | 211.78 | 1.947x | 182.81 -> 91.80 | 1.647-2.382x |
| `filter/mixed` | 2,969.85 | 1,131.47 | 2.625x | 1,353.71 -> 515.09 | 2.138-3.280x |
| `refs/mixed` | 9,769.47 | 7,123.46 | 1.371x | 4,444.92 -> 3,239.45 | 1.002-1.732x |
| `sort/mixed` | 9,282.45 | 6,993.99 | 1.327x | 4,248.05 -> 3,180.86 | 0.981-1.794x |
| `filter/edits` | 1,660.24 | 1,419.78 | 1.169x | 756.42 -> 647.43 | 0.871-1.416x |
| `refs/edits` | 7,540.17 | 7,066.87 | 1.067x | 3,430.07 -> 3,214.84 | 0.846-1.436x |
| `sort/edits` | 7,530.74 | 6,987.14 | 1.078x | 3,432.42 -> 3,178.51 | 0.828-1.485x |
| `filter/five` | 235.36 | 214.94 | 1.095x | 106.91 -> 98.39 | 0.925-1.408x |
| `refs/five` | 595.05 | 472.44 | 1.260x | 266.80 -> 209.77 | 1.099-2.177x |
| `sort/five` | 449.29 | 395.27 | 1.137x | 200.00 -> 175.78 | 0.885-1.360x |
| `filter/eight` | 404.97 | 370.08 | 1.094x | 184.13 -> 168.28 | 0.806-1.276x |
| `refs/eight` | 792.26 | 633.63 | 1.250x | 356.25 -> 283.21 | 0.805-1.676x |
| `sort/eight` | 674.79 | 576.19 | 1.171x | 302.35 -> 257.42 | 0.932-1.378x |
| `filter/unicode` | 1,581.14 | 1,487.19 | 1.063x | 721.07 -> 678.15 | 0.820-1.253x |
| `refs/unicode` | 23,976.09 | 20,491.53 | 1.170x | 10,920.70 -> 9,340.62 | 0.908-1.353x |
| `sort/unicode` | 21,472.41 | 19,707.85 | 1.090x | 9,816.80 -> 9,010.15 | 0.903-1.425x |
| `filter/large` | 133,414.81 | 138,833.75 | 0.961x | 60,856.25 -> 63,125.00 | 0.748-1.130x |
| `refs/large` | 1,358,499.25 | 1,339,210.62 | 1.014x | 619,387.50 -> 610,787.50 | 0.815-1.167x |
| `sort/large` | 1,472,941.12 | 1,270,136.69 | 1.160x | 671,893.75 -> 579,087.50 | 0.847-1.283x |
| `filter/huge` | 569,588.69 | 571,166.50 | 0.997x | 259,931.25 -> 260,293.75 | 0.770-1.204x |
| `refs/huge` | 5,550,290.62 | 5,199,008.44 | 1.068x | 2,538,662.50 -> 2,376,325.00 | 0.875-1.237x |
| `sort/huge` | 6,130,182.12 | 5,394,953.31 | 1.136x | 2,797,756.25 -> 2,462,881.25 | 0.927-1.314x |

Slower combined medians are retained in the table: `compare/equal` 19.7% more cycles (0.835x); `filter/large` 4.1% more cycles (0.961x); `filter/huge` 0.3% more cycles (0.997x). Unchanged filter/index-sort controls vary across separately linked binaries and processes too. A paired range spanning 1x does not demonstrate a uniform gain. These measurements do not establish the cause of every shift.

Isolated step regressions matter as well: `filter`/`filter/large` 0.956x; `filter`/`filter/huge` 0.956x; `collect`/`refs/standard` 0.862x; `collect`/`refs/wrong` 0.830x; `collect`/`refs/one` 0.919x; `collect`/`refs/mixed` 0.895x; `collect`/`refs/large` 0.992x. The collection change is supported by five/eight-edit throughput and allocation savings; every list length is not faster.

## Allocations

All 35 workloads have invariant allocation counters across every process for a given version. Counts below probe one complete public reference-collection operation; allocations equal frees and requested bytes equal freed bytes in each cell.

| Fixture | Allocations/frees before -> after | Reallocations before -> after | Requested/freed bytes before -> after | Scoped peak bytes before -> after |
| --- | ---: | ---: | ---: | ---: |
| `refs/standard` | 0 -> 0 | 0 -> 0 | 0 -> 0 | 0 -> 0 |
| `refs/wrong` | 0 -> 0 | 0 -> 0 | 0 -> 0 | 0 -> 0 |
| `refs/one` | 1 -> 1 | 0 -> 0 | 32 -> 8 | 32 -> 8 |
| `refs/mixed` | 1 -> 1 | 3 -> 1 | 480 -> 384 | 256 -> 256 |
| `refs/edits` | 1 -> 1 | 3 -> 1 | 480 -> 384 | 256 -> 256 |
| `refs/five` | 1 -> 1 | 1 -> 0 | 96 -> 40 | 64 -> 40 |
| `refs/eight` | 1 -> 1 | 1 -> 0 | 96 -> 64 | 64 -> 64 |
| `refs/unicode` | 1 -> 1 | 3 -> 1 | 480 -> 384 | 256 -> 256 |
| `refs/large` | 2 -> 2 | 9 -> 7 | 49,120 -> 49,024 | 32,768 -> 32,768 |
| `refs/huge` | 2 -> 2 | 11 -> 9 | 196,576 -> 196,480 | 131,072 -> 131,072 |

Comparison and filter fixtures remain allocation-free. Index-list sorting retains the same allocation/reallocation/byte counts; its stable-sort scratch storage remains intact. Empty reference lists allocate nothing, single matches request eight rather than 32 bytes, and five/eight matches eliminate their growth reallocation. Larger reference lists retain stable-sort scratch storage and still grow after spilling; the table reports those costs.

## Method and verification

Windows x86_64 MSVC, Intel Xeon E5-2696 v4, rustc 1.99.1 / LLVM 23.1.1. The four archived unit-test binaries use `--release --locked`, repository-default LTO and unchanged CPU/static-runtime settings. The existing helper pins the measuring thread to logical processor 2 and uses `QueryThreadCycleTime`. Each process discards one warmup sample and retains the median of eleven measured samples. Numeric results are per complete fixture, not per element. The driver alternates fresh-process order: eight before/after pairs per isolated change and twelve pairs for the original/final comparison.

The fixed matrix contains 35 labels. Comparison fixtures contain 256 prepared string pairs and repeat 128 times per sample. Cases cover early differences in long ASCII strings, equal ASCII text with different case, late differences after long common prefixes, Unicode mappings and short names. Filter fixtures scan 5, 6, 8, 10, 32, 64, 2048 or 8192 real `ChartData` objects, repeating 2048 times per sample, or eight times for the two large fixtures. Public `edit_chart_indices_sorted` and `edit_charts_sorted` fixtures use those same songs and repeat 128 times, or eight times for the two large fixtures. Names, charts and all input construction remain outside timing; inputs and outputs use `black_box`. The large fixtures stress allocation and sorting scale and do not claim typical songs contain thousands of edits. The smaller fixtures include standard charts, wrong-type edits, a single edit, mixed charts, all edits and Unicode description ties.

The existing scoped System allocator measures one complete fixture separately from timing, with tracking disabled during timed samples. All allocator requests still pass through that wrapper. Requested and scoped peak bytes describe allocator requests, not size classes or process RSS. No compilation started by this task overlaps retained measurements; unrelated shared-machine activity is uncontrolled.

Frozen function bodies come from `2080a074c`. The comparator checks scan every valid Unicode scalar for changing lowercase mappings, construct prefixed versions of those mappings, compare them in both orders against representative ASCII/Unicode/expansion cases, test 100000 deterministic generated pairs and exercise common-prefix lengths up to 8192 bytes. Edit checks compare complete index lists and chart-reference identities against the original stable filtering/sorting for empty through 2048-chart inputs, eight chart distributions and five chart-type queries. They cover case differences, raw whitespace, non-ASCII difficulty markers and equal sort keys. Every archived stage passes both new correctness tests.

Source audits reconstruct the entire baseline `song.rs` by reversing only three runtime function bodies and removing its test registration, verify frozen bodies and unchanged harness/helper hashes, compare final production bytes with the measured snapshot and verify every raw record's executable digest. `background.rs`, build configuration and all excluded Lua crates remain unchanged by this pass.

Validation: **576 release tests pass** across chart/score/simfile (32 ignored), **305 debug tests pass** across chart/score (13 ignored), scoped rustfmt passes for every changed runtime body and both new Rust files, and chart/score/simfile Clippy passes with `--no-deps`. Existing warnings remain. Performance archives retain default release LTO; broad validation disables LTO to reduce linking time.

Four TSVs retain all **2,520 process/workload records**, elapsed times, allocation counters and executable SHA-256 values. Sources, archives, raw logs, audits and rejected screenings remain under `target/chart-shortcuts/`.

```powershell
cargo test --release --locked -p deadsync-chart --lib --no-run --message-format=json
python scripts/benchmark_sampling.py --runs 8 --test-filter benchmark_chart_shortcuts --log-dir target/chart-shortcuts/runs --before target/chart-shortcuts/before/chart.exe --after target/chart-shortcuts/compare/chart.exe --output docs/performance/2026-10-26-chart-compare.tsv
python scripts/benchmark_sampling.py --runs 8 --test-filter benchmark_chart_shortcuts --log-dir target/chart-shortcuts/runs --before target/chart-shortcuts/compare/chart.exe --after target/chart-shortcuts/filter/chart.exe --output docs/performance/2026-10-26-chart-filter.tsv
python scripts/benchmark_sampling.py --runs 8 --test-filter benchmark_chart_shortcuts --log-dir target/chart-shortcuts/runs --before target/chart-shortcuts/filter/chart.exe --after target/chart-shortcuts/after/chart.exe --output docs/performance/2026-10-26-chart-collect.tsv
python scripts/benchmark_sampling.py --runs 12 --test-filter benchmark_chart_shortcuts --log-dir target/chart-shortcuts/runs --before target/chart-shortcuts/before/chart.exe --after target/chart-shortcuts/after/chart.exe --output docs/performance/2026-10-26-chart-final.tsv
cargo test --release --locked --config profile.release.lto=false -p deadsync-chart -p deadsync-score -p deadsync-simfile -- --test-threads=1
cargo test --locked -p deadsync-chart -p deadsync-score -- --test-threads=1
cargo clippy --no-deps --release --locked --config profile.release.lto=false -p deadsync-chart -p deadsync-score -p deadsync-simfile --all-targets
```

Keep the committed harness/reference files fixed. Restore the three runtime bodies from `2080a074c` for `before`, retaining the test-module registration. Apply the comparator for `compare`, the edit predicate for `filter`, and reference collection for `after`. Build/archive the single reported unit-test executable at each stage and run its two ordinary correctness checks before measurements.
