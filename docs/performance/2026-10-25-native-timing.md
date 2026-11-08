# Native timing and chart note counting ? 2026-10-25

Three safe Rust optimizations based on `18ec14347356b8e2811cff98a923289ab97f6e0e`. Both Lua crates and concurrent unrelated edits are excluded. No dependencies, public interfaces, target CPU requirements, or production `unsafe` were added.

Measurements use Rust 1.99.1 / LLVM 23.1.1 on Windows x86-64 MSVC, Intel Xeon E5-2696 v4 @ 2.20 GHz. All archives use the repository release profile (opt-level 3, full LTO, static CRT, baseline CPU). The existing `kernel_bench.rs` pins each benchmark thread to logical CPU 2 and reads Windows `QueryThreadCycleTime`: one warmup followed by eleven samples, taking the median CPU-cycle sample. Tables below take the median across four separate process runs per stage, rotating process order. Fixture preparation stays outside timing. Timing operations contain 4,096 opaque production calls; note operations parse and destroy an entire 8,192-row chart. These are kernel throughput measurements, not application FPS claims.

The immutable `tests/perf/core_alloc.rs` probe measures allocations separately. Every timing workload remains allocation-free. Every note workload retains exactly one allocation and one free, zero reallocations, and identical byte counts: 81,920 bytes for sparse/hold fixtures, and 1,310,720?3,276,800 bytes for dense fixtures. All 180 retained timing/allocation records and executable SHA-256 hashes are in [timing stages](2026-10-25-native-timing-stages.tsv) and [note scans](2026-10-25-native-timing-notes.tsv); elapsed nanoseconds are included as secondary measurements.

`before` precedes every change. `delta` adds only the timestamp optimization. `final` includes the timing classifier change; the final note executable includes all three changes. The note baseline archive is named `classify` and contains the original note counter. Initial baselines ran before each edit; archived binaries were then rerun in rotated order. Rejected overlapping benchmark runs were rerun sequentially and are excluded from these records.

1. `deadsync-core/src/song_time.rs`: replace 128-bit subtraction/conversion with `i64::abs_diff`, a `u64` magnitude conversion, and sign restoration. The full difference fits `u64`, even for MIN/MAX pairs. Preserve the original f64 division and final f32 conversion, including positive zero for equal timestamps. Isolated CPU throughput improves 3.460x geometrically across the three fixtures.

| Workload | Before cycles/op | After cycles/op | CPU throughput |
| --- | ---: | ---: | ---: |
| delta/extreme/4096 | 90,302.13 | 25,479.51 | 3.544x |
| delta/mixed/4096 | 90,562.74 | 26,262.68 | 3.448x |
| delta/nearby/4096 | 88,802.84 | 26,208.96 | 3.388x |

2. `deadsync-rules/src/timing.rs`: classify offsets with a native unsigned magnitude and explicitly reject negative windows. Read thresholds directly instead of constructing a five-entry tuple table on every call; only the constant result pairs remain. Window order, disabled-window behavior, and `i64::MIN` behavior are unchanged. Isolated CPU throughput improves 1.629x geometrically.

| Workload | Before cycles/op | After cycles/op | CPU throughput |
| --- | ---: | ---: | ---: |
| classify/disabled/4096 | 80,530.65 | 49,281.70 | 1.634x |
| classify/hits/4096 | 39,513.51 | 28,471.56 | 1.388x |
| classify/negative/4096 | 94,570.31 | 56,632.54 | 1.670x |
| classify/spread/4096 | 91,839.57 | 49,331.96 | 1.862x |

The timing TSV also retains unchanged controls. Between `delta` and `final`, timestamp kernel medians vary from 0.987x to 1.002x for mixed/extreme inputs and 0.927x for nearby inputs despite identical timestamp source. Call layout and machine variation affect short kernels; the isolated before/after columns above are used for attribution.

