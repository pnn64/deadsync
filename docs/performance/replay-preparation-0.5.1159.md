# Local replay preparation - 0.5.1159

Baseline: `07ef01b31` (0.5.1158). This pass applies the supplied
`rust-performance.md` guidance on measured CPU/allocation costs (M-HOTPATH),
reuse of owned allocations (M-MEM-REUSE), and sufficient output capacity
(M-INITIAL-CAPACITY). The guide itself is excluded from the commit.

Preparing five replay entries from 4,096 candidates uses 40.90%
fewer median thread cycles and 28.68% less
requested-byte churn. Selecting five candidates from that history uses
40.77% fewer cycles. Converting 65,536 valid replay edges uses
42.72% fewer cycles and halves byte churn, including
the common input clone. Date formatting for a modern local timestamp uses
35.22% fewer cycles. These are in-memory preparation results;
filesystem reads and binary decoding are outside benchmark timing.

## Changes

1. **Select replay candidates incrementally.** The active local replay loader
   formerly sorted every candidate before reading its requested prefix. Large
   histories with a small requested prefix now build a best-first binary heap
   in the existing candidate allocation. The score/date/name/ordinal comparator
   is unchanged. Popping the next candidate continues past unreadable files,
   preserving the read sequence and the ranks of successful entries. Histories
   of at most 64 candidates, requests above one eighth of the history, and NaN
   scores retain the original sort. Already ordered and strictly reversed inputs
   have linear paths. If selection consumes four times the requested count
   (at least 16 candidates), it sorts the remaining buffer so a long run of
   unreadable files does not require a heap pop for the entire history.
2. **Convert replay edges in their owned buffer.** Consuming filter/map collection
   lets Vec reuse storage for the equally sized local/output edge types on the
   measured target. The invalid-time filter, edge order, lane values, pressed
   state and fallback mapping of unknown input sources stay the same. No new
   unsafe code or serialized-layout change is introduced. Returned replay
   storage can retain the original spare capacity.
3. **Write score dates directly into one final string.** Fixed Chrono formatting
   items replace repeated format-string parsing. Checked local-time conversion
   avoids formatting an unused timezone string, and write_to avoids the temporary
   display string. The final buffer reserves 22 bytes, covering signed six-digit
   years. UTC boundary cases whose local dates exceed NaiveDate's range keep
   the original DateTime formatter. Timestamp validity and local timezone
   resolution remain unchanged.

## Method

Windows x86_64 MSVC; Intel Xeon E5-2696 v4 @ 2.20 GHz; rustc 1.98.0
(88d9e12ae, 2026-08-18), LLVM 22.1.8; Chrono 0.4.45. Host timezone is
Romance Standard Time (Paris). Release optimization level 3, full LTO.

Frozen test-only functions retain the old local date formatter, complete
leaderboard/replay entry construction, replay candidate comparator and filesystem
loader. Two adapters expose the original conversion loop and sorting step
independently of surrounding work. Audits compare those bodies with the prior
commit and verify that header readers, full-record readers, candidate gathering,
score arithmetic and entry fields are unchanged.

All 36 pairs run in the same release executable with black-boxed inputs.
Three invocations alternate old/new, new/old, old/new. Each invocation uses
three warmups, seven timing batches and a separate allocation-counted operation.
Tables report the median of the three per-invocation medians. Source hashes are
checked before/after measurement and before commit. No Cargo build runs beside
the benchmark. These local measurements do not provide confidence intervals.

CPU counts use Windows QueryThreadCycleTime for the calling thread. The existing
scoped Rust allocator delegates to System; timing runs with allocation counting
disabled. Results and temporary input buffers are dropped inside the operation.
Byte counters include allocations and reallocation replacement sizes; they
measure churn, not peak RSS, committed pages, cache misses, filesystem throughput
or the production allocator's performance.

Batch sizes:

- Date formatting and single leaderboard entry: 8,192 operations.
- Edge conversion and replay entry construction: 1,024 operations through 4,096
  input edges, 32 for 65,536 edges.
- Selection: 1,024 operations through 256 candidates, otherwise 32.
- Combined preparation: 256 operations through 256 candidates, otherwise 32.

