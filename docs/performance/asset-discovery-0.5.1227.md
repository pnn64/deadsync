# Asset discovery — 0.5.1227

Baseline: `0f2bf56e6` (0.5.1226). The supplied `rust-performance.md` calls for
measuring hot paths and allocation churn (M-HOTPATH), avoiding short-lived
allocations and copies (M-MEM-REUSE), and doing more useful work per CPU cycle
(M-THROUGHPUT). This pass targets three filesystem discovery operations used
while preparing sound and texture catalogs. The guide is excluded from the commit.

## Changes

1. **Sound-folder discovery:** check the entry's filename for an eligible `.ogg`
   extension and non-underscore stem before building its full path. Ordinary
   files use the type supplied by directory enumeration. Links and type-query
   errors retain `Path::is_file` behavior. Returned paths keep their original sort.
2. **Graphic-choice discovery:** reject filenames that fail the PNG or multiframe
   rule before checking their type. Use entry metadata for ordinary files and
   construct full paths only for retained choices. Case-insensitive duplicate
   handling, first-root precedence, label stripping, and Love-first sorting remain
   unchanged. The multiframe mode continues accepting hinted non-PNG names.
3. **Noteskin PNG discovery:** classify directories from entry metadata, and
   inspect other entries' extensions before allocating full paths. Depth-first
   traversal order, hidden/staging and `pack.json` exclusions, canonical keys,
   and first-key precedence remain unchanged. Links and type-query errors use
   the original path-based directory check. The walker still includes dangling
   PNG paths that are not directories, matching its existing behavior.

These changes reduce repeated metadata queries and temporary path work during
catalog discovery. Existing cached catalog reads retain their behavior.
Filesystem enumeration, output paths, and output collections still allocate;
this is not a claim of zero-allocation discovery or improved gameplay frame rate.

## Results

For 128 accepted files mixed with 512 unrelated files:

| Operation | Fewer thread cycles | Throughput gain | Less allocated-byte churn |
|---|---:|---:|---:|
| Sound-folder listing | 97.1% | 34.4x | 80.4% |
| Graphic choices (multiframe) | 96.7% | 30.3x | 74.9% |
| Noteskin PNG discovery | 96.8% | 31.2x | 78.0% |

Allocation/free calls fall from 3,207 to 1,159 for sounds, 3,728 to 1,680
for graphics, and 3,476 to 1,428 for noteskins. Every nonempty measured
workload uses fewer median thread cycles. Accepted-only directories retain the
same allocation/reallocation counts while reducing requested-byte churn and
CPU work. The nested noteskin fixture improves throughput 15.4x and reduces
cycles 93.5%.

Empty-directory allocation counters are unchanged. Their timing sample ranges
overlap: the empty noteskin scan uses 2.9% more median cycles (2.7 microseconds
more wall time), and the other empty cases vary around the baseline. No
empty-directory speedup is claimed.

[Raw measurements: 210 rows, 21 workloads, five processes, both variants](asset-discovery-0.5.1227.csv).

### Timing and throughput

