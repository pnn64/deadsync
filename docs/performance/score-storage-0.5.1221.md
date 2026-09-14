# Score storage performance pass: 0.5.1221

Baseline: `67a81510d` / 0.5.1220. This pass applies the local
`rust-performance.md` guidance on measuring hot paths (M-HOTPATH), avoiding
wasted work (M-THROUGHPUT), initial capacity (M-INITIAL-CAPACITY), and reusing
allocations (M-MEM-REUSE). The guide is intentionally excluded from the commit.

## Changes

1. **Chart-specific local score searches.** Filter filenames before inspecting
   file types, use the type from directory enumeration for regular files, and
   retain the follow-target metadata check for symlinks. This removes metadata
   queries from leaderboard and replay searches while retaining one full path
   per directory entry. Header decoding, full replay decoding, ordering,
   filename timestamps, failed scores, and candidate truncation are unchanged.
2. **Borrow GS cache usernames during decoding.** Add internal borrowed V2/V1
   wire views. Index building converts the borrowed fields directly to a cached
   score, so it no longer allocates a username that it immediately discards.
   The public owning decoder still returns an owned `String`. The V2-first
   fallback order, wire format and trailing-byte acceptance are unchanged.
3. **Stream GS duplicate detection.** Compare each borrowed record as it is
   read and return at the first duplicate. Reuse a local byte buffer, reserve
   128 bytes only after opening a matching file, and keep larger capacity for
   the remainder of that scan. The check also uses directory-entry file types
   and tests the filename prefix without allocating a formatted prefix. It
   avoids retaining an owned history vector and all its usernames. ASCII case
   comparison, the `1e-9` score tolerance, lamp/grade checks, write paths,
   overwrites and error categories are preserved.

## Measurement

Final measurements use five independent invocations of the release unit-test
executable, pinned to logical CPU 4. Runs 1/3/5 measure old first; runs 2/4
measure new first. Each row has three warmups and seven timing batches (eight
operations per batch for filesystem workloads; 20,000 for decoder workloads).
Allocation counters come from a separate complete operation including result
destruction, outside timed batches. Function pointers and inputs pass through
`black_box`. Local benchmarks exercise the directory collectors with the same
fresh header and output buffers. The nine frozen baseline functions match the
old commit after ignoring whitespace; wrapper adapters select the same inputs and output types.

Host: Windows x86-64, Intel Xeon E5-2696 v4 @ 2.20 GHz, rustc 1.98.0
(88d9e12ae, 2026-08-18), LLVM 22.1.8, x86_64-pc-windows-msvc. Release uses
optimization level 3 and full LTO, without extra target CPU features.
The machine's filesystem/security software participates in these timings.
These are warm-cache synchronous storage benchmarks, not a cold-disk or
end-to-end application loading benchmark.

Times/cycles below are medians of the five per-invocation medians. CPU cycles
are measured with Windows `QueryThreadCycleTime`. Throughput counts directory
entries for local searches, records for index building, and complete operations
for decoding/duplicate checks. Early duplicate termination still counts as one
operation. The CSV retains all 170 rows, including each seven-batch range.