Selection fixtures contain 0, 16, 256 or 4,096 candidates, with owned 47-byte paths,
borrowed initials, deterministic shuffled scores, seven timestamp values and
unique source ordinals. Additional cases request 1, 32 or all 4,096 records;
use sorted, reversed, equal-score or all-NaN input; or model half/all files
being unreadable. Both sides clone the candidate vector and paths inside the
measured operation to supply equivalent owned inputs. Selection returns an
order-dependent ordinal checksum, so output-vector growth cannot distort the
comparison. The ordered iterator itself allocates nothing, including its
transition from heap selection to sorting the remaining buffer.

Replay fixtures contain 0, 64, 4,096 or 65,536 edges. Mixed conversion invalidates
every third timestamp; another case invalidates every edge. Source codes cycle
through keyboard, gamepad and two unknown values, and lanes span the full u8
range. Both sides clone the source Vec inside each operation; the old path
then allocates a second Vec, while the new path keeps the clone's allocation.
Full replay-entry cases invalidate every seventh timestamp and include an owned
Unicode name, rank, score/date preparation and disposal.

Combined preparation clones the candidate history, selects five winners and
constructs complete replay entries with the old/new date and edge implementations.
It uses 64 edges per selected replay for 16 candidates and 4,096 edges for
256/4,096 candidates. Both sides clone these replay inputs in the measurement.
It excludes disk reads and bincode decoding; filesystem behavior is tested
separately against the frozen loader.

Date cases cover epoch, modern, pre-epoch and invalid timestamps through the
full local-time API. Fixed-offset cases isolate formatting for UTC, a +05:30
offset and years -100,000/+100,000. Invalid timestamps are zero-allocation
controls on both sides; their tiny timing differences do not imply a changed
validation algorithm.

## CPU time and throughput

Positive saved percentages mean fewer cycles. Throughput is complete operations
per second calculated from median ns/op. Raw output additionally reports input
edges/s or candidates/s for those groups, and completed replay entries/s for
combined preparation. No filesystem throughput is inferred from these figures.

### Date and leaderboard entry preparation

| Case | ns/op old -> new | Thread cycles/op old -> new | Cycles saved | Operations/s old -> new |
| --- | ---: | ---: | ---: | ---: |
| `leaderboard_entry` | 1,914.4 -> 1,127.7 | 4,197.5 -> 2,471.3 | 41.12% | 522,356.9 -> 886,760.7 |
| `date_epoch` | 1,522.8 -> 990.0 | 3,333.7 -> 2,171.7 | 34.86% | 656,685.1 -> 1,010,101.0 |
| `date_modern` | 1,526.4 -> 985.3 | 3,336.0 -> 2,161.0 | 35.22% | 655,136.3 -> 1,014,919.3 |
| `date_past` | 1,545.2 -> 978.0 | 3,387.8 -> 2,143.0 | 36.74% | 647,165.4 -> 1,022,494.9 |
| `date_invalid` | 10.3 -> 4.3 | 22.8 -> 9.7 | 57.46% | 97,087,378.6 -> 232,558,139.5 |
| `date_fixed_utc` | 730.5 -> 128.7 | 1,602.4 -> 281.6 | 82.43% | 1,368,925.4 -> 7,770,007.8 |
| `date_fixed_offset` | 690.1 -> 135.9 | 1,512.2 -> 298.0 | 80.29% | 1,449,065.4 -> 7,358,351.7 |
| `date_fixed_negative_year` | 724.5 -> 154.0 | 1,584.1 -> 337.4 | 78.70% | 1,380,262.2 -> 6,493,506.5 |
| `date_fixed_future_year` | 707.3 -> 154.4 | 1,551.3 -> 337.7 | 78.23% | 1,413,827.2 -> 6,476,683.9 |

### Replay edge and entry preparation