| Workload | ns old ? new | Thread cycles old ? new | Fewer cycles | Files/s old ? new |
|---|---:|---:|---:|---:|
| `audio/empty` | 83,231.2 → 79,543.8 | 182,857.2 → 174,310.4 | 4.7% | 12,014.7 → 12,571.7 |
| `audio/one` | 141,293.8 → 93,131.2 | 308,507.2 → 200,060.6 | 35.2% | 7,077.5 → 10,737.5 |
| `audio/accepted128` | 5,887,350.0 → 404,193.8 | 12,876,830.4 → 877,862.8 | 93.2% | 21,741.5 → 316,679.8 |
| `audio/mixed640` | 29,157,175.0 → 847,718.8 | 63,530,721.9 → 1,848,670.2 | 97.1% | 21,950.0 → 754,967.4 |
| `audio/rejected512` | 23,162,106.2 → 537,543.8 | 50,436,888.6 → 1,171,169.7 | 97.7% | 22,105.1 → 952,480.6 |
| `graphics/empty/multifalse` | 81,493.8 → 79,493.8 | 173,871.4 → 173,775.4 | 0.1% | 12,270.9 → 12,579.6 |
| `graphics/empty/multitrue` | 76,925.0 → 77,831.2 | 168,685.8 → 168,713.2 | -0.0% | 12,999.7 → 12,848.3 |
| `noteskin/empty` | 97,300.0 → 100,000.0 | 212,434.9 → 218,498.6 | -2.9% | 10,277.5 → 10,000.0 |
| `graphics/one/multifalse` | 142,012.5 → 90,687.5 | 310,921.8 → 198,729.8 | 36.1% | 7,041.6 → 11,026.9 |
| `graphics/one/multitrue` | 139,193.8 → 91,293.8 | 305,050.1 → 199,964.5 | 34.4% | 7,184.2 → 10,953.7 |
| `noteskin/one` | 157,437.5 → 111,262.5 | 343,544.9 → 243,795.9 | 29.0% | 6,351.7 → 8,987.8 |
| `graphics/accepted128/multifalse` | 6,394,712.5 → 546,812.5 | 13,921,595.4 → 1,190,650.3 | 91.4% | 20,016.5 → 234,083.9 |
| `graphics/accepted128/multitrue` | 6,355,743.8 → 553,906.2 | 13,825,001.7 → 1,209,911.4 | 91.2% | 20,139.3 → 231,086.0 |
| `noteskin/accepted128` | 6,132,250.0 → 508,531.2 | 13,341,278.8 → 1,098,583.8 | 91.8% | 20,873.3 → 251,705.3 |
| `graphics/mixed640/multifalse` | 29,682,093.8 → 1,002,631.2 | 64,596,613.1 → 2,190,486.6 | 96.6% | 21,561.8 → 638,320.4 |
| `graphics/mixed640/multitrue` | 29,651,900.0 → 979,493.8 | 64,483,584.8 → 2,133,704.7 | 96.7% | 21,583.8 → 653,398.8 |
| `noteskin/mixed640` | 29,463,500.0 → 943,887.5 | 64,116,059.3 → 2,047,166.8 | 96.8% | 21,721.8 → 678,046.9 |
| `graphics/rejected512/multifalse` | 23,686,681.2 → 543,468.8 | 51,534,319.8 → 1,178,632.6 | 97.7% | 21,615.5 → 942,096.5 |
| `graphics/rejected512/multitrue` | 23,487,000.0 → 516,050.0 | 51,131,948.7 → 1,128,312.3 | 97.8% | 21,799.3 → 992,151.9 |
| `noteskin/rejected512` | 23,741,687.5 → 549,225.0 | 51,698,519.6 → 1,192,776.9 | 97.7% | 21,565.4 → 932,222.7 |
| `noteskin/nested768` | 37,221,887.5 → 2,423,612.5 | 81,085,412.8 → 5,286,204.6 | 93.5% | 20,633.0 → 316,882.3 |

### Allocation churn

Counters are per completed operation and identical in all five processes.
Frees equal allocations, and freed bytes equal allocated bytes in every row.

| Workload | Allocations/frees old → new | Reallocations old → new | Allocated/freed bytes old → new |
|---|---:|---:|---:|
| `audio/empty` | 6 → 6 | 5 → 5 | 1,251 → 1,251 |
| `audio/one` | 12 → 12 | 10 → 10 | 2,567 → 2,411 |
| `audio/accepted128` | 647 → 647 | 650 → 650 | 161,379 → 141,411 |
| `audio/mixed640` | 3,207 → 1,159 | 3,210 → 650 | 769,635 → 150,627 |
| `audio/rejected512` | 2,566 → 518 | 2,565 → 5 | 609,507 → 10,467 |
| `graphics/empty/multifalse` | 7 → 7 | 5 → 5 | 1,368 → 1,368 |
| `graphics/empty/multitrue` | 7 → 7 | 5 → 5 | 1,368 → 1,368 |
| `noteskin/empty` | 12 → 12 | 10 → 10 | 2,582 → 2,582 |
| `graphics/one/multifalse` | 18 → 18 | 12 → 12 | 3,227 → 3,051 |
| `graphics/one/multitrue` | 18 → 18 | 12 → 12 | 3,227 → 3,051 |
| `noteskin/one` | 21 → 21 | 15 → 15 | 4,196 → 4,020 |
| `graphics/accepted128/multifalse` | 1,168 → 1,168 | 906 → 906 | 226,724 → 204,196 |
| `graphics/accepted128/multitrue` | 1,168 → 1,168 | 906 → 906 | 226,724 → 204,196 |
| `noteskin/accepted128` | 916 → 916 | 655 → 655 | 192,578 → 170,050 |
| `graphics/mixed640/multifalse` | 3,728 → 1,680 | 3,466 → 906 | 850,340 → 213,412 |
| `graphics/mixed640/multitrue` | 3,728 → 1,680 | 3,466 → 906 | 850,340 → 213,412 |
| `noteskin/mixed640` | 3,476 → 1,428 | 3,215 → 655 | 816,194 → 179,266 |
| `graphics/rejected512/multifalse` | 2,567 → 519 | 2,565 → 5 | 624,984 → 10,584 |
| `graphics/rejected512/multitrue` | 2,567 → 519 | 2,565 → 5 | 624,984 → 10,584 |
| `noteskin/rejected512` | 2,572 → 524 | 2,570 → 10 | 626,198 → 11,798 |
| `noteskin/nested768` | 4,508 → 2,451 | 4,787 → 1,715 | 1,785,744 → 641,352 |

