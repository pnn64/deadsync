# Lua event preparation performance 0.5.1686

Parent: `0410ebce6` (0.5.1685). Date: 2026-10-02.

This pass follows `rust-performance.md`'s M-HOTPATH, M-MEM-REUSE,
M-INITIAL-CAPACITY and M-THROUGHPUT guidance in three preparation paths:

1. Group large overlay ease records by sorting compact indices, then applying
   their gather permutation in place. Comparing the original index last
   preserves the parent's stable order for equal keys. Already ordered records
   need no sort scratch. Fewer than 64 records or records smaller than 128
   bytes retain the original stable sort. Records need neither Clone nor Copy;
   invalid overlay indices are still removed before sorting.
2. Delay ease-window output reservation until an accepted target is pushed.
   Wrong-player, unsupported, ignored and invalid windows no longer reserve
   empty output storage. Nonempty batches retain the original capacity hint
   and reserve it exactly once, including tiny batches. Tail-extension scratch
   and callback order are unchanged. Public append signatures stay unchanged;
   private helpers forward the capacity hint to the actual push.
3. Convert message beats through a stack-owned exact-time event cursor on
   BPM-only maps with more than eight unique, usable, row-aligned BPM points.
   The original continuous conversion kernel and arithmetic are reused. The
   cursor resets on an actual beat rewind, including rewinds inside one note
   row. Nonfinite beats retain the scalar path. Stops, delays, warps, duplicate
   BPMs, invalid BPM values and subrow BPMs keep independent conversions.
   Single-BPM message batches use their original path directly. The new
   iterator borrows immutable timing data and consumes input lazily.

Production callers prepare overlay ease groups and message timestamps in
song-Lua gameplay and playback preparation; these are song-load costs.
No dependency or unsafe production code was added. Timestamp output vectors
and nonempty ease vectors still require owned storage. The optimizations
remove scratch traffic and empty output allocations, rather than promising
zero allocation for all compilation output.

## Measurement method

Windows 11 Pro 10.0.26100, Intel Xeon E5-2696 v4 (22 cores/44 logical
processors), Rust/Cargo 1.98.1, release opt-level 3 with full LTO. Validation
and builds finish before timing. Six serial rounds alternate old-first and
new-first. Each value is the median of seven timing batches, with min/max.
Windows QueryThreadCycleTime measures calling-thread CPU cycles. Other host
activity was not isolated; the complete CPU-change round ranges are shown.

The [raw CSV](event-preparation-0.5.1686.csv) contains 276 measurements for
23 paired workloads, covering elapsed time, cycles, throughput, allocation
calls, reallocations, frees, requested bytes and freed bytes. The repository's
thread-local System wrapper counts allocations in separate operations after
timing. Counts and bytes balance in every sample and are consistent across
rounds. Requested bytes measure allocator traffic, not peak RSS or allocator
metadata. CPU reductions and throughput increases are medians of paired round
ratios; old/new ns/op are separate six-round medians. Their ratios can differ
when samples vary. Negative reductions mean slower.

Ten baseline function bodies, including all modified append dependencies,
are frozen from the parent and checked against it modulo naming and formatting.
The scalar exact-time kernel and ease-tail helpers are unchanged. These
microbenchmarks do not establish end-to-end song-load or gameplay FPS gains.

## Overlay grouping

One operation filters, sorts and groups a prepared record vector into 16
ranges. Input cloning and destruction are outside timing and allocation
accounting; sort scratch and range creation/destruction are included. Each
operation is timed separately, including equal per-call clock overhead. That
overhead materially affects tiny CPU-cycle controls.

Wide deltas are synthetic 520-byte values containing an identifier and 64
u64 payload entries. Their complete ease records are 1088 bytes, representing
large captured state records. The small-record control uses usize deltas.
Throughput counts records per second. Shuffled, tied and ordered cases are
all retained. The input vector allocation is reused in both variants.