| Case | ns/op old -> new | Thread cycles/op old -> new | Cycles saved | Operations/s old -> new |
| --- | ---: | ---: | ---: | ---: |
| `edges_0_valid` | 13.8 -> 2.3 | 31.5 -> 6.4 | 79.68% | 72,463,768.1 -> 434,782,608.7 |
| `edges_64_valid` | 234.7 -> 159.9 | 508.5 -> 352.2 | 30.74% | 4,260,758.4 -> 6,253,908.7 |
| `edges_64_mixed` | 212.8 -> 130.5 | 468.2 -> 287.5 | 38.59% | 4,699,248.1 -> 7,662,835.2 |
| `edges_4096_valid` | 34,540.4 -> 9,050.9 | 75,608.1 -> 19,841.4 | 73.76% | 28,951.6 -> 110,486.2 |
| `edges_4096_mixed` | 11,633.7 -> 7,948.7 | 25,474.0 -> 17,430.5 | 31.58% | 85,957.2 -> 125,806.7 |
| `edges_65536_valid` | 880,978.1 -> 504,665.6 | 1,925,220.8 -> 1,102,733.7 | 42.72% | 1,135.1 -> 1,981.5 |
| `edges_65536_mixed` | 828,159.4 -> 531,915.6 | 1,809,276.7 -> 1,163,830.1 | 35.67% | 1,207.5 -> 1,880.0 |
| `edges_all_invalid` | 6,452.2 -> 5,704.9 | 14,149.8 -> 12,501.0 | 11.65% | 154,985.9 -> 175,287.9 |
| `entry_0` | 1,637.7 -> 1,028.4 | 3,591.5 -> 2,258.4 | 37.12% | 610,612.4 -> 972,384.3 |
| `entry_4096` | 16,478.7 -> 9,183.0 | 35,978.3 -> 20,122.0 | 44.07% | 60,684.4 -> 108,896.9 |
| `entry_65536` | 840,603.1 -> 497,406.2 | 1,840,212.6 -> 1,090,057.5 | 40.76% | 1,189.6 -> 2,010.4 |

### Candidate selection

| Case | ns/op old -> new | Thread cycles/op old -> new | Cycles saved | Operations/s old -> new |
| --- | ---: | ---: | ---: | ---: |
| `select_empty` | 17.2 -> 23.7 | 39.4 -> 53.4 | -35.53% | 58,139,534.9 -> 42,194,092.8 |
| `select_small` | 1,557.8 -> 1,528.3 | 3,414.7 -> 3,355.9 | 1.72% | 641,930.9 -> 654,321.8 |
| `select_medium` | 25,567.6 -> 20,131.7 | 55,993.3 -> 44,050.5 | 21.33% | 39,112.0 -> 49,672.9 |
| `select_large` | 673,900.0 -> 399,859.4 | 1,474,100.3 -> 873,116.2 | 40.77% | 1,483.9 -> 2,500.9 |
| `select_one` | 684,209.4 -> 431,343.8 | 1,499,143.8 -> 945,297.3 | 36.94% | 1,461.5 -> 2,318.3 |
| `select_thirty_two` | 668,231.2 -> 396,631.2 | 1,458,858.8 -> 868,829.0 | 40.44% | 1,496.5 -> 2,521.2 |
| `select_all` | 694,634.4 -> 686,837.5 | 1,518,034.6 -> 1,503,396.7 | 0.96% | 1,439.6 -> 1,455.9 |
| `select_sorted` | 373,771.9 -> 380,593.8 | 817,075.0 -> 834,010.9 | -2.07% | 2,675.4 -> 2,627.5 |
| `select_reverse` | 404,137.5 -> 398,428.1 | 885,860.8 -> 873,424.8 | 1.40% | 2,474.4 -> 2,509.9 |
| `select_ties` | 338,156.2 -> 352,184.4 | 741,155.5 -> 769,628.8 | -3.84% | 2,957.2 -> 2,839.4 |
| `select_nan` | 440,453.1 -> 360,653.1 | 965,182.7 -> 790,392.0 | 18.11% | 2,270.4 -> 2,772.7 |
| `select_half_missing` | 803,425.0 -> 441,359.4 | 1,758,222.5 -> 963,186.5 | 45.22% | 1,244.7 -> 2,265.7 |
| `select_all_missing` | 815,656.2 -> 945,984.4 | 1,786,140.1 -> 2,071,270.6 | -15.96% | 1,226.0 -> 1,057.1 |

### Combined preparation

| Case | ns/op old -> new | Thread cycles/op old -> new | Cycles saved | Operations/s old -> new |
| --- | ---: | ---: | ---: | ---: |
| `prepare_16` | 13,804.7 -> 8,902.3 | 30,280.7 -> 19,528.6 | 35.51% | 72,439.1 -> 112,330.5 |
| `prepare_256` | 139,781.2 -> 78,297.3 | 306,106.5 -> 170,951.9 | 44.15% | 7,154.0 -> 12,771.8 |
| `prepare_4096` | 772,587.5 -> 457,484.4 | 1,692,722.3 -> 1,000,467.3 | 40.90% | 1,294.4 -> 2,185.9 |