| Workload | ns old -> new | Thread cycles old -> new | Fewer cycles | Units/s old -> new |
|---|---:|---:|---:|---:|
| `local/selective` | 29,276,287.5 -> 2,285,325.0 | 64,009,986.6 -> 4,974,967.4 | 92.2% | 17,488.6 -> 224,038.2 |
| `local/replay` | 28,025,100.0 -> 2,344,362.5 | 60,845,290.4 -> 5,092,235.4 | 91.6% | 18,269.3 -> 218,396.3 |
| `local/missing` | 26,399,000.0 -> 1,434,700.0 | 57,368,739.6 -> 3,109,848.5 | 94.6% | 19,394.7 -> 356,869.0 |
| `local/all` | 17,719,500.0 -> 11,620,600.0 | 38,480,546.6 -> 25,232,155.8 | 34.4% | 7,223.7 -> 11,014.9 |
| `decode/v2` | 110.9 -> 36.9 | 243.3 -> 81.0 | 66.7% | 9,017,945.7 -> 27,096,599.4 |
| `decode/v1` | 110.0 -> 36.2 | 240.9 -> 79.3 | 67.1% | 9,089,669.6 -> 27,639,579.9 |
| `decode/long` | 339.8 -> 154.2 | 740.7 -> 336.9 | 54.5% | 2,942,691.1 -> 6,484,243.3 |
| `decode/invalid` | 10.1 -> 9.5 | 21.9 -> 20.9 | 4.6% | 99,304,865.9 -> 105,429,625.7 |
| `owned/v2` | 106.1 -> 93.5 | 230.2 -> 205.0 | 10.9% | 9,428,625.3 -> 10,696,903.2 |
| `owned/v1` | 105.0 -> 97.4 | 228.5 -> 213.7 | 6.5% | 9,526,531.4 -> 10,264,305.9 |
| `index/128` | 19,616,162.5 -> 19,443,362.5 | 42,558,579.9 -> 42,194,759.8 | 0.9% | 6,525.2 -> 6,583.2 |
| `duplicate/early` | 19,864,312.5 -> 209,500.0 | 43,314,263.5 -> 459,907.4 | 98.9% | 50.3 -> 4,773.3 |
| `duplicate/late` | 20,581,712.5 -> 13,987,775.0 | 44,653,680.4 -> 30,621,704.1 | 31.4% | 48.6 -> 71.5 |
| `duplicate/miss` | 20,388,237.5 -> 13,825,887.5 | 44,630,687.5 -> 30,264,961.9 | 32.2% | 49.0 -> 72.3 |
| `duplicate/one-hit` | 325,625.0 -> 185,425.0 | 714,033.4 -> 406,267.1 | 43.1% | 3,071.0 -> 5,393.0 |
| `duplicate/one-miss` | 315,675.0 -> 207,662.5 | 688,324.5 -> 453,102.9 | 34.2% | 3,167.8 -> 4,815.5 |
| `duplicate/empty` | 133,787.5 -> 86,100.0 | 290,947.1 -> 189,209.0 | 35.0% | 7,474.5 -> 11,614.4 |

The selective local fixtures contain 512 files, eight for the requested chart;
the no-match control has 512 unrelated files and the all-match control has
128 matches. Selective leaderboard/replay scans use **92.2%/91.6% fewer cycles**,
with about **12.8x/12.0x throughput**. The all-match control still saves **34.4%**
of cycles. Windows metadata path handling also accounts for one removed
allocation per enumerated file in these fixtures.

Borrowed cache-score decoding uses **66.7%/67.1% fewer cycles** for current/legacy
records with a five-byte username, and **54.5% fewer cycles** with a 4,096-byte
username. All three now allocate and free **zero bytes**. UTF-8 validation still
scales with username length. The public owning-decoder controls retain their
one username allocation; they measure 6.5-10.9% fewer cycles.

Streaming duplicate detection uses **98.9% fewer cycles** when the first of 128
records matches. Actual directory enumeration determines early/late placement;
fixtures contain a mix of current and legacy records. When the last record
matches, cycles fall **31.4%**; with no match they fall **32.2%** and throughput
rises from **49.0 to 72.3 checks/s**. One-record and empty-directory controls
also improve in these measurements. The duplicate benchmark isolates the
lookup predicate with the incoming entry prepared outside both variants;
real writer results and output bytes are tested separately.

| Operation | Allocations/frees old -> new | Reallocations old -> new | Requested/freed bytes old -> new |
|---|---:|---:|---:|
| Selective local leaderboard scan | 2,576 -> 2,064 | 2,565 -> 2,565 | 615,882 -> 521,594 |
| Selective local replay scan | 2,576 -> 2,064 | 2,565 -> 2,565 | 615,978 -> 521,690 |
| Local scan, no match | 2,567 -> 2,055 | 2,564 -> 2,564 | 610,362 -> 518,202 |
| Local scan, all match | 776 -> 648 | 647 -> 647 | 181,674 -> 158,378 |
| Borrowed current record | 1 -> 0 | 0 -> 0 | 5 -> 0 |
| Borrowed legacy record | 1 -> 0 | 0 -> 0 | 5 -> 0 |
| Borrowed record, long username | 1 -> 0 | 0 -> 0 | 4,096 -> 0 |
| Owning current record (control) | 1 -> 1 | 0 -> 0 | 5 -> 5 |
| GS index, 128 records | 1,044 -> 916 | 655 -> 655 | 186,627 -> 185,219 |
| Duplicate, first record | 1,033 -> 12 | 650 -> 9 | 190,284 -> 2,372 |
| Duplicate, last record | 1,033 -> 647 | 650 -> 644 | 189,636 -> 150,574 |
| Duplicate, no match | 1,033 -> 647 | 650 -> 644 | 189,648 -> 150,574 |
| Duplicate, empty directory | 8 -> 6 | 5 -> 4 | 1,244 -> 1,078 |

