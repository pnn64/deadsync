# StepManiaOnline pack preparation — 0.5.1155

Baseline: `856b03b86` (0.5.1154). This pass applies the supplied
`rust-performance.md` guidance on measured hot paths (M-HOTPATH), allocation
reuse (M-MEM-REUSE), and initial capacity (M-INITIAL-CAPACITY).
The guide itself is excluded from the commit.

Valid path validation uses no allocations. Deep destination construction drops
from 69 allocations and 37 reallocations to one allocation without growth, using
73.09% fewer thread cycles. Warm unique updates with 64 install records drop from
68 allocations to zero; their shared-reader control remains allocation-heavy.
Long ASCII name sanitization uses 92.56% fewer cycles and 64.76% less byte churn.
Thirty-four of 35 cases use fewer median cycles; the exception is listed below.

## Changes

1. **Borrow validated archive paths.** Validation retains the original name and
   component count instead of allocating a vector and a string per component.
   Destination paths push borrowed components into one reserved PathBuf instead
   of repeatedly joining and copying growing paths. Duplicate-detection keys
   build directly into a reserved string; ASCII keys lowercase in place, while
   Unicode keys retain str's contextual casing rules.
2. **Reuse install snapshots when uniquely owned.** Find the install before
   requesting mutable access, so missing IDs do not clone anything. Arc::make_mut
   updates a unique snapshot in place and copies it when a reader retains it.
   Repeated progress updates reuse the existing identical progress message.
3. **Track sanitized-name length incrementally.** Count emitted Unicode scalars
   as they are written instead of recounting the entire prefix for every input
   character. Names needing an ID suffix truncate and append into the existing
   string instead of allocating separate suffix and base strings.

## Method

Windows x86_64 MSVC; Intel Xeon E5-2696 v4 @ 2.20 GHz; rustc 1.98.0
(88d9e12ae, 2026-08-18), LLVM 22.1.8. Cargo release uses optimization level 3
and full LTO. Old implementations are frozen in a test-only module. The old
progress closure is unchanged apart from accepting an isolated RuntimeState;
both progress benchmarks exclude the unchanged runtime mutex acquisition.

All 35 comparisons run in the same release binary with black-boxed inputs.
Sanitizer and validation calls use opaque function pointers on both sides.
Three invocations alternate old/new, new/old, old/new. Each invocation uses three
warmups and seven timing batches, then separately counts one operation. Tables
report the median of the three per-invocation medians. Batches use 1,000 operations
except archive inspection, which uses 16. No benchmark runs beside a Cargo build.

CPU counts use Windows QueryThreadCycleTime for the calling thread. The scoped
allocator delegates to System and counts allocations, reallocations, frees and
requested bytes separately from timing. Returned values are dropped inside each
operation. Byte totals describe churn, including replacement buffers, not peak
RSS, committed heap pages, cache misses or the production allocator.

Sanitizer inputs cover short clean names, reserved Windows names, invalid path
characters, 160/2,048 ASCII characters, 240 Unicode scalars and long mixed input.
Path fixtures cover a normal three-component path, 32 nested directories,
Unicode casing, repeated/mixed separators and rejected traversal. Key and
destination cases include validation and final output construction.

Progress fixtures contain 0, 1 or 64 install records with an existing progress
message. Each operation changes the byte count. Shared cases retain an Arc
reader during every update, including its acquisition and release in timing;
unique cases retain no reader. Missing cases request an absent ID; all zero-record
cases are misses. Runtime setup is outside timing, and replaced allocations are
freed inside the operation. The first progress-message allocation is outside
these warmed comparisons, but message transitions are covered by behavior tests.

Inspection fixtures are local ZIP files with 1 or 256 stored entries containing
small identical payloads; a 256-entry fixture uses Unicode names. They include
file opening, ZIP metadata parsing, entry checks and duplicate-path tracking.
Fixture creation and removal are outside timing. OS cache and filesystem work
limit how much the path-only improvement changes whole inspection time. These
measurements exclude network transfer and extraction file writes.

## Timing, CPU and throughput

