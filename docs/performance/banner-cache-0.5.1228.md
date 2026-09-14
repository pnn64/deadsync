# Banner cache — 0.5.1228

Baseline: `91e9d8021` (0.5.1227). This pass applies the supplied
`rust-performance.md` guidance on measured hot paths (M-HOTPATH), reducing
temporary allocation churn (M-MEM-REUSE), and useful work per CPU cycle
(M-THROUGHPUT) to three operations in the dynamic image cache.

## Changes

1. **Cache loading:** retain the first cache metadata result for the freshness
   comparison, removing a second path-based cache metadata query. Loading still
   opens the file, checks its current length, and returns identical owned pixels.
2. **Cache prewarming:** validate a reusable cache without constructing and
   immediately discarding an image-sized pixel vector. Validation checks the
   same header, dimensions, arithmetic bounds, and exact length, then reads the
   complete payload through a fixed 64 KiB stack buffer. Truncation and payload
   read errors still cause rebuilding. Heap usage no longer scales with the
   image size on this path; each active validation uses the bounded stack buffer.
3. **Stale cache cleanup:** check entry names before constructing full paths or
   querying file metadata. Regular entries use their enumerated file type;
   symlinks and type-query errors retain the path-based check. Prefix matching
   no longer allocates a formatted string. Current files, directories,
   unrelated names, and exact lowercase extension rules retain their behavior.

These paths serve banner/background cache loading, prewarming, and cache writes.
The normal loading API must still allocate its returned pixel buffer. Filesystem
APIs also allocate on this Windows platform. Only the streaming validation core
is asserted to have zero heap churn; no claim of zero-allocation application
startup or improved gameplay frame rate is made.

## Measurements

| Operation and fixture | Fewer thread cycles | Throughput gain | Allocations/frees old → new | Allocated/freed bytes old → new |
|---|---:|---:|---:|---:|
| Load a 320×80 banner | 17.0% | 1.21x | 5 → 4 | 103,122 → 102,932 |
| Prewarm a 1024×512 image (2 MiB pixels) | 45.7% | 1.86x | 5 → 3 | 2,097,874 → 532 |
| Prune a shard with 512 unrelated files | 97.5% | 40.89x | 2,567 → 518 | 601,854 → 10,958 |

Prewarming reduces requested-byte churn by 99.97% for the 2 MiB fixture.
The 16 MiB fixture uses 50.0% fewer median cycles and doubles throughput.
At the ordinary 320×80 banner size, complete prewarming uses 19.3% fewer
cycles and 99.5% fewer allocated bytes. Cleanup that actually deletes four
stale files among 128 unrelated files uses 83.4% fewer cycles and 94.6% fewer
allocated bytes; deleting four files alone uses 16.1% fewer cycles.

There are limits to the gains. Isolated validation of a one-pixel image uses
3.4% more median cycles, and isolated 320×80 validation uses 2.1% more
(roughly 3 microseconds of wall time). It still removes the pixel allocation,
and complete prewarming improves by 18.0% and 19.3% respectively because of
metadata reuse. Normal loading of 2–16 MiB images is dominated by the retained
pixel allocation and read: they use 0.8% fewer and 0.4% more cycles respectively,
so no material speedup is claimed there. Their allocation count
still drops by one. Empty validation counters are unchanged.

The initial standard-copy experiment removed heap churn but increased large
validation CPU time. The final 64 KiB buffer avoids that regression. A separate
128/256 KiB buffer-size experiment did not establish an additional benefit;
the smaller bounded buffer was retained. Only the final production version's
five-process measurements appear below.

[Raw measurements: 210 rows, 21 workloads, five processes, both variants](banner-cache-0.5.1228.csv).

### Timing and throughput

Throughput is images/s for loading, prewarming, and validation; entries/s for
pruning, except for the empty scan's operations/s. Negative cycle reductions
mean the new variant used more cycles. Cycle and time medians are independent.