| Workload | Old -> new ns/op | CPU-cycle reduction (round range) | Throughput increase | Allocations/op | Requested bytes/op |
|---|---:|---:|---:|---:|---:|
| `group_wide_8_shuffled` | 250.0 -> 268.8 | -1.6% (-8.0..-0.3) | -5.8% | 1 -> 1 | 256 -> 256 |
| `group_wide_64_shuffled` | 15900.0 -> 7154.7 | 53.6% (49.5..85.0) | 128.4% | 2 -> 2 | 69888 -> 768 |
| `group_wide_1024_shuffled` | 1718945.3 -> 209186.0 | 87.8% (87.2..88.1) | 721.8% | 2 -> 2 | 1114368 -> 8448 |
| `group_wide_8192_shuffled` | 16988143.8 -> 2565887.5 | 85.1% (83.7..85.8) | 571.7% | 2 -> 2 | 7999232 -> 65792 |
| `group_wide_1024_ties` | 1267192.2 -> 237787.5 | 81.2% (80.7..81.8) | 434.4% | 2 -> 2 | 1114368 -> 8448 |
| `group_wide_1024_sorted` | 34429.6 -> 12418.8 | 62.0% (58.3..64.3) | 177.3% | 2 -> 1 | 1114368 -> 256 |
| `group_small_1024_shuffled` | 109610.9 -> 110596.9 | -0.9% (-6.0..1.4) | -1.0% | 2 -> 2 | 65792 -> 65792 |

Large shuffled and tied batches reduce median CPU cycles by 81.2-87.8% at
1024-8192 records and requested allocation bytes by about 99%. Ordered wide
records eliminate sort scratch entirely. Tiny and small-record controls show
1.6% and 0.9% median cycle increases; they retain the original sort and
allocation budgets.

## Lazy ease output

One operation builds and drops the complete owning ease output and performs
its existing tail-extension pass. Prepared input strings and windows are
outside measurements. Cases include empty and single-window inputs, all
wrong-player, unsupported and function targets, accepted PlayerX targets,
two-target incoming aliases, and mixed supported/unsupported input. Throughput
counts input windows per second; the empty case uses one operation as a unit.

| Workload | Old -> new ns/op | CPU-cycle reduction (round range) | Throughput increase | Allocations/op | Requested bytes/op |
|---|---:|---:|---:|---:|---:|
| `ease_0_valid` | 37.5 -> 35.9 | 3.2% (0.0..3.3) | 4.3% | 0 -> 0 | 0 -> 0 |
| `ease_1_valid` | 282.9 -> 284.4 | -0.8% (-2.0..1.0) | -0.5% | 1 -> 1 | 128 -> 128 |
| `ease_1024_wrong_player` | 3417.2 -> 1099.2 | 61.1% (47.0..67.8) | 165.3% | 1 -> 0 | 131072 -> 0 |
| `ease_1024_unsupported` | 107035.9 -> 100560.1 | 4.7% (0.2..9.4) | 5.1% | 1 -> 0 | 131072 -> 0 |
| `ease_1024_ignored` | 25837.5 -> 11925.8 | 53.5% (53.1..55.8) | 115.3% | 1 -> 0 | 131072 -> 0 |
| `ease_1024_valid` | 57376.6 -> 42644.6 | 25.0% (19.4..28.7) | 33.3% | 2 -> 2 | 139264 -> 139264 |
| `ease_1024_alias` | 284305.4 -> 286225.8 | -0.3% (-6.3..7.9) | -0.1% | 2 -> 2 | 147456 -> 147456 |
| `ease_1024_mixed` | 75976.5 -> 68431.2 | 10.7% (1.4..17.9) | 12.0% | 2 -> 2 | 136528 -> 136528 |

The three rejected-output batches remove one allocation and 131072 requested
bytes per operation. Wrong-player and ignored batches also reduce median CPU
cycles by 61.1% and 53.5%. Nonempty batches retain their allocation budgets;
the single-window and alias controls have CPU-change ranges spanning zero.

