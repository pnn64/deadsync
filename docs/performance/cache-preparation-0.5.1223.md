# Score cache preparation performance pass: 0.5.1223

Baseline: `b0a4c0c0d` / 0.5.1222. This pass applies the local
`rust-performance.md` guidance on measuring CPU and allocation costs
(M-HOTPATH), removing wasted work (M-THROUGHPUT), resource reuse
(M-MEM-REUSE), and known collection capacity (M-INITIAL-CAPACITY).
The guide is excluded from this commit.

## Three changes

1. **Score index rebuilding.** Local and GrooveStats index scans filter names
   before testing file types, use the type returned by directory enumeration,
   and retain follow-target metadata checks for symlinks. GrooveStats also
   reuses one read buffer across all shards, reserving 128 bytes after the first
   successful file open and growing for larger records. It previously allocated
   a fresh file buffer for every score. Local index cache loading/writing,
   traversal depth, GS filename rules, wire decoding and winner selection remain
   the same. These scans run when score indexes need rebuilding; local machine
   best-score preparation also uses the local loader.
2. **Leaderboard cache invalidation.** Update existing invalidation timestamps
   in place and extract matching owned keys from ready, in-flight and pending
   tables into the invalidation table. The old path cloned every match across
   four tables into a temporary HashSet before removing entries. The new path
   avoids those key copies and the set, including when entries overlap between
   tables. API-key matching remains case-sensitive, chart matching remains
   ASCII-case-insensitive, and stale fetches remain suppressed.
3. **GrooveStats validation.** Compare the borrowed chart-type bytes directly
   instead of allocating a lowercase String, and reserve precisely the number
   of failed checks for reason output. Valid plays allocate nothing. Invalid
   plays still return owned reason strings in the original policy order.
   Unicode whitespace trimming, ASCII-only matching, rate normalization and
   modifier rules are unchanged. This path feeds profile/gameplay submission
   eligibility and invalid-reason presentation.

## Measurement method

Five independent invocations of the release unit-test executable were pinned
onto logical CPU 4. Runs 1/3/5 measure old first; runs 2/4 measure new first.
Each case gets three warmups and seven timing batches: eight operations per
batch for filesystem cases, 100 for cache invalidation, and 20,000 for
validation. No build or other test ran concurrently with the benchmarks.
All eight frozen baseline functions were compared with the old commit after
whitespace normalization; the invalidation adapter only replaces `self` with
an explicit state argument. Source hashes stayed unchanged through the final
measurements. The initial diagnostic invocation is excluded.

Windows x86-64, Intel Xeon E5-2696 v4 @ 2.20 GHz; rustc 1.98.0
(88d9e12ae, 2026-08-18), LLVM 22.1.8, x86_64-pc-windows-msvc. Release uses
optimization level 3 and full LTO. CPU cycles use Windows
`QueryThreadCycleTime`, not elapsed TSC ticks. These measurements cover
synchronous operations on a warm local filesystem; filesystem/security
software participates. They do not measure cold startup or network latency.

Functions and inputs pass through `black_box`. Files and input data are made
outside measurement. Owning scan/validation operations include result
destruction. Mutating invalidation uses a fresh cloned fixture per operation,
with both cloning and final fixture destruction excluded from timing and
allocation accounting. Frees performed by invalidation itself are counted.
The new shared `measure_sampled_with_setup` helper measures those mutations;
its per-operation clock overhead is included equally in both variants and is
particularly visible in the single-key control. Existing batched timing
helpers are unchanged. The counted allocator's TLS checks remain present
while timing, although counters are disabled.

Filesystem `128` cases contain 128 scores across 32 chart hashes; local uses
one directory and GS one shard. `mixed` adds 384 unrelated files; GS distributes
scores across four shards. Local benchmarks exercise the directory collector;
GS benchmarks exercise the complete shard scan. Persisted local index behavior
is tested separately. Throughput counts score records (one scan for empty
controls), unique fixture keys for invalidation, and calls for validation.

Invalidation `all` has 128 matching keys in all four tables; `one` has one.
`sparse` has eight matches among 1,024 keys, and `miss` has none among 1,024.
`moving` has 128 matches in the first three tables with a reserved but empty
invalidation table; `growing` starts that table with zero capacity.
`tombstones` contains only 128 existing invalidation timestamps. Ready fixtures
include both shared successful data and cached errors. No-match and tombstone
updates allocate nothing; removing cached entries can still free owned keys.

## Results

Values are medians of five per-invocation medians. The [raw CSV](cache-preparation-0.5.1223.csv)
contains all 200 rows and each seven-batch time range.