Throughput below is complete operations per second. Raw output also reports
input bytes/s for name/path cases, updates/s for progress, and entries/s for
inspection. Positive CPU savings mean fewer cycles.

| Case | Old ns/op | New ns/op | Old cycles/op | New cycles/op | CPU saved | Old → new ops/s |
|---|---:|---:|---:|---:|---:|---:|
| sanitize_short | 136.6 | 124.9 | 301.2 | 275.5 | 8.53% | 7,320,644 → 8,006,405 |
| sanitize_reserved | 513.7 | 342.5 | 1,124.7 | 754.2 | 32.94% | 1,946,661 → 2,919,708 |
| sanitize_invalid | 534.0 | 293.4 | 1,173.2 | 645.1 | 45.01% | 1,872,659 → 3,408,316 |
| sanitize_ascii_limit | 2,448.6 | 484.8 | 5,369.2 | 1,065.5 | 80.16% | 408,397 → 2,062,706 |
| sanitize_ascii_long | 34,243.5 | 2,547.1 | 75,088.5 | 5,588.0 | 92.56% | 29,203 → 392,603 |
| sanitize_unicode | 7,462.8 | 1,449.8 | 16,345.3 | 3,180.6 | 80.54% | 133,998 → 689,750 |
| sanitize_mixed | 30,100.4 | 4,051.0 | 65,992.9 | 8,887.1 | 86.53% | 33,222 → 246,853 |
| validate_shallow | 299.5 | 68.6 | 658.7 | 151.7 | 76.97% | 3,338,898 → 14,577,259 |
| key_shallow | 422.5 | 181.4 | 928.7 | 399.3 | 57.00% | 2,366,864 → 5,512,679 |
| destination_shallow | 739.4 | 303.6 | 1,624.3 | 667.5 | 58.91% | 1,352,448 → 3,293,808 |
| validate_deep | 3,136.9 | 813.1 | 6,881.1 | 1,781.5 | 74.11% | 318,786 → 1,229,861 |
| key_deep | 3,321.1 | 1,346.6 | 7,288.3 | 2,954.0 | 59.47% | 301,105 → 742,611 |
| destination_deep | 10,372.1 | 2,790.2 | 22,734.7 | 6,117.2 | 73.09% | 96,412 → 358,397 |
| validate_unicode | 295.8 | 57.0 | 650.6 | 126.4 | 80.57% | 3,380,663 → 17,543,860 |
| key_unicode | 523.1 | 316.2 | 1,146.7 | 695.6 | 39.34% | 1,911,680 → 3,162,555 |
| destination_unicode | 731.5 | 286.6 | 1,607.0 | 630.2 | 60.78% | 1,367,054 → 3,489,184 |
| validate_mixed | 391.0 | 106.6 | 859.3 | 235.3 | 72.62% | 2,557,545 → 9,380,863 |
| key_mixed | 507.6 | 234.0 | 1,115.3 | 515.4 | 53.79% | 1,970,055 → 4,273,504 |
| destination_mixed | 1,022.5 | 401.1 | 2,242.9 | 882.8 | 60.64% | 977,995 → 2,493,144 |
| validate_invalid | 377.1 | 157.5 | 828.8 | 346.8 | 58.16% | 2,651,816 → 6,349,206 |
| progress_0_unique | 80.5 | 4.5 | 178.0 | 11.0 | 93.82% | 12,422,360 → 222,222,222 |
| progress_0_shared | 92.7 | 14.5 | 204.8 | 32.9 | 83.94% | 10,787,487 → 68,965,517 |
| progress_0_missing | 85.1 | 4.3 | 187.9 | 10.5 | 94.41% | 11,750,881 → 232,558,140 |
| progress_0_shared_missing | 92.6 | 14.5 | 204.6 | 32.9 | 83.92% | 10,799,136 → 68,965,517 |
| progress_1_unique | 281.2 | 14.4 | 618.6 | 32.7 | 94.71% | 3,556,188 → 69,444,444 |
| progress_1_shared | 293.8 | 260.0 | 646.0 | 572.0 | 11.46% | 3,403,676 → 3,846,154 |
| progress_1_missing | 173.0 | 4.8 | 380.8 | 11.6 | 96.95% | 5,780,347 → 208,333,333 |
| progress_1_shared_missing | 184.0 | 15.2 | 405.2 | 34.7 | 91.44% | 5,434,783 → 65,789,474 |
| progress_64_unique | 3,830.7 | 13.6 | 8,402.0 | 30.9 | 99.63% | 261,049 → 73,529,412 |
| progress_64_shared | 3,933.4 | 4,017.0 | 8,626.1 | 8,812.9 | -2.17% | 254,233 → 248,942 |
| progress_64_missing | 4,450.8 | 40.7 | 9,762.5 | 90.9 | 99.07% | 224,679 → 24,570,025 |
| progress_64_shared_missing | 4,064.1 | 43.3 | 8,915.9 | 96.4 | 98.92% | 246,057 → 23,094,688 |
| inspect_one | 114,506.2 | 112,243.8 | 250,984.5 | 243,823.4 | 2.85% | 8,733 → 8,909 |
| inspect_many | 3,629,500.0 | 3,446,306.2 | 7,947,573.4 | 7,550,306.2 | 5.00% | 276 → 290 |
| inspect_unicode | 3,580,931.2 | 3,496,862.5 | 7,842,981.8 | 7,659,068.5 | 2.34% | 279 → 286 |

