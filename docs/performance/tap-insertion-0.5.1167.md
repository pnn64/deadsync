# Tap insertion performance - 0.5.1167

Baseline: `1d8ef2f51` (`0.5.1166`), measured on 2026-09-12. This pass increments the patch exactly once to `0.5.1167`, including the three inherited package versions in Cargo.lock.

## Three optimizations

The local `rust-performance.md` guidance on measuring CPU and allocation costs (M-HOTPATH), reusing owned memory (M-MEM-REUSE), reserving useful capacity (M-INITIAL-CAPACITY), and batching work for throughput (M-THROUGHPUT) motivates these changes.

1. **Batch Wide additions.** Eligible taps are appended to the existing chart allocation, then sorted once. Replacements update their existing cell. Previously, every insertion moved the remaining chart. A forward row cursor also replaces the two searches that rediscovered each completed row. Added taps share an already occupied row, so later spacing checks can search the original sorted prefix. Hold tracking incorporates both replacements and appended taps.
2. **Batch Stomp additions.** Mirrored taps use the same storage strategy, with Stomp's tap/lift spacing and held-lane rules preserved. Every added tap shares a row with a preexisting nonfake tap/lift, so appending it does not change the answer to subsequent spacing queries. This removes repeated tail moves and completed-row searches.
3. **Batch Echo additions.** Echoes are appended and sorted once. The most recent echo is folded into the next grid row's summary and hold state, preserving chained echoes. Future spacing searches use the original prefix: earlier appended echoes are behind the current row. Echo generation still respects the original end-row limit, including the possible final echo beyond it.

All three paths defer reservations until the first actual addition. Charts admitting no additions, or only replacements, need no additional storage. Sufficiently preallocated input vectors transform without allocation, reallocation, or deallocation. Growing outputs still need storage; the ordinary growth budgets are unchanged. No scratch vector, persistent cache, dependency, or unsafe production code is added.

The batched paths require strictly increasing `(row, column)` keys. This guarantees that the final unstable sort cannot reorder equal cells. Duplicate cells and unsorted inputs retain the previous sorted or unordered implementation, respectively. Their allocation policy is also unchanged.

These modifiers run during chart preparation, including attack transforms. The measurements concern those operations, not frame rate, audio latency, or total song loading time.

## Behavior and project checks

- 784 gameplay unit tests pass in debug and release. Six manual benchmarks are ignored by ordinary runs, including the new benchmark.
- Six new behavior/allocation tests cover known output cells and Echo chains; generated charts over six lane counts, two offsets and 32 seeds; holds/rolls, fake notes, replacement cells, foreign lanes and note metadata; duplicate and reversed inputs; empty charts, invalid lane counts and missing row timing; fake/warp timing, saturating column offsets and repeated combinations of the transforms.
- Full note output is compared with frozen parent functions, including hold and judgment state. Preallocated productive inputs, dense inputs admitting no additions, empty inputs, and one-lane replacement-only Wide/Stomp inputs have explicit no-churn assertions. Productive preallocated inputs also retain their original pointer and capacity.
- `cargo test -p deadsync-gameplay --lib --locked`, its `--release` equivalent, `cargo clippy -p deadsync-gameplay --all-targets --locked -- -D clippy::perf`, and `cargo check --all-targets --locked` pass. Existing unrelated style warnings remain.

## Measurement method

Windows x86_64, Intel Xeon E5-2696 v4 @ 2.20 GHz, rustc 1.98.0 (LLVM 22.1.8). Release opt-level 3 with full LTO. Parent public dispatchers and sorted implementations are frozen in `crates/deadsync-gameplay/tests/perf/tap_insertion/baseline.rs`; their shared helpers and unordered fallbacks are unchanged from the parent. Both variants execute in the same test binary.

Reproduce with:

```text
cargo test -p deadsync-gameplay --lib --release --locked tap_insertion_bench -- --ignored --test-threads=1 --nocapture
```

Run three times, setting `DEADSYNC_PERF_REVERSE=1` for the middle run and unsetting it for the others. Builds and other Cargo work finish before timing. Each case has three warmups, seven timing batches, and separate allocation counting using `tests/support/perf.rs`. Tables report the median of the three per-run medians. The harness contains the iteration counts, from 10 to 20,000 depending on workload.

Fixture creation, timing-data construction, and correctness comparisons are outside measurement. Both measured closures clone the same source vector to supply a fresh owned chart, then transform and destroy it. The tables include this identical fixture-cloning cost and returned-output destruction. The no-churn tests separately verify transformation alone with adequate capacity.

Sparse fixtures cycle four lanes at 96-row (two-beat) spacing, making additions common; the 4,096-note case is a long-chart stress case. Dense fixtures use 12-row spacing, admitting no additions. Mixed fixtures include a hold every fifth note and fake notes every seventh non-hold; holds last 144 rows. Duplicate fixtures add a duplicate cell halfway through 512 notes; reverse fixtures reverse 128 sparse notes. Throughput counts source notes/s, including duplicate or filtered items; empty controls use operations/s.