| Workload | ns old → new | Thread cycles old → new | Fewer cycles | Units/s old → new |
|---|---:|---:|---:|---:|
| `load/0x0` | 198,500.0 → 156,150.0 | 435,405.6 → 342,173.0 | 21.4% | 5,037.8 → 6,404.1 |
| `prewarm/0x0` | 203,756.2 → 156,381.2 | 444,610.9 → 340,266.1 | 23.5% | 4,907.8 → 6,394.6 |
| `validate/0x0` | 73,406.2 → 73,368.8 | 160,824.9 → 160,701.4 | 0.1% | 13,622.8 → 13,629.8 |
| `load/1x1` | 204,312.5 → 163,300.0 | 439,096.1 → 358,141.7 | 18.4% | 4,894.5 → 6,123.7 |
| `prewarm/1x1` | 201,037.5 → 166,543.8 | 439,932.9 → 360,885.4 | 18.0% | 4,974.2 → 6,004.4 |
| `validate/1x1` | 80,950.0 → 83,793.8 | 177,356.0 → 183,474.6 | -3.4% | 12,353.3 → 11,934.1 |
| `load/320x80` | 219,043.8 → 181,675.0 | 479,045.0 → 397,665.4 | 17.0% | 4,565.3 → 5,504.3 |
| `prewarm/320x80` | 218,275.0 → 175,062.5 | 475,272.4 → 383,425.4 | 19.3% | 4,581.4 → 5,712.2 |
| `validate/320x80` | 97,200.0 → 100,743.8 | 211,940.9 → 216,289.8 | -2.1% | 10,288.1 → 9,926.2 |
| `load/1024x512` | 1,288,818.8 → 1,283,168.8 | 2,797,966.5 → 2,775,577.4 | 0.8% | 775.9 → 779.3 |
| `prewarm/1024x512` | 1,339,912.5 → 721,437.5 | 2,898,936.4 → 1,574,157.9 | 45.7% | 746.3 → 1,386.1 |
| `validate/1024x512` | 1,189,343.8 → 646,950.0 | 2,587,315.1 → 1,405,252.8 | 45.7% | 840.8 → 1,545.7 |
| `load/2048x2048` | 7,903,393.8 → 7,859,762.5 | 17,054,354.6 → 17,118,613.2 | -0.4% | 126.5 → 127.2 |
| `prewarm/2048x2048` | 7,967,912.5 → 3,990,206.2 | 17,352,037.6 → 8,684,147.5 | 50.0% | 125.5 → 250.6 |
| `validate/2048x2048` | 7,763,531.2 → 3,944,706.2 | 16,886,574.1 → 8,639,245.9 | 48.8% | 128.8 → 253.5 |
| `prune/unrelated0` | 79,793.8 → 78,043.8 | 174,653.4 → 170,963.0 | 2.1% | 12,532.3 → 12,813.3 |
| `prune/unrelated1` | 135,925.0 → 82,312.5 | 293,348.1 → 180,813.1 | 38.4% | 7,357.0 → 12,148.8 |
| `prune/unrelated128` | 5,578,956.2 → 191,156.2 | 12,110,761.6 → 415,582.1 | 96.6% | 22,943.4 → 669,609.3 |
| `prune/unrelated512` | 21,897,481.2 → 535,487.5 | 47,755,915.9 → 1,174,201.6 | 97.5% | 23,381.7 → 956,138.1 |
| `prune/delete4-mixed0` | 1,291,743.8 → 1,086,318.8 | 2,788,871.0 → 2,340,651.6 | 16.1% | 3,096.6 → 3,682.2 |
| `prune/delete4-mixed128` | 7,450,700.0 → 1,262,918.8 | 16,279,135.0 → 2,701,029.6 | 83.4% | 17,716.5 → 104,519.8 |

### Allocation churn

| Workload | Allocations/frees old → new | Reallocations old → new | Allocated/freed bytes old → new |
|---|---:|---:|---:|
| `load/0x0` | 4 → 3 | 0 → 0 | 722 → 532 |
| `prewarm/0x0` | 4 → 3 | 0 → 0 | 722 → 532 |
| `validate/0x0` | 1 → 1 | 0 → 0 | 190 → 190 |
| `load/1x1` | 5 → 4 | 0 → 0 | 726 → 536 |
| `prewarm/1x1` | 5 → 3 | 0 → 0 | 726 → 532 |
| `validate/1x1` | 2 → 1 | 0 → 0 | 194 → 190 |
| `load/320x80` | 5 → 4 | 0 → 0 | 103,122 → 102,932 |
| `prewarm/320x80` | 5 → 3 | 0 → 0 | 103,122 → 532 |
| `validate/320x80` | 2 → 1 | 0 → 0 | 102,590 → 190 |
| `load/1024x512` | 5 → 4 | 0 → 0 | 2,097,874 → 2,097,684 |
| `prewarm/1024x512` | 5 → 3 | 0 → 0 | 2,097,874 → 532 |
| `validate/1024x512` | 2 → 1 | 0 → 0 | 2,097,342 → 190 |
| `load/2048x2048` | 5 → 4 | 0 → 0 | 16,777,938 → 16,777,748 |
| `prewarm/2048x2048` | 5 → 3 | 0 → 0 | 16,777,938 → 532 |
| `validate/2048x2048` | 2 → 1 | 0 → 0 | 16,777,406 → 190 |
| `prune/unrelated0` | 7 → 6 | 6 → 5 | 1,278 → 1,230 |
| `prune/unrelated1` | 12 → 7 | 11 → 5 | 2,451 → 1,249 |
| `prune/unrelated128` | 647 → 134 | 646 → 5 | 151,422 → 3,662 |
| `prune/unrelated512` | 2,567 → 518 | 2,566 → 5 | 601,854 → 10,958 |
| `prune/delete4-mixed0` | 31 → 30 | 26 → 25 | 6,730 → 6,062 |
| `prune/delete4-mixed128` | 671 → 158 | 666 → 25 | 156,874 → 8,494 |