## Allocation churn

Each allocation count equals its free count, and each allocated-byte count
equals its freed-byte count in every row. All figures are per operation.

| Case | Allocs/frees old → new | Reallocs old → new | Allocated/freed bytes old → new |
|---|---:|---:|---:|
| sanitize_short | 1 → 1 | 0 → 0 | 10 → 10 |
| sanitize_reserved | 3 → 1 | 2 → 2 | 64 → 49 |
| sanitize_invalid | 3 → 1 | 2 → 1 | 85 → 45 |
| sanitize_ascii_limit | 1 → 1 | 0 → 0 | 160 → 160 |
| sanitize_ascii_long | 3 → 1 | 2 → 0 | 454 → 160 |
| sanitize_unicode | 3 → 1 | 4 → 2 | 1,792 → 1,120 |
| sanitize_mixed | 3 → 1 | 3 → 1 | 851 → 480 |
| validate_shallow | 4 → 0 | 0 → 0 | 113 → 0 |
| key_shallow | 6 → 1 | 0 → 0 | 141 → 19 |
| destination_shallow | 7 → 1 | 2 → 0 | 198 → 30 |
| validate_deep | 35 → 0 | 4 → 0 | 3,181 → 0 |
| key_deep | 37 → 1 | 4 → 0 | 3,647 → 238 |
| destination_deep | 69 → 1 | 37 → 0 | 15,269 → 249 |
| validate_unicode | 4 → 0 | 0 → 0 | 111 → 0 |
| key_unicode | 6 → 2 | 0 → 0 | 135 → 29 |
| destination_unicode | 7 → 1 | 2 → 0 | 193 → 28 |
| validate_mixed | 5 → 0 | 0 → 0 | 119 → 0 |
| key_mixed | 7 → 1 | 0 → 0 | 161 → 29 |
| destination_mixed | 9 → 1 | 3 → 0 | 270 → 40 |
| validate_invalid | 5 → 1 | 0 → 0 | 186 → 76 |
| progress_0_unique | 1 → 0 | 0 → 0 | 13 → 0 |
| progress_0_shared | 1 → 0 | 0 → 0 | 13 → 0 |
| progress_0_missing | 1 → 0 | 0 → 0 | 13 → 0 |
| progress_0_shared_missing | 1 → 0 | 0 → 0 | 13 → 0 |
| progress_1_unique | 5 → 0 | 0 → 0 | 219 → 0 |
| progress_1_shared | 5 → 4 | 0 → 0 | 219 → 192 |
| progress_1_missing | 3 → 0 | 0 → 0 | 96 → 0 |
| progress_1_shared_missing | 3 → 0 | 0 → 0 | 96 → 0 |
| progress_64_unique | 68 → 0 | 0 → 0 | 5,448 → 0 |
| progress_64_shared | 68 → 67 | 0 → 0 | 5,448 → 5,421 |
| progress_64_missing | 66 → 0 | 0 → 0 | 5,325 → 0 |
| progress_64_shared_missing | 66 → 0 | 0 → 0 | 5,325 → 0 |
| inspect_one | 19 → 14 | 2 → 2 | 1,300 → 1,174 |
| inspect_many | 3079 → 1799 | 512 → 512 | 233,656 → 200,596 |
| inspect_unicode | 3079 → 2055 | 512 → 512 | 230,590 → 202,540 |