Windows `QueryThreadCycleTime` measures calling-thread CPU cycles, not retired instructions or elapsed timestamp-counter ticks. The allocator records calls and cumulative requested/freed bytes, including reallocations; these are allocation traffic rather than RSS, peak live memory, or measured memory-copy bandwidth. Timing is single-threaded and allocation counting is disabled during timing. This is a synthetic operation benchmark, not a whole-game profile.

## Results

Positive cycle savings indicate an improvement on this workload and machine.

| Scenario | Old us/op | New us/op | Old cycles/op | New cycles/op | Fewer cycles |
|---|---:|---:|---:|---:|---:|
| wide_empty | 0.0112 | 0.0115 | 24.7 | 25.3 | -2.43% |
| wide_tiny | 0.5317 | 0.4506 | 1,166.3 | 987.6 | 15.32% |
| wide_sparse_512 | 523.7130 | 82.2410 | 1,147,982.8 | 180,233.6 | 84.30% |
| wide_sparse_4096 | 45,343.6000 | 1,360.6800 | 99,373,356.2 | 2,981,512.6 | 97.00% |
| wide_dense_4096 | 647.3760 | 230.1780 | 1,418,452.9 | 504,551.5 | 64.43% |
| wide_mixed_512 | 300.7070 | 72.5430 | 659,099.2 | 159,060.7 | 75.87% |
| wide_duplicates_512 | 566.6390 | 558.2490 | 1,241,742.2 | 1,223,339.4 | 1.48% |
| wide_reverse_128 | 146.3720 | 148.1300 | 321,010.0 | 324,473.7 | -1.08% |
| stomp_empty | 0.0099 | 0.0101 | 21.8 | 22.3 | -2.29% |
| stomp_tiny | 0.5140 | 0.4387 | 1,127.7 | 962.5 | 14.65% |
| stomp_sparse_512 | 520.7100 | 85.6990 | 1,141,340.7 | 187,935.9 | 83.53% |
| stomp_sparse_4096 | 43,882.1400 | 1,074.3200 | 96,164,728.3 | 2,356,134.9 | 97.55% |
| stomp_dense_4096 | 779.2210 | 368.0680 | 1,707,694.6 | 806,451.8 | 52.78% |
| stomp_mixed_512 | 284.4020 | 58.0870 | 622,725.9 | 127,250.7 | 79.57% |
| stomp_duplicates_512 | 515.2020 | 523.6600 | 1,129,375.8 | 1,147,704.0 | -1.62% |
| stomp_reverse_128 | 130.0680 | 130.5160 | 285,271.0 | 286,267.5 | -0.35% |
| echo_empty | 0.0785 | 0.0144 | 172.1 | 31.6 | 81.64% |
| echo_tiny | 1.1938 | 0.8560 | 2,608.0 | 1,877.9 | 27.99% |
| echo_sparse_512 | 1,603.4720 | 282.0690 | 3,513,962.3 | 618,175.6 | 82.41% |
| echo_sparse_4096 | 136,178.3300 | 3,747.6500 | 298,440,058.1 | 8,202,451.6 | 97.25% |
| echo_dense_4096 | 467.1520 | 323.4560 | 1,023,719.5 | 708,901.6 | 30.75% |
| echo_mixed_512 | 1,592.9620 | 282.9110 | 3,491,349.5 | 620,149.0 | 82.24% |
| echo_duplicates_512 | 1,598.4600 | 1,596.4590 | 3,502,480.2 | 3,499,091.2 | 0.10% |
| echo_reverse_128 | 845.4540 | 846.5800 | 1,853,440.5 | 1,855,732.0 | -0.12% |

A/R/F means allocation/reallocation/free calls. Byte totals include reallocations and the shared input clone.

