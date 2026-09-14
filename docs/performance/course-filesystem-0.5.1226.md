# Course filesystem preparation - 0.5.1226

Baseline: `0342a1a4e` (0.5.1225). This pass applies the supplied
`rust-performance.md` guidance on measured hot paths (M-HOTPATH), avoiding
short-lived string allocations, and reusing available data (M-MEM-REUSE).
The guide is excluded from the commit.

## Changes

1. **Reuse directory-entry metadata during course discovery.** Ordinary entries
   use `DirEntry::file_type` instead of a separate `Path::is_dir` query. Links
   and file-type errors retain the path-based lookup. Recursive traversal,
   case-insensitive `.crs` selection, and stable cached-key sorting stay the same.
2. **Filter directory names before path construction and metadata lookup.** Compare
   borrowed, lossy filename text directly using ASCII case-insensitive equality.
   Build a path and inspect its type only for an exact match or a possible
   first case-insensitive match. Continue looking for an exact match after finding
   a case-insensitive directory. This removes lowercase copies and work for
   irrelevant entries from group and song resolution.
3. **Stream unqualified song resolution.** Search direct packs as they are
   classified, returning immediately when the first matching pack is found.
   Retain only series-folder paths for the second pass. All direct packs still
   precede nested packs, and later roots retain priority. Nested directory checks
   also reuse entry metadata. Pack classification itself is unchanged.

## Results

| Optimization and representative workload | Fewer thread cycles | Throughput gain | Allocated-byte churn reduction |
|---|---:|---:|---:|
| Course discovery: 128 courses + 512 unrelated files | 95.5% | 22.0x | 13.9% |
| Directory lookup: missing name among 128 directories | 96.7% | 30.9x | 98.3% |
| Song resolution: matching first pack among 32 | 95.7% | 23.3x | 95.0% |

These gains come from fewer metadata queries, filtering before allocating paths,
and avoiding work after an early match. Filesystem enumeration and returned paths
still allocate. The whitespace-only fast path retains zero allocations, reallocations,
frees, or byte churn.

Every measured workload that inspected a nonempty directory used fewer median
thread cycles. Looking through
all 32 packs still reduced cycles by 13.7% for a last-pack hit and 12.9% for a miss;
the warm named-pack lookup reduced them by 96.6%. Empty directory lookup measured
3.2% more cycles (2.8 microseconds more wall time), and empty-root resolution measured
1.1% more cycles (4.6 microseconds more wall time). Their sample ranges overlap and
their per-process changes vary in direction; neither is claimed as a speedup.
Empty scanning was effectively unchanged. The blank-query cycle result is equal
and includes substantial batch-clock overhead relative to this tiny operation.

### Timings and throughput