| Workload | ns old -> new | Thread cycles old -> new | Fewer cycles | Units/s old -> new |
|---|---:|---:|---:|---:|
| `local/empty` | 88,812.5 -> 84,475.0 | 195,848.9 -> 184,133.1 | 6.0% | 11,259.7 -> 11,837.8 |
| `local/one` | 263,387.5 -> 194,250.0 | 577,339.9 -> 426,680.6 | 26.1% | 3,796.7 -> 5,148.0 |
| `local/128` | 18,043,262.5 -> 11,756,112.5 | 39,173,067.4 -> 25,594,441.1 | 34.7% | 7,094.1 -> 10,888.0 |
| `local/mixed` | 36,731,875.0 -> 12,386,650.0 | 80,097,964.6 -> 27,045,088.8 | 66.2% | 3,484.7 -> 10,333.7 |
| `gs/empty` | 265,050.0 -> 226,037.5 | 567,846.5 -> 488,332.8 | 14.0% | 3,772.9 -> 4,424.0 |
| `gs/one` | 443,125.0 -> 356,787.5 | 962,343.1 -> 782,407.8 | 18.7% | 2,256.7 -> 2,802.8 |
| `gs/128` | 19,121,800.0 -> 13,302,575.0 | 41,472,302.2 -> 28,803,065.0 | 30.5% | 6,693.9 -> 9,622.2 |
| `gs/mixed` | 39,396,500.0 -> 14,573,762.5 | 85,434,693.8 -> 31,744,721.4 | 62.8% | 3,249.0 -> 8,782.9 |
| `invalidate/one` | 1,894.0 -> 589.0 | 5,562.1 -> 2,623.0 | 52.8% | 527,983.1 -> 1,697,792.9 |
| `invalidate/all` | 275,281.0 -> 76,509.0 | 600,857.2 -> 168,949.2 | 71.9% | 464,979.4 -> 1,673,005.8 |
| `invalidate/sparse` | 94,196.0 -> 74,232.0 | 209,014.5 -> 164,811.6 | 21.1% | 10,870,949.9 -> 13,794,589.9 |
| `invalidate/miss` | 73,946.0 -> 68,172.0 | 164,258.4 -> 150,719.7 | 8.2% | 13,847,943.1 -> 15,020,829.7 |
| `invalidate/moving` | 197,083.0 -> 61,597.0 | 432,379.8 -> 137,110.7 | 68.3% | 649,472.6 -> 2,078,023.3 |
| `invalidate/growing` | 217,664.0 -> 77,841.0 | 477,274.2 -> 172,368.9 | 63.9% | 588,062.3 -> 1,644,377.6 |
| `invalidate/tombstones` | 81,010.0 -> 1,666.0 | 177,140.9 -> 4,923.4 | 97.2% | 1,580,051.8 -> 76,830,732.3 |
| `eligibility/valid` | 157.0 -> 35.2 | 343.8 -> 76.7 | 77.7% | 6,367,804.4 -> 28,380,871.3 |
| `eligibility/pump` | 152.7 -> 40.4 | 335.1 -> 88.7 | 73.5% | 6,547,073.5 -> 24,740,227.6 |
| `eligibility/solo` | 209.9 -> 160.2 | 456.7 -> 347.4 | 23.9% | 4,764,400.4 -> 6,243,561.3 |
| `eligibility/invalid` | 487.4 -> 454.8 | 1,062.2 -> 991.3 | 6.7% | 2,051,639.8 -> 2,198,889.6 |
| `eligibility/unicode` | 151.5 -> 38.7 | 332.2 -> 84.9 | 74.4% | 6,601,531.6 -> 25,856,496.4 |

The 128-file local and GS scans use 34.7% and 30.5% fewer cycles, respectively;
with unrelated files the savings rise to 66.2% and 62.8%. Overlapping cache
invalidation uses 71.9% fewer cycles, and the unreserved-table control still
uses 63.9% fewer. Valid dance-chart checks use 77.7% fewer cycles. Every final
workload median improved, but the empty local scan performs unchanged work;
its small apparent gain is timing noise, not an optimization claim. Small
changes such as no-match invalidation and multiple-invalid-reason validation
also have overlapping sample ranges; the stronger allocation reductions do
not depend on those timing differences.

## Allocation traffic

All five invocations reported identical allocation counters for each variant.
These count allocator requests and frees, including reallocation traffic;
they do not measure peak RSS or retained process memory. Invalidation retains
its output table after measurement, so its requested and freed byte totals
need not match.