## Message timestamp batches

One operation builds and drops all Option<f32> timestamps. Input timing maps
and beat vectors are prepared outside measurements; compatibility validation,
cursor creation, conversion and output allocation are inside. Each batch has
128-2048 queries. Forward queries advance across the entire map, fractional
queries add 0.0037 beats, rewinds descend, and random queries shuffle order.
Pauses and subrow maps exercise conservative fallback. Throughput counts
queries per second. Every timestamp batch retains one output allocation and
no reallocations; the cached iterator itself allocates nothing.

| Workload | Old -> new ns/op | CPU-cycle reduction (round range) | Throughput increase | Allocations/op | Requested bytes/op |
|---|---:|---:|---:|---:|---:|
| `message_times_1_forward` | 6623.4 -> 6642.2 | -0.3% (-4.7..5.4) | -0.3% | 1 -> 1 | 1024 -> 1024 |
| `message_times_64_forward` | 328717.2 -> 38014.1 | 88.4% (87.4..88.7) | 760.4% | 1 -> 1 | 4096 -> 4096 |
| `message_times_1024_forward` | 19102475.0 -> 202662.5 | 98.9% (98.9..99.0) | 93.5x | 1 -> 1 | 16384 -> 16384 |
| `message_times_64_rewind` | 81281.2 -> 82920.3 | -0.5% (-9.1..1.9) | -0.6% | 1 -> 1 | 1024 -> 1024 |
| `message_times_64_random` | 83179.7 -> 80389.0 | 1.2% (0.8..7.1) | 1.4% | 1 -> 1 | 1024 -> 1024 |
| `message_times_64_fractional` | 328125.0 -> 37118.8 | 88.6% (88.5..89.5) | 781.1% | 1 -> 1 | 4096 -> 4096 |
| `message_times_64_pauses` | 415832.8 -> 414436.0 | -0.0% (-4.7..0.4) | -0.1% | 1 -> 1 | 4096 -> 4096 |
| `message_times_64_subrow` | 325223.4 -> 326050.0 | 0.1% (-4.4..0.6) | 0.1% | 1 -> 1 | 4096 -> 4096 |

Forward and fractional queries reduce median CPU cycles by 88.4-98.9% by
reusing event traversal. Single-BPM, descending, pause and subrow controls
show little median change and ranges spanning zero. This optimization
reduces traversal work; timestamp output allocation remains unchanged.

## Behavior checks and reproduction

2358 debug/profile cases pass (102 ignored), including seven
new regression tests covering stable ties, signed-zero/nonfinite sort keys,
invalid indices, ranges, non-Clone ownership and exactly-once destruction;
empty/tiny output allocation budgets, aliases, callbacks and iterator
consumption; fractional beats, same-row rewinds, nonfinite values, timing-map
fallbacks and lazy allocation-free iteration. 2 native spline
parity cases pass. 973 release unit cases pass (17
ignored). Clippy completes for the four affected crates and all their targets.

```powershell
cargo test -p deadsync-rules -p deadsync-gameplay -p deadsync-notefield -p deadsync-song-lua --locked
cargo clippy -p deadsync-rules -p deadsync-gameplay -p deadsync-notefield -p deadsync-song-lua --all-targets --locked
cargo test -p deadsync --test song_lua_itgmania_semantic_parity position_spline --locked
cargo test --release -p deadsync-rules -p deadsync-gameplay --lib --locked
cargo test --release -p deadsync-gameplay --lib event_preparation_perf::benchmark_event_preparation -- --exact --ignored --nocapture --test-threads=1
$env:DEADSYNC_PERF_REVERSE = '1'
# Repeat the benchmark with reversed order; alternate for six rounds.
Remove-Item Env:DEADSYNC_PERF_REVERSE
```