3. `deadsync-chart/src/notes.rs`: count note bytes eight at a time using safe fixed-size chunks, exact zero-byte masks, and a horizontal byte sum. Six word comparisons replace up to 72 scalar byte comparisons. Keep the original predicate for the final 0?7 bytes. Case-folded control bytes are excluded by their original bit 5; masked addition prevents inter-byte carries. This preserves the exact reservation, parsing order, and resulting notes. No table, allocation, alignment requirement, CPU dispatch, or `unsafe` is introduced. Full-parser throughput improves 1.055x geometrically across twelve workloads, with 1.247x on sparse ten-lane charts and 1.125x on eight-lane holds. Four-lane dense and eight-lane sparse controls are respectively 2.6% and 1.4% slower; dense results are mostly limited by note construction and allocation. Retained benefits are concentrated in sparse ten-lane and hold-heavy inputs.

| Workload | Before cycles/op | After cycles/op | CPU throughput |
| --- | ---: | ---: | ---: |
| notes/10/dense | 3,792,696.60 | 3,723,244.58 | 1.019x |
| notes/10/holds | 583,133.58 | 531,304.15 | 1.098x |
| notes/10/sparse | 537,596.09 | 431,263.72 | 1.247x |
| notes/4/dense | 1,614,828.58 | 1,656,160.46 | 0.975x |
| notes/4/holds | 299,720.66 | 288,472.38 | 1.039x |
| notes/4/sparse | 277,672.97 | 261,637.40 | 1.061x |
| notes/5/dense | 2,021,951.65 | 1,979,990.98 | 1.021x |
| notes/5/holds | 399,677.68 | 389,362.30 | 1.026x |
| notes/5/sparse | 305,019.40 | 289,093.58 | 1.055x |
| notes/8/dense | 3,099,352.04 | 2,993,502.60 | 1.035x |
| notes/8/holds | 546,703.16 | 485,857.78 | 1.125x |
| notes/8/sparse | 440,039.32 | 446,325.83 | 0.986x |

The classifier removes 24 nonblank production lines. Timestamp conversion adds four and word counting adds 21, including comments explaining the arithmetic: one additional nonblank production line overall. Runtime simplification removes wide arithmetic, the temporary tuple table, and most per-byte dispatch. Test and measurement code is separate from these counts.

Two alternatives were rejected: a scalar bitwise predicate lowered full-parser throughput (about 0.79?0.98x), and merging the L/M word tests reduced the measured geometric benefit to about 1.032x. Their complete records are preserved in [scalar predicate trial](2026-10-25-native-timing-rejected-predicates.tsv) and [merged comparison trial](2026-10-25-native-timing-rejected-merged.tsv). The latter trial's `final` archive is identified by its different binary hash; it is not the retained executable.

Correctness: timestamp results are compared bit-for-bit with the original wide calculation at integer extremes, around powers of two, across their Cartesian product, and for 100,000 generated pairs. Classification checks every disabled-window mask and threshold neighbor, including negative, unsorted, absent, and extreme windows. Note reservations and results are compared against the original count for all 256 bytes, seven lane counts, and twenty lengths (35,840 fixtures), plus the existing arbitrary-byte parser differential tests.

Validation completed on the retained code:

- Debug: 1,275 passed, 29 opt-in benchmarks ignored across core, rules, chart, gameplay, and simfile.
- Release: 185 passed, 10 opt-in benchmarks ignored across the three changed crates.
- Clippy on all targets of the three changed crates passes with existing warnings; changed code introduces no new warnings. New/edited small files and the classifier block pass formatting checks; the complete timing.rs file has pre-existing formatting differences outside this change. Whitespace checks pass.

Reproduce the builds at each source stage with these added benchmark files present, then copy the JSON artifact executable paths into separate archive directories:

```powershell
cargo test --release -p deadsync-rules --test native_timing_kernel -p deadsync-chart --test note_scan_kernel --no-run --message-format=json
python scripts/benchmark_native_timing.py --archive before=target/native-timing/before --archive delta=target/native-timing/delta --archive final=target/native-timing/final --target timing --runs 4 --output target/native-timing/final-timing.tsv
python scripts/benchmark_native_timing.py --archive classify=target/native-timing/classify --archive final=target/native-timing/final --target notes --runs 4 --output target/native-timing/retained-notes.tsv
cargo test -p deadsync-core -p deadsync-rules -p deadsync-chart -p deadsync-gameplay -p deadsync-simfile
cargo test --release -p deadsync-core -p deadsync-rules -p deadsync-chart
cargo clippy -p deadsync-core -p deadsync-rules -p deadsync-chart --all-targets
```