## Tradeoffs and interpretation

- `progress_64_shared` used 2.17% more cycles (3,933.4 → 4,017.0 ns/op).

Zero allocation applies to successful path validation, missing-install lookups
and warmed uniquely owned progress updates. Errors still own their messages;
keys and output paths still allocate final storage. Unicode duplicate keys use
an additional lowercase buffer to preserve contextual behavior such as Greek
final sigma. Path buffers reserve an upper bound based on the original name and
can retain spare capacity for the stripped prefix or repeated separators.

Snapshots retained by readers still require a copy on update. The benchmark
therefore includes shared controls, and the unique-update result must not be
interpreted as the cost of every runtime update. The public snapshot type and
reader API are unchanged. Existing readers retain their original values;
unique updates can retain allocation identity, and misses preserve identity.

Sanitizer output still needs its owned string, and Unicode growth or prefix/ID
insertion can reallocate. The original behavior for invalid-character runs,
Windows reserved stems, trailing spaces/dots, scalar limits, ID suffixes and
the changed flag is preserved, including replacement characters after the limit.
The ID-suffix truncation uses UTF-8 boundaries and the exact decimal ID length.

Archive checks retain traversal/control/drive-name rejection, root matching,
file type checks, duplicate output detection, size limits and simfile requirements.
Inspection still completes before extraction creates its staging directory.
Whole archive inspection has smaller gains than path microbenchmarks because
ZIP parsing and filesystem calls remain. These are not overall download-speed
or game frame-rate claims.

## Validation and reproduction

- 269 online tests pass in debug and release, including five new tests.
- Old/new path comparisons exercise 2,278 curated/generated names, all returned
  components, error variants/messages, normalized keys and destination paths.
- Sanitizer comparisons use the same names with seven IDs, including zero and
  u64::MAX, and compare both the output and changed flag.
- Snapshot tests cover missing IDs, unique/shared ownership, weak references,
  message transitions, byte extremes, other runtime fields and retained readers.
- Fourteen local ZIP cases compare complete inspection/extraction results and
  extracted directory trees and file contents. Rejections create no staging dir.
- Allocation assertions require zero churn for validation and warmed unique
  updates, unchanged String/Arc pointers, and one allocation with no growth for
  representative ASCII keys and destination paths.
- Performance lints, the root all-targets check, targeted rustfmt and Git
  whitespace checks pass. Clippy reports existing non-performance warnings.

```powershell
cargo test -p deadsync-online --lib --locked -- --test-threads=1
cargo test -p deadsync-online --release --lib --locked -- --test-threads=1
cargo test -p deadsync-online --release --lib --locked smo_preparation_bench -- --ignored --test-threads=1 --nocapture
$env:DEADSYNC_PERF_REVERSE = '1'
cargo test -p deadsync-online --release --lib --locked smo_preparation_bench -- --ignored --test-threads=1 --nocapture
Remove-Item Env:DEADSYNC_PERF_REVERSE
cargo test -p deadsync-online --release --lib --locked smo_preparation_bench -- --ignored --test-threads=1 --nocapture
cargo clippy -p deadsync-online --all-targets --locked -- -D clippy::perf
cargo check --all-targets --locked
```

The live network test remains ignored. Benchmark logs, per-round measurements
and source hashes are local ignored artifacts under `target/smo-preparation-perf/`.
The baseline and harness are committed so the comparisons can be reproduced.