All allocation counters are per completed operation and identical across the
five processes. Frees equal allocations, and freed bytes equal allocated bytes
in every row. Reallocation bytes are included in the byte totals.

## Method

- Windows x86-64, Intel Xeon E5-2696 v4 @ 2.20 GHz; 44 logical processors.
- Rust 1.98.0 (`88d9e12ae`, 2026-08-18), LLVM 22.1.8; workspace release profile,
  optimization level 3 and full LTO. Both variants run in the same executable
  with the shared counting wrapper around the system allocator.
- Test-only baseline function bodies are copied from the parent commit and
  checked against it after whitespace normalization. They retain their own
  cache loading, freshness, saving, and pruning call chain. Unchanged image
  decoding and raw file writing helpers are shared.
- Five fresh serial processes pinned to logical CPU 4, alternating old/new
  execution order. No build or test runs concurrently with recorded benchmarks.
- Each variant has three warmups and seven batches of 16 operations. Tables
  report the median of the five process medians. The CSV includes each process's
  timing range, thread cycles, throughput, and allocation counters. Thread cycles
  use Windows `QueryThreadCycleTime`, not wall-clock timestamp counter ticks.
- Fixture creation, timestamp changes, path construction by the harness, and
  correctness comparisons are outside measured operations. Pixel destruction
  is included. Mutating pruning fixtures restore four stale files before each
  operation, outside timing and allocation tracking; both variants include the
  same per-operation clock overhead.
- Loading and prewarming process one image per operation. Validation isolates
  the payload change from metadata reuse. Pruning throughput counts directory
  entries; the empty directory uses one operation as its unit.
- Files are warm in the OS cache. The experiment measures serial operation
  throughput, not cold storage latency or the parallel prewarm scheduler.
  Allocation counters record requested heap bytes and calls, not peak RSS,
  committed pages, cache misses, power use, or whole-process CPU consumption.
  Both validators read the same payload bytes; disk traffic is not reduced.

## Behavioral verification

- `cargo test -p deadlib-assets --locked --no-fail-fast`: 80 unit tests and six
  integration tests passed; two manual benchmarks ignored. Existing cache
  prewarming, cache write/prune, texture binding, and upload behavior tests pass.
- Targeted release tests: nine passed and the manual benchmark ignored.
- New old/new comparisons cover exact pixels, missing/equal/older/newer source
  timestamps, valid/corrupt/missing/directory caches, reuse and rebuild return
  values, decoding failures, disk contents, and stale variant preservation.
- Raw format checks cover all short header lengths, bad magic, empty images,
  dimension overflow, extreme dimensions, exact/truncated/trailing payloads,
  short and interrupted reads, injected I/O errors, and shrinking files.
  Payload tests exercise both sides of the 64 KiB chunk boundary.
- Cleanup tests cover current files, prefix boundaries, lowercase extension
  rules, invalid Unicode filenames, matching directories, and Windows directory
  junctions. File symlink tests are included, but this Windows account returned
  privilege error 1314, so those assertions could not run here. Unix branches
  and 32-bit execution were not exercised on this machine.
- Allocation assertions require reduced churn for loading on Windows,
  prewarming, and cleanup, plus zero heap churn for in-memory validation.
- `cargo check --locked` passed for the application.
- `cargo clippy -p deadlib-assets --lib --locked -- -D clippy::perf` passed;
  existing warnings in dependencies remain. Formatting checks on changed Rust
  files and `git diff --check` passed.
- The version is bumped exactly once, from 0.5.1227 to 0.5.1228, including the
  three workspace-version package entries in `Cargo.lock`.

Reproduce with:

```powershell
cargo test -p deadlib-assets --locked
cargo test -p deadlib-assets --release --lib --locked benchmark_banner_cache -- --ignored --nocapture --test-threads=1
```

For repeated measurements, build once, invoke that release test executable in
five fresh processes pinned to one CPU, and set `DEADSYNC_PERF_REVERSE=1` for
every second process.