| Workload | ns old -> new | Thread cycles old -> new | Fewer cycles | Units/s old -> new |
|---|---:|---:|---:|---:|
| `scan/empty` | 84,000.0 -> 83,750.0 | 184,009.6 -> 183,968.4 | 0.0% | 11,904.8 -> 11,940.3 |
| `scan/one` | 144,975.0 -> 92,062.5 | 317,863.4 -> 201,555.9 | 36.6% | 6,897.7 -> 10,862.2 |
| `scan/courses128` | 5,957,943.8 -> 358,312.5 | 12,965,796.4 -> 773,806.1 | 94.0% | 21,483.9 -> 357,230.1 |
| `scan/mixed640` | 28,992,393.8 -> 1,315,062.5 | 63,037,409.3 -> 2,867,328.4 | 95.5% | 22,074.8 -> 486,668.9 |
| `scan/nested` | 12,669,725.0 -> 1,269,668.8 | 27,565,976.1 -> 2,766,193.9 | 90.0% | 20,837.1 -> 207,928.2 |
| `lookup/empty` | 79,268.8 -> 82,103.1 | 173,631.3 -> 179,201.2 | -3.2% | 12,615.3 -> 12,179.8 |
| `lookup/one` | 115,975.0 -> 76,040.6 | 251,999.7 -> 166,285.0 | 34.0% | 8,622.5 -> 13,150.9 |
| `lookup/exact128` | 5,525,390.6 -> 173,053.1 | 12,028,167.9 -> 375,557.6 | 96.9% | 181.0 -> 5,778.6 |
| `lookup/case128` | 5,559,134.4 -> 184,093.8 | 12,101,131.2 -> 400,285.7 | 96.7% | 179.9 -> 5,432.0 |
| `lookup/missing128` | 5,581,634.4 -> 180,756.2 | 12,159,936.3 -> 395,840.8 | 96.7% | 179.2 -> 5,532.3 |
| `lookup/files512` | 22,247,784.4 -> 466,084.4 | 48,655,762.2 -> 1,020,126.3 | 97.9% | 44.9 -> 2,145.5 |
| `lookup/blank` | 21.9 -> 18.8 | 89.2 -> 89.2 | 0.0% | 45,714,285.7 -> 53,333,333.3 |
| `resolve/packs0-missing` | 84,212.5 -> 88,775.0 | 183,996.0 -> 186,026.2 | -1.1% | 11,874.7 -> 11,264.4 |
| `resolve/packs1-first` | 813,462.5 -> 717,575.0 | 1,777,675.6 -> 1,563,800.4 | 12.0% | 1,229.3 -> 1,393.6 |
| `resolve/packs32-first` | 17,785,062.5 -> 764,325.0 | 38,712,446.5 -> 1,672,809.6 | 95.7% | 56.2 -> 1,308.3 |
| `resolve/packs32-last` | 21,486,275.0 -> 18,535,612.5 | 46,815,125.0 -> 40,420,129.9 | 13.7% | 46.5 -> 54.0 |
| `resolve/packs32-missing` | 21,121,600.0 -> 18,487,962.5 | 46,158,161.9 -> 40,224,006.4 | 12.9% | 47.3 -> 54.1 |
| `resolve/nested8` | 6,380,850.0 -> 5,213,062.5 | 13,883,566.9 -> 11,323,072.1 | 18.4% | 156.7 -> 191.8 |
| `resolve/qualified128` | 17,828,462.5 -> 12,238,775.0 | 38,807,873.9 -> 26,618,819.9 | 31.4% | 56.1 -> 81.7 |
| `resolve/qualified128-warm` | 5,685,087.5 -> 195,231.2 | 12,436,540.8 -> 423,717.3 | 96.6% | 175.9 -> 5,122.1 |

### Allocation churn

Counters are per completed operation and identical across all five processes.
Frees equal allocation counts, and freed bytes equal allocated bytes in every row.
Reallocations are counted separately; byte churn includes replacement allocations.

| Workload | Allocations/frees old -> new | Reallocations old -> new | Allocated/freed bytes old -> new |
|---|---:|---:|---:|
| `scan/empty` | 8 -> 8 | 5 -> 5 | 1,366 -> 1,366 |
| `scan/one` | 14 -> 13 | 10 -> 10 | 2,682 -> 2,512 |
| `scan/courses128` | 778 -> 650 | 650 -> 650 | 176,342 -> 154,582 |
| `scan/mixed640` | 3,338 -> 2,698 | 3,210 -> 3,210 | 784,598 -> 675,798 |
| `scan/nested` | 1,506 -> 1,242 | 1,372 -> 1,372 | 359,478 -> 308,502 |
| `lookup/empty` | 7 -> 6 | 5 -> 5 | 1,272 -> 1,265 |
| `lookup/one` | 13 -> 11 | 10 -> 10 | 2,466 -> 2,291 |
| `lookup/exact128` | 902 -> 138 | 645 -> 10 | 154,993 -> 3,688 |
| `lookup/case128` | 903 -> 138 | 645 -> 10 | 155,004 -> 3,688 |
| `lookup/missing128` | 903 -> 134 | 645 -> 5 | 155,000 -> 2,673 |
| `lookup/files512` | 2,567 -> 518 | 2,565 -> 5 | 601,848 -> 5,873 |
| `lookup/blank` | 0 -> 0 | 0 -> 0 | 0 -> 0 |
| `resolve/packs0-missing` | 6 -> 6 | 5 -> 5 | 1,265 -> 1,265 |
| `resolve/packs1-first` | 92 -> 88 | 74 -> 74 | 22,450 -> 21,950 |
| `resolve/packs32-first` | 2,789 -> 119 | 1,379 -> 74 | 444,151 -> 22,229 |
| `resolve/packs32-last` | 3,223 -> 2,971 | 1,689 -> 1,531 | 522,023 -> 477,061 |
| `resolve/packs32-missing` | 3,207 -> 2,950 | 1,672 -> 1,509 | 516,753 -> 470,769 |
| `resolve/nested8` | 647 -> 580 | 470 -> 428 | 144,262 -> 126,435 |
| `resolve/qualified128` | 3,129 -> 2,361 | 2,856 -> 2,221 | 858,631 -> 706,266 |
| `resolve/qualified128-warm` | 905 -> 140 | 645 -> 10 | 155,740 -> 3,533 |

