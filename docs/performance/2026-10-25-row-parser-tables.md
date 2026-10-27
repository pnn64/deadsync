# Note rows, compact note parsing, and safe timing tables - 2026-10-25

Three safe Rust optimizations based on `669eb598a` and the hot-path, throughput, indirection, and unsafe guidance in `rust-performance.md`. Both Lua crates and concurrent unrelated edits are excluded. No dependency, public interface, or target CPU requirement changes.

Measurements use Rust 1.99.1 / LLVM 23.1.1, Windows x86-64 MSVC, Intel Xeon E5-2696 v4 @ 2.20 GHz. Builds use the repository release profile: opt-level 3, full LTO, baseline CPU, static CRT. The existing `kernel_bench.rs` pins its thread to logical CPU 2 and reads `QueryThreadCycleTime`. Each process takes one warmup and eleven measured samples, selecting the median CPU-cycle sample. The tables take the median across four separate process runs per version, alternating before/after order. Builds and correctness tests finish before measurement runs. Fixture setup and allocation probes stay outside timing.

These are kernel CPU-throughput measurements, not application FPS or whole-library loading estimates. Raw records also contain elapsed nanoseconds, allocation/free/reallocation counts, byte counts, peak live bytes, and executable SHA-256 hashes.

1. `deadsync-core/src/timing.rs`: round note rows using f32's unit spacing above 2^23. Add/subtract that exponent bias for smaller magnitudes, restore the sign, and preserve the saturating integer cast. Larger magnitudes are already integral. This replaces general nearest-even rounding with direct floating-point operations. Throughput improves **1.259x** geometrically across all three fixtures. One operation converts 4,096 beats through opaque production calls. [Raw row measurements](2026-10-25-row-parser-rows.tsv).

| Workload | Before cycles/op | After cycles/op | CPU throughput |
| --- | ---: | ---: | ---: |
| rows/bits/4096 | 42,536.02 | 36,239.89 | 1.174x |
| rows/chart/4096 | 42,170.67 | 31,918.48 | 1.321x |
| rows/signed/4096 | 41,976.96 | 32,645.14 | 1.286x |

2. `deadsync-chart/src/notes.rs`: classify each note byte once, then share hold invalidation and note construction. Zip columns with their head slots to remove repeated indexing. Replace two-word `Option<usize>` heads with one-word indices using `usize::MAX`, which cannot index any Vec element. On this target the ten-slot scratch table shrinks from 160 to 80 bytes; wider heap-backed head tables also halve their element size. Remove `invalidate_hold` and its repeated call sites. Output ordering, callbacks, hold blocking/tails, exact result-vector capacity, row advancement, and malformed-input handling remain unchanged.

Complete parsing and destruction of each 8,192-row chart improves **1.116x** geometrically across all sixteen workloads. Sparse/hold-heavy charts benefit most. Four-lane dense and invalid charts are respectively 3.8% and 2.1% slower, and five-lane dense charts are 1.3% slower; retain those controls alongside the gains. [Raw parser measurements](2026-10-25-row-parser-notes.tsv).

| Workload | Before cycles/op | After cycles/op | CPU throughput |
| --- | ---: | ---: | ---: |
| notes/10/dense | 3,757,822.42 | 3,666,830.96 | 1.025x |
| notes/10/holds | 567,293.33 | 484,011.77 | 1.172x |
| notes/10/invalid | 3,703,970.30 | 3,570,021.54 | 1.038x |
| notes/10/sparse | 502,631.95 | 381,038.83 | 1.319x |
| notes/4/dense | 1,563,561.08 | 1,625,340.42 | 0.962x |
| notes/4/holds | 314,437.06 | 265,418.30 | 1.185x |
| notes/4/invalid | 775,655.92 | 792,478.42 | 0.979x |
| notes/4/sparse | 297,027.41 | 257,059.74 | 1.155x |
| notes/5/dense | 1,968,553.92 | 1,995,005.86 | 0.987x |
| notes/5/holds | 397,543.05 | 291,657.34 | 1.363x |
| notes/5/invalid | 1,899,580.48 | 1,832,334.42 | 1.037x |
| notes/5/sparse | 336,618.62 | 273,049.23 | 1.233x |
| notes/8/dense | 3,064,642.57 | 2,982,562.75 | 1.028x |
| notes/8/holds | 544,524.66 | 465,887.66 | 1.169x |
| notes/8/invalid | 3,069,136.77 | 3,025,421.20 | 1.014x |
| notes/8/sparse | 439,793.48 | 335,567.20 | 1.311x |

3. `deadsync-rules/src/timing.rs`: collect exact-size slice maps directly into `Arc<[T]>` for BPM points, scroll prefixes, and speed runtimes. Remove the custom `exact_arc` initializer, its uniqueness check, manual slot writes, and `unsafe assume_init`. The installed standard library specializes these trusted-length iterators to one Arc allocation. Direct slice iteration also removes index generation and source indexing; assign speed runtime directly to its destination. Initialization order and floating-point operations are unchanged.