All allocation counters are identical across the five invocations for each
workload/variant. Requested/freed bytes include reallocation traffic, not just
payload sizes. Full GS index building eliminates 128 username allocations in
the 128-record fixture, but total cycles differ by only **0.9%**, within the
observed timing variation. **No meaningful index-loading CPU speedup is claimed.**
The tiny invalid-record control differs by 0.6 ns; it remains allocation-free
in both versions and is not a substantive optimization result. Individual
sample ranges overlap for these controls; see the [raw samples](score-storage-0.5.1221.csv).

File enumeration, paths, reads and owned outputs still allocate. Streaming
changes retained duplicate-history storage from O(number of matching records
plus their usernames) to a scratch buffer sized for the largest record read,
plus the current path/entry. That buffer is dropped after the lookup; it is not
a global or long-lived cache. Process RSS and peak resident memory were not
measured. Both variants use the same counting allocator; timing disables its
counters, but its lightweight thread-local checks remain in allocation calls.


## Behavior and validation

Seven new regression tests cover:

- Both GS wire versions, 216 combinations of username/float/lamp values,
  truncations (all short-record prefixes and long-name boundaries), trailing
  bytes, byte mutations and 2,048 deterministic random buffers. Comparisons
  preserve scalar bit patterns, including signed zero and NaN. Borrowed
  usernames must point into the input slice; borrowed decoding must have zero
  heap churn for every tested buffer.
- Local directory scans with matching/unrelated charts, malformed filenames,
  timestamp overflow, corrupt records, directories named like score files,
  missing directories, file paths used as directories, candidate order and
  limits. Both public full-play readers and private candidate readers are
  compared to frozen implementations.
- Legacy/current GS index records, corrupt files, grade normalization,
  non-finite scores and best-score selection.
- Duplicate/no-duplicate checks, ASCII case and whitespace, epsilon boundaries,
  lamps, non-finite scores, long records followed by short/corrupt records,
  Unicode/empty/extended chart prefixes, actual writes, repeated imports,
  overwrite behavior, missing directories and create/write errors.
- Strictly reduced allocation churn for streaming a complete no-match history
  and for converting a decoded record to a cached score.

The old unbounded owning decoder attempted a 3.4 TB allocation during initial
corruption/legacy coverage: an empty-name V1 record can be interpreted as a V2
record with its timestamp used as a string length. The corruption-test adapter
therefore applies a 16 KiB bincode allocation limit to the old algorithm; all
valid fixtures fit that limit. The new decoder is tested with its production
configuration. Benchmark baselines and file behavior comparisons retain the
original unbounded decoder. Borrowing checks slice bounds before accessing the
name and avoids the erroneous allocation attempt.

The symlink fixture covers file links, broken links and directory links when
creation is available. **This Windows run could not create symlinks (error 1314,
missing privilege), so that fixture returned early.** The production code keeps
an explicit follow-target fallback; no claim is made that this host exercised it.
Filesystem race behavior and concurrent directory mutation are not covered.

The locked score/profile suite passes **477 tests** (eight manual benchmarks
ignored), with the symlink limitation above. All seven new regression tests
also pass in release mode under the same limitation. The locked application
check, scoped rustfmt check, diff whitespace check, and
`cargo clippy -p deadsync-score --lib --locked -- -D clippy::perf` pass.
Clippy emits existing non-performance warnings in unchanged score/rules code.
The only Cargo.toml/Cargo.lock changes are **0.5.1220 -> 0.5.1221**, one patch
increment in the workspace and all three inheriting lockfile entries.

Reproduce with:

```powershell
cargo test -p deadsync-score -p deadsync-profile --locked -- --test-threads=1
cargo test -p deadsync-score --release --lib local_store::score_storage --locked -- --nocapture --test-threads=1
cargo test -p deadsync-score --release --lib --locked -- --ignored --exact local_store::score_storage::benchmark_score_storage --nocapture --test-threads=1
$env:DEADSYNC_PERF_REVERSE = '1'
cargo test -p deadsync-score --release --lib --locked -- --ignored --exact local_store::score_storage::benchmark_score_storage --nocapture --test-threads=1
Remove-Item Env:DEADSYNC_PERF_REVERSE
cargo check --locked
cargo clippy -p deadsync-score --lib --locked -- -D clippy::perf
```

Run the built benchmark executable five times with the same affinity, alternating
order as described, with other build/test work idle. CPU, operating system,
filesystem, storage latency, allocator, history size and match position affect
the results.