[Raw results: 200 rows, 20 workloads, five processes, both variants](course-filesystem-0.5.1226.csv).

## Method

Windows x86_64 MSVC; Intel Xeon E5-2696 v4 @ 2.20 GHz; rustc 1.98.0
(88d9e12ae, 2026-08-18), LLVM 22.1.8. Release optimization level 3 and full LTO.
Four test-only frozen function bodies match baseline Git sources, ignoring
formatting/visibility. Shared pack classification and types remain unchanged.

Five fresh benchmark processes run the same old/new binary, pinned to logical
CPU 4. Order alternates old-first/new-first/old-first/new-first/old-first.
Each comparison has three warmups and seven timing samples, then a separate
allocation-counted operation. Tables give the median of five per-process medians.
Final source hashes were checked before and after measurement. No benchmark
runs alongside Cargo builds or tests.

Course scans use 16 iterations per sample, directory-name lookups 32, complete
resolvers eight, and warm group-cache resolvers 16. Function pointers and inputs
are opaque to the optimizer. Fixture creation and cleanup are outside measurement;
returned paths/collections and temporary cold caches are destroyed inside it.
Warm caches are prepared outside measurement and reused. Scan throughput counts
input files/directories, or operations for the empty fixture; other throughput
counts completed lookups. These are synthetic, warmed filesystem-cache cases,
not measurements of cold disks or complete application startup.

Fixtures cover empty/one-entry directories, 128 course files, 128 courses mixed
with 512 unrelated files, eight nested folders, 128 directory-name candidates,
512 irrelevant files, and a whitespace-only query. Resolver fixtures cover one
or 32 direct packs with first/last/missing matches, eight series folders, and
128 songs in a named pack with both cold and warm group caches. First/last
fixtures use actual enumeration order rather than assuming alphabetical order.

CPU counts use Windows `QueryThreadCycleTime` around each timing batch for the
calling thread. A TLS-scoped counting allocator delegates to System, recording
allocations, reallocations, frees, and requested/freed byte churn. Timings retain
the allocator's disabled-accounting TLS checks. Bytes describe allocator churn;
peak RSS, committed heap pages, cache misses, and physical disk traffic were not
measured. The raw CSV retains every timing range and all counters.

## Behavior coverage

Six new tests compare ordered course paths, directory results, root/pack/series
precedence, and cold/warm cache contents against frozen implementations. Cases
include missing roots, file-as-root errors, directory names ending in `.crs`,
case variants, non-ASCII case distinctions, Unicode whitespace, and lossy
filenames (an unpaired UTF-16 surrogate on Windows; invalid bytes on Unix).
Allocation assertions verify reduced churn for missing-name lookups and early
unqualified resolution, plus no heap operations for whitespace-only queries.

The Windows junction test passed, including a valid pack junction and a broken
junction whose `.crs` path must remain in discovery results. The separate
symlink test returned early with Windows privilege error 1314; symbolic-file-link
and symbolic-directory-link cases were not exercised on this machine. Exact-case
preference is tested on the available filesystem; the same test creates distinct
case variants where the filesystem supports them. Directory fixtures are static;
filesystem enumeration provides no transactional snapshot during concurrent edits.

## Validation

- `cargo test -p deadsync-simfile --locked`: 231 passed, zero failed,
  nine ignored manual benchmarks across unit/integration tests.
- Release regression selection: six reported passed, zero failed, one ignored
  benchmark. Five regression tests were fully exercised; the symlink test
  returned early for the privilege restriction described above.
- `cargo check --locked`: passed for the application.
- `cargo clippy -p deadsync-simfile --lib --locked -- -D clippy::perf`: passed;
  non-performance warnings in existing code remain.
- Scoped Rust formatting, whitespace checks, frozen-baseline comparisons, and
  final source-hash verification passed.
- Workspace version increased exactly once: `0.5.1225` -> `0.5.1226` in
  `Cargo.toml` and all three corresponding workspace entries in `Cargo.lock`.

## Reproduce

```powershell
cargo test -p deadsync-simfile --locked
cargo test -p deadsync-simfile --lib --release --locked course_filesystem -- --nocapture
cargo test -p deadsync-simfile --lib --release --locked course::course_filesystem::benchmark_course_filesystem -- --ignored --exact --nocapture --test-threads=1
```

For the recorded method, build first and run the emitted `deadsync_simfile-*.exe`
directly on one logical CPU. Repeat five times, setting `DEADSYNC_PERF_REVERSE=1`
for runs 2 and 4 and removing it for the others. The Windows junction regression
uses the system PowerShell to create fixture links without changing OS settings.