The 4,096-candidate combined case changes completed replay throughput from
6,472 to 10,929 entries/s.

## Allocation and byte churn

Counts are per complete operation, including the common input clones described
above. Freed-byte totals equal requested-byte totals for every old/new row,
including reallocation accounting. The actual edge conversion and candidate
ordering helpers reuse already-owned storage with no additional allocation.

| Case | Allocations old -> new | Reallocations old -> new | Frees old -> new | Requested bytes old -> new |
| --- | ---: | ---: | ---: | ---: |
| `leaderboard_entry` | 5 -> 3 | 2 -> 0 | 5 -> 3 | 94 -> 33 |
| `date_epoch` | 3 -> 1 | 2 -> 0 | 3 -> 1 | 83 -> 22 |
| `date_modern` | 3 -> 1 | 2 -> 0 | 3 -> 1 | 83 -> 22 |
| `date_past` | 3 -> 1 | 2 -> 0 | 3 -> 1 | 83 -> 22 |
| `date_invalid` | 0 -> 0 | 0 -> 0 | 0 -> 0 | 0 -> 0 |
| `date_fixed_utc` | 3 -> 1 | 2 -> 0 | 3 -> 1 | 83 -> 22 |
| `date_fixed_offset` | 3 -> 1 | 2 -> 0 | 3 -> 1 | 83 -> 22 |
| `date_fixed_negative_year` | 3 -> 1 | 2 -> 0 | 3 -> 1 | 86 -> 22 |
| `date_fixed_future_year` | 3 -> 1 | 2 -> 0 | 3 -> 1 | 86 -> 22 |
| `edges_0_valid` | 0 -> 0 | 0 -> 0 | 0 -> 0 | 0 -> 0 |
| `edges_64_valid` | 2 -> 1 | 0 -> 0 | 2 -> 1 | 2,048 -> 1,024 |
| `edges_64_mixed` | 2 -> 1 | 0 -> 0 | 2 -> 1 | 2,048 -> 1,024 |
| `edges_4096_valid` | 2 -> 1 | 0 -> 0 | 2 -> 1 | 131,072 -> 65,536 |
| `edges_4096_mixed` | 2 -> 1 | 0 -> 0 | 2 -> 1 | 131,072 -> 65,536 |
| `edges_65536_valid` | 2 -> 1 | 0 -> 0 | 2 -> 1 | 2,097,152 -> 1,048,576 |
| `edges_65536_mixed` | 2 -> 1 | 0 -> 0 | 2 -> 1 | 2,097,152 -> 1,048,576 |
| `edges_all_invalid` | 2 -> 1 | 0 -> 0 | 2 -> 1 | 131,072 -> 65,536 |
| `entry_0` | 4 -> 2 | 2 -> 0 | 4 -> 2 | 90 -> 29 |
| `entry_4096` | 6 -> 3 | 2 -> 0 | 6 -> 3 | 131,162 -> 65,565 |
| `entry_65536` | 6 -> 3 | 2 -> 0 | 6 -> 3 | 2,097,242 -> 1,048,605 |
| `select_empty` | 0 -> 0 | 0 -> 0 | 0 -> 0 | 0 -> 0 |
| `select_small` | 17 -> 17 | 0 -> 0 | 17 -> 17 | 1,904 -> 1,904 |
| `select_medium` | 257 -> 257 | 0 -> 0 | 257 -> 257 | 30,464 -> 30,464 |
| `select_large` | 4,097 -> 4,097 | 0 -> 0 | 4,097 -> 4,097 | 487,424 -> 487,424 |
| `select_one` | 4,097 -> 4,097 | 0 -> 0 | 4,097 -> 4,097 | 487,424 -> 487,424 |
| `select_thirty_two` | 4,097 -> 4,097 | 0 -> 0 | 4,097 -> 4,097 | 487,424 -> 487,424 |
| `select_all` | 4,097 -> 4,097 | 0 -> 0 | 4,097 -> 4,097 | 487,424 -> 487,424 |
| `select_sorted` | 4,097 -> 4,097 | 0 -> 0 | 4,097 -> 4,097 | 487,424 -> 487,424 |
| `select_reverse` | 4,097 -> 4,097 | 0 -> 0 | 4,097 -> 4,097 | 487,424 -> 487,424 |
| `select_ties` | 4,097 -> 4,097 | 0 -> 0 | 4,097 -> 4,097 | 487,424 -> 487,424 |
| `select_nan` | 4,097 -> 4,097 | 0 -> 0 | 4,097 -> 4,097 | 487,424 -> 487,424 |
| `select_half_missing` | 4,097 -> 4,097 | 0 -> 0 | 4,097 -> 4,097 | 487,424 -> 487,424 |
| `select_all_missing` | 4,097 -> 4,097 | 0 -> 0 | 4,097 -> 4,097 | 487,424 -> 487,424 |
| `prepare_16` | 48 -> 33 | 10 -> 0 | 48 -> 33 | 13,054 -> 7,629 |
| `prepare_256` | 288 -> 273 | 10 -> 0 | 288 -> 273 | 686,734 -> 358,749 |
| `prepare_4096` | 4,128 -> 4,113 | 10 -> 0 | 4,128 -> 4,113 | 1,143,694 -> 815,709 |