Throughput improves **1.042x** geometrically across seven workloads: 11.3% for one-entry BPM tables and 5.8% for complete timing construction with 256 scroll and speed segments. The one-segment complete-build control is effectively unchanged. Each operation includes destruction. [Raw timing-table measurements](2026-10-25-row-parser-tables.tsv).

| Workload | Before cycles/op | After cycles/op | CPU throughput |
| --- | ---: | ---: | ---: |
| bpm/1 | 219.14 | 196.89 | 1.113x |
| bpm/16 | 247.84 | 237.97 | 1.042x |
| bpm/256 | 1,238.55 | 1,229.12 | 1.008x |
| bpm/4096 | 18,491.15 | 17,247.43 | 1.072x |
| timing-build/1 | 1,707.70 | 1,704.78 | 1.002x |
| timing-build/16 | 6,242.94 | 6,202.70 | 1.006x |
| timing-build/256 | 67,243.46 | 63,531.71 | 1.058x |

Allocation counts and byte counts are unchanged in every measured workload. Rows remain allocation-free. BPM tables retain one allocation/free and zero reallocations (24, 144, 2,064, and 32,784 bytes). Complete timing builds retain six allocations/frees and zero reallocations (192, 1,208, and 17,528 bytes). Sparse/hold chart fixtures retain one 81,920-byte allocation/free; dense fixtures retain one 1,310,720-3,276,800-byte allocation/free. Invalid-hold fixtures retain two allocations/frees and 11-13 reallocations with identical churn and peak bytes. The measured chart fixtures use at most ten lanes, so their head tables stay on the stack.

Two custom helpers and duplicated note-dispatch paths are removed. Across the three production files there are eight fewer nonblank production lines, excluding test-module declarations and test/benchmark code. No new unsafe code is introduced; the manual Arc initializer's unsafe block is eliminated. Existing allocation and timing helpers are shared at the rules test-crate root so each is compiled once.

All final comparisons use archived binaries. The row baseline has the original rounding function. The final parser baseline restores only the original note parser while retaining the row and timing-table changes. The final timing-table baseline restores only the original initializer while retaining optimized row rounding. Baselines ran before edits; final archives were then compared in alternating process order. Final table builds use identical shared benchmark instrumentation on both sides.

The parser's flatten-only intermediate improved 1.066x geometrically but had weaker eight-lane results; it was superseded by compact heads. Its complete [intermediate measurements](2026-10-25-row-parser-flatten-only.tsv) are retained. A byte-color-packing trial replaced fused scaling with `(x as f64 * 255.0 + 0.5) as f32 as u8`. Boundary and one-million-input differential checks passed, but all five workloads were slower, with 0.951x geometric CPU throughput. That source change and its temporary harness were discarded; [rejected measurements](2026-10-25-row-parser-rejected-pack.tsv) preserve the evidence. The five TSVs contain 376 cycle/allocation records in total.

Correctness checks compare row conversion with the original nearest-even expression at positive/negative ties, around 2^23/2^24 and integer saturation boundaries, across all exponent/sign classes, and for one million generated f32 bit patterns. Parser checks compare every output field and vector capacity against the frozen scalar implementation: all 256 bytes between hold heads/tails across nine lane counts, plus existing arbitrary-byte and reservation tests. Existing timing tests cover BPM sorting, empty/default maps, duplicate/nonfinite inputs, offsets, stops, delays, warps, scroll prefixes, speed interpolation, and native float-bit parity.

Validation on retained source:

- Debug: 1,277 passed, 31 opt-in benchmarks ignored across core, rules, chart, gameplay, and simfile.
- Release: 187 passed, 12 opt-in benchmarks ignored across core, rules, and chart.
- Clippy on all targets of the three changed crates completes with existing warnings; the new duplicate-module warning was fixed by sharing the test helper. Changed blocks/new files are formatted; whitespace checks and Python compilation pass.

To reproduce, retain the added benchmark files/test-only module declarations in each source variant and restore only the production change being compared. Build with the same Cargo options, then copy compiler-artifact executable paths from the JSON output into separate archive directories. Name the executables `note_row_kernel.exe`, `note_scan_kernel.exe`, and `rules.exe` respectively.

```powershell
cargo test --release -p deadsync-core -p deadsync-rules -p deadsync-chart --no-run --message-format=json
python scripts/benchmark_row_parser_tables.py --before target/round-pack-parse/before --after target/round-pack-parse/retained --target rows --runs 4 --output target/row-parser-tables/rows.tsv
python scripts/benchmark_row_parser_tables.py --before target/round-pack-parse/isolated-before --after target/round-pack-parse/retained --target notes --runs 4 --output target/row-parser-tables/notes.tsv
python scripts/benchmark_row_parser_tables.py --before target/round-pack-parse/isolated-tables-before --after target/round-pack-parse/retained --target creation --runs 4 --output target/row-parser-tables/tables.tsv
cargo test -p deadsync-core -p deadsync-rules -p deadsync-chart -p deadsync-gameplay -p deadsync-simfile
cargo test --release -p deadsync-core -p deadsync-rules -p deadsync-chart
cargo clippy -p deadsync-core -p deadsync-rules -p deadsync-chart --all-targets
```