## Method and scope

Windows x86_64 MSVC; Intel Xeon E5-2696 v4 @ 2.20 GHz; rustc 1.98.0
(88d9e12ae, 2026-08-18), LLVM 22.1.8. Release optimization level 3 and full LTO.
Test-only frozen bodies match the baseline Git source, ignoring visibility and
formatting. Unchanged filename, canonical-key, and sorting helpers are shared.

Five fresh processes run the same release test executable on logical CPU 4.
Old/new order alternates between processes. Each variant has three warmups and
seven samples of 16 complete operations, followed by a separate allocation-counted
operation. Results aggregate the five per-process medians. Function pointers and
inputs are opaque to the optimizer. Fixture creation and cleanup are outside
measurement; returned values and temporary root-path copies are destroyed inside
measurement. No builds or other tests run alongside these benchmarks.

The 21 workloads cover empty and single-entry directories, 128 accepted files,
128 accepted files mixed with 512 unrelated files, and 512 rejected files.
Graphics exercise both PNG and multiframe filters. A further noteskin workload
has eight nested skins containing 256 PNGs and 512 unrelated files. Throughput
counts fixture files processed per second, or completed scans for empty fixtures.
These are synthetic directories with warm filesystem caches, not measurements
of cold disks, entire application startup, or all supported operating systems.

CPU counts use Windows `QueryThreadCycleTime` for the calling thread. The
TLS-scoped System allocator wrapper records allocations, reallocations, frees,
and requested/freed bytes. Reallocations count separately and their replacement
buffers contribute to byte churn. Timing samples retain disabled-accounting TLS
checks on both sides. These counters measure churn, not retained heap size,
peak RSS, committed pages, cache misses, or physical disk traffic. Allocation
assertions are Windows-specific because path and metadata allocations vary by OS.

## Behavior and validation

Five new behavior tests compare old/new ordered outputs and exercise extension
case, underscore exclusions, Unicode and non-UTF filenames, duplicate roots,
missing roots and file-as-root errors, overlay precedence, multiframe names,
hidden folders, native packs, directory names ending in `.png`/`.ogg`, and
canonical-key filtering. Allocation assertions cover the three mixed-directory
operations; an empty-root noteskin call has no heap churn.

Valid and broken Windows directory junctions were exercised successfully.
Optional file-symlink cases reported Windows privilege error 1314 and were not
exercised on this machine. Tests provide Unix symlink and invalid-byte filename
coverage when run there. Directory-entry metadata describes the enumeration
snapshot; neither implementation guarantees a transactional view during edits.

- `cargo test -p deadsync-assets --locked --no-fail-fast -- --nocapture`:
  137 passed, four ignored, one failed across unit/integration/doc tests.
  All 132 unit tests passed. The existing `pack_refresh` integration test fails
  at its expectation that a preview is queued for render upload. The identical
  failure was reproduced after restoring all three changed asset source files
  from `0f2bf56e6`; this pass does not introduce it.
- Release `asset_discovery` selection: five passed, two manual benchmarks ignored.
- `cargo check --locked`: passed for the application.
- `cargo clippy -p deadsync-assets --lib --locked -- -D clippy::perf`: passed;
  existing non-performance warnings remain.
- All ten benchmark test executions passed across the five processes.
- Scoped formatting, whitespace checks, frozen-baseline comparisons, and
  unchanged source/executable SHA-256 checks before and after measurement passed.
- Workspace version increased exactly once: `0.5.1226` → `0.5.1227` in
  `Cargo.toml` and all three inherited package entries in `Cargo.lock`.

## Reproduce

```powershell
cargo test -p deadsync-assets --locked --no-fail-fast
cargo test -p deadsync-assets --lib --release --locked asset_discovery -- --nocapture
cargo test -p deadsync-assets --lib --release --locked benchmark_asset_discovery -- --ignored --nocapture --test-threads=1
```

For the recorded method, build first and run the emitted `deadsync_assets-*.exe`
directly five times with the benchmark filter and one-CPU process affinity.
Set `DEADSYNC_PERF_REVERSE=1` for runs 2 and 4, and remove it for runs 1, 3, and 5.