| Scenario | Old A/R/F | New A/R/F | Old requested/freed B | New requested/freed B | Old units/s | New units/s |
|---|---:|---:|---:|---:|---:|---:|
| wide_empty | 0/0/0 | 0/0/0 | 0/0 | 0/0 | 89,245,872.4 | 87,108,013.9 |
| wide_tiny | 1/1/1 | 1/1/1 | 1,440/1,440 | 1,440/1,440 | 7,523,039.3 | 8,876,855.8 |
| wide_sparse_512 | 1/1/1 | 1/1/1 | 184,320/184,320 | 184,320/184,320 | 977,634.7 | 6,225,605.2 |
| wide_sparse_4096 | 1/1/1 | 1/1/1 | 1,474,560/1,474,560 | 1,474,560/1,474,560 | 90,332.5 | 3,010,259.6 |
| wide_dense_4096 | 1/1/1 | 1/0/1 | 1,474,560/1,474,560 | 491,520/491,520 | 6,327,080.4 | 17,794,923.9 |
| wide_mixed_512 | 1/1/1 | 1/1/1 | 184,320/184,320 | 184,320/184,320 | 1,702,654.1 | 7,057,882.9 |
| wide_duplicates_512 | 1/1/1 | 1/1/1 | 184,680/184,680 | 184,680/184,680 | 905,338.3 | 918,944.8 |
| wide_reverse_128 | 2/1/2 | 2/1/2 | 47,104/47,104 | 47,104/47,104 | 874,484.2 | 864,105.9 |
| stomp_empty | 0/0/0 | 0/0/0 | 0/0 | 0/0 | 101,061,142.0 | 98,814,229.2 |
| stomp_tiny | 1/1/1 | 1/1/1 | 1,440/1,440 | 1,440/1,440 | 7,782,555.4 | 9,117,848.2 |
| stomp_sparse_512 | 1/1/1 | 1/1/1 | 184,320/184,320 | 184,320/184,320 | 983,272.8 | 5,974,398.8 |
| stomp_sparse_4096 | 1/1/1 | 1/1/1 | 1,474,560/1,474,560 | 1,474,560/1,474,560 | 93,340.9 | 3,812,644.3 |
| stomp_dense_4096 | 1/1/1 | 1/0/1 | 1,474,560/1,474,560 | 491,520/491,520 | 5,256,531.8 | 11,128,378.5 |
| stomp_mixed_512 | 1/1/1 | 1/1/1 | 184,320/184,320 | 184,320/184,320 | 1,800,268.6 | 8,814,364.7 |
| stomp_duplicates_512 | 1/1/1 | 1/1/1 | 184,680/184,680 | 184,680/184,680 | 995,725.9 | 979,643.3 |
| stomp_reverse_128 | 2/1/2 | 2/1/2 | 47,104/47,104 | 47,104/47,104 | 984,100.6 | 980,722.7 |
| echo_empty | 1/0/1 | 0/0/0 | 480/480 | 0/0 | 12,736,419.8 | 69,637,883.0 |
| echo_tiny | 1/2/1 | 1/2/1 | 3,360/3,360 | 3,360/3,360 | 3,350,785.3 | 4,672,624.3 |
| echo_sparse_512 | 1/2/1 | 1/2/1 | 430,080/430,080 | 430,080/430,080 | 319,307.1 | 1,815,158.7 |
| echo_sparse_4096 | 1/2/1 | 1/2/1 | 3,440,640/3,440,640 | 3,440,640/3,440,640 | 30,078.2 | 1,092,951.6 |
| echo_dense_4096 | 1/1/1 | 1/0/1 | 1,474,560/1,474,560 | 491,520/491,520 | 8,768,024.1 | 12,663,237.0 |
| echo_mixed_512 | 1/2/1 | 1/2/1 | 430,080/430,080 | 430,080/430,080 | 321,413.8 | 1,809,756.4 |
| echo_duplicates_512 | 1/2/1 | 1/2/1 | 430,920/430,920 | 430,920/430,920 | 320,933.9 | 321,336.2 |
| echo_reverse_128 | 2/2/2 | 2/2/2 | 108,544/108,544 | 108,544/108,544 | 151,397.9 | 151,196.6 |

Per-run paired cycle savings for representative productive cases:

- `wide_sparse_4096`: 96.59%, 97.01%, 97.00%.
- `stomp_sparse_4096`: 97.76%, 97.58%, 97.55%.
- `echo_sparse_4096`: 97.40%, 97.17%, 97.25%.

Cases with higher median cycle counts are reported explicitly:

- `wide_empty`: 2.43% more cycles; elapsed 11.2 to 11.5 ns/op.
- `wide_reverse_128`: 1.08% more cycles; elapsed 146372.0 to 148130.0 ns/op.
- `stomp_empty`: 2.29% more cycles; elapsed 9.9 to 10.1 ns/op.
- `stomp_duplicates_512`: 1.62% more cycles; elapsed 515202.0 to 523660.0 ns/op.
- `stomp_reverse_128`: 0.35% more cycles; elapsed 130068.0 to 130516.0 ns/op.
- `echo_reverse_128`: 0.12% more cycles; elapsed 845454.0 to 846580.0 ns/op.

## Limits

The final sort replaces quadratic tail movement with bounded sorting work but still moves notes. Productive fixtures retain the same allocation traffic as their parent implementations; their primary gain is CPU/throughput. Dense fixtures remove an unused reallocation, reducing measured byte churn by two thirds when the shared input clone is included. With that clone outside the measured operation, the new dense transform has zero heap churn.

Duplicate and reversed inputs keep the original algorithms and gain no batching benefit; validating the new fast path adds a small scan before fallback. Empty-operation timings are only a few nanoseconds and fluctuate. No claim is made that every input is faster or that all of DeadSync is allocation-free.

Raw logs, source audits and per-run measurements are retained locally under ignored `target/tap-insertion-perf/`. The committed tests and baselines reproduce the comparisons. The four user-excluded files are outside this commit.