## Limits and regressions

Measured cycle regressions in this run:

- `select_empty`: 35.53% more median cycles.
- `select_sorted`: 2.07% more median cycles.
- `select_ties`: 3.84% more median cycles.
- `select_all_missing`: 15.96% more median cycles.

The empty-selection difference is 6.5 ns/op, with zero heap churn on both sides.
Heap preparation can cost more on unusual selection workloads; the bulk-sort,
ordered/reversed and exhaustion paths bound that tradeoff without changing
which readable records are selected. NaN inputs retain the old partial comparator
and sort behavior. Production candidates have unique traversal ordinals that
resolve otherwise equal ranking keys.

Vec's allocation reuse is a compiler/library optimization, checked on the
locked target by pointer and churn assertions. An oversized input replay retains
its spare capacity: the test with 257 edges in a 512-edge buffer keeps that
capacity, whereas the old output reserved 257 edges. Benchmark input clones
reserve their actual lengths. Date strings reserve 22 bytes, including three
spare bytes for ordinary four-digit years. Reduced allocation churn does not
by itself prove lower retained memory or peak process memory.

## Behavior and checks

All 239 score unit tests pass in debug and release, including six new tests:

- Exact local date output for known values and 512 generated timestamps, plus
  fixed-offset/year/range boundary comparisons against Chrono's old formatter.
- Exact replay edge order and fields for empty, valid, mixed and invalid data,
  all 256 source codes and extreme timestamps; complete entry comparisons include
  unusual scores, NaN payload bits, extreme ranks, names, flags and beat-zero time.
- Pointer/capacity preservation and zero churn for valid/mixed replay conversion;
  a single 22-byte output allocation for fixed date formatting.
- Exact candidate ordinal sequences across sizes, request limits, ties, NaNs,
  sorted/reversed inputs and skipped candidates, including draining the iterator.
- Candidate-buffer identity and zero churn through heap construction, popping
  and the transition to sorting the remainder.
- Real file reads compared with the frozen loader across missing files,
  truncated payloads, valid replays, all-unreadable input and different limits.
  Existing corrupt-winner and local-score filesystem tests also pass.

Clippy performance checks, application all-target checks, targeted rustfmt and
git whitespace checks pass. Existing warnings outside the changed code remain.
Cargo.toml and the three shared-version entries in Cargo.lock change exactly
once from 0.5.1158 to 0.5.1159. No dependency is added.

Reproduce from the repository root (PowerShell):

```powershell
cargo test -p deadsync-score --lib --locked
cargo test -p deadsync-score --lib --release --locked
cargo test -p deadsync-score --lib --release --locked replay_preparation_bench -- --ignored --test-threads=1 --nocapture
$env:DEADSYNC_PERF_REVERSE = '1'
cargo test -p deadsync-score --lib --release --locked replay_preparation_bench -- --ignored --test-threads=1 --nocapture
Remove-Item Env:DEADSYNC_PERF_REVERSE
cargo test -p deadsync-score --lib --release --locked replay_preparation_bench -- --ignored --test-threads=1 --nocapture
cargo clippy -p deadsync-score --all-targets --locked -- -D clippy::perf
cargo check --all-targets --locked
```

The two added benchmarks are ignored during ordinary tests. Frozen implementations
and measurement helpers compile only in tests.