| Workload | Allocations old -> new | Reallocations old -> new | Frees old -> new | Requested bytes old -> new | Freed bytes old -> new |
|---|---:|---:|---:|---:|---:|
| `local/empty` | 7 -> 7 | 4 -> 4 | 7 -> 7 | 2,090 -> 2,090 | 2,090 -> 2,090 |
| `local/one` | 19 -> 18 | 9 -> 9 | 19 -> 18 | 3,991 -> 3,833 | 3,991 -> 3,833 |
| `local/128` | 1,004 -> 876 | 644 -> 644 | 1,004 -> 876 | 193,892 -> 173,376 | 193,892 -> 173,376 |
| `local/mixed` | 2,924 -> 2,412 | 2,564 -> 2,564 | 2,924 -> 2,412 | 625,562 -> 552,274 | 625,562 -> 552,274 |
| `gs/empty` | 18 -> 17 | 12 -> 12 | 18 -> 17 | 3,223 -> 3,105 | 3,223 -> 3,105 |
| `gs/one` | 27 -> 25 | 17 -> 17 | 27 -> 25 | 4,774 -> 4,593 | 4,774 -> 4,593 |
| `gs/128` | 951 -> 695 | 652 -> 652 | 951 -> 695 | 183,385 -> 158,339 | 183,385 -> 158,339 |
| `gs/mixed` | 2,904 -> 2,261 | 2,596 -> 2,596 | 2,904 -> 2,261 | 624,646 -> 544,170 | 624,646 -> 544,170 |
| `invalidate/one` | 13 -> 0 | 0 -> 0 | 22 -> 9 | 464 -> 0 | 557 -> 93 |
| `invalidate/all` | 1,543 -> 0 | 0 -> 0 | 2,695 -> 1,152 | 57,716 -> 0 | 70,058 -> 12,342 |
| `invalidate/sparse` | 99 -> 0 | 0 -> 0 | 171 -> 72 | 3,308 -> 0 | 4,052 -> 744 |
| `invalidate/miss` | 0 -> 0 | 0 -> 0 | 0 -> 0 | 0 -> 0 | 0 -> 0 |
| `invalidate/moving` | 1,159 -> 0 | 0 -> 0 | 1,927 -> 768 | 53,602 -> 0 | 61,830 -> 8,228 |
| `invalidate/growing` | 1,166 -> 7 | 0 -> 0 | 1,933 -> 774 | 102,990 -> 49,388 | 86,370 -> 32,768 |
| `invalidate/tombstones` | 391 -> 0 | 0 -> 0 | 391 -> 0 | 45,374 -> 0 | 45,374 -> 0 |
| `eligibility/valid` | 2 -> 0 | 0 -> 0 | 2 -> 0 | 156 -> 0 | 156 -> 0 |
| `eligibility/pump` | 2 -> 0 | 0 -> 0 | 2 -> 0 | 155 -> 0 | 155 -> 0 |
| `eligibility/solo` | 3 -> 2 | 0 -> 0 | 3 -> 2 | 201 -> 71 | 201 -> 71 |
| `eligibility/invalid` | 8 -> 7 | 0 -> 0 | 8 -> 7 | 395 -> 388 | 395 -> 388 |
| `eligibility/unicode` | 2 -> 0 | 0 -> 0 | 2 -> 0 | 153 -> 0 | 153 -> 0 |

The GS scan removes 256 allocations in the 128-file case; local scans remove
128. Filesystem path conversions and owned result maps still allocate.
Invalidation needs no new allocation when table capacity is available, but
still grows the destination table when needed: the `growing` control drops
from 1,166 allocation calls to seven. Valid validation removes both temporary
allocations (156 requested bytes for `dance-single`). Owned invalid-reason
strings remain necessary; the exhaustive reason test also verifies a full
12-reason result stays within 13 allocations with no reallocations.

## Behavior and validation

- `cargo test -p deadsync-score -p deadsync-profile -p deadsync-profile-gameplay --locked`:
  509 tests reported passed, ten manual benchmark tests ignored.
- `cargo test -p deadsync-score --lib cache_preparation --release --locked -- --nocapture`:
  nine new regression tests reported passed, one manual benchmark ignored.
- New differential coverage includes all 4,096 reason masks; mixed-case,
  Unicode and generated chart types; special/invalid rates and modifier masks;
  all table-membership combinations; case-sensitive API keys; repeated and
  backward invalidation timestamps; stale successful/failed fetches and a
  later valid completion; GS V1/V2 records; corrupt/empty/large records;
  ignored filenames and directories; traversal depth; persisted local indexes;
  and Windows locked-file read failures.
- Allocation assertions cover valid validation, tombstone reuse, complete GS
  scans and the full reason list. Local index comparisons include ITG, EX,
  HardEX and pass-rate maps.
- Windows denied symlink creation with error 1314. The new file/directory/link
  regression and existing symlink fixtures return early; those cases were not
  exercised on this host. The production follow-target fallback is retained.
- `cargo check --locked`: passed, including the root application.
- `cargo clippy -p deadsync-score -p deadsync-profile-gameplay --lib --locked -- -D clippy::perf`:
  passed; existing non-performance warnings remain.
- Scoped `rustfmt --check` and `git diff --check`: passed.

To rerun the release comparison:

```powershell
cargo test -p deadsync-score --lib --release --locked cache_preparation::benchmark_cache_preparation -- --ignored --nocapture --test-threads=1
```

Set `$env:DEADSYNC_PERF_REVERSE = "1"` to measure new first; leave it unset to
measure old first. The recorded five-run comparison additionally pinned the
benchmark process to logical CPU 4. Run it alone for comparable timing.
