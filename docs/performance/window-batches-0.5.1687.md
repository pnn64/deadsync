# Window batch performance 0.5.1687

Parent: `34dfff9a7` (0.5.1686). Date: 2026-10-02.

This pass applies `rust-performance.md`'s M-HOTPATH, M-MEM-REUSE and
M-THROUGHPUT guidance to three gameplay window operations:

1. Paint overlapping note-hide ranges only once when preparing zoom splines.
   Lane windows already have ordered start beats, so their rounded, inclusive
   ranges can skip the previously painted prefix without another buffer.
   The existing spline solve and coefficient arithmetic remain unchanged.
2. Reuse ordered ease windows directly for tail extension, eliminating an
   identity index allocation. Unordered windows retain their compact index
   sort and original-index tie order. For groups of at least 32 windows and
   8-256 constants, prepare a stack-owned cutoff summary once per target:
   sorted constant ends with suffix minimum start times. Each window then
   uses a binary search instead of scanning every constant. Small groups,
   larger constant sets and nonfinite starts retain scalar scans. Nonfinite
   ends and signed-zero corner cases retain the scalar cutoff/fold behavior.
   The bounded summary uses 2048 bytes of stack scratch reused across groups;
   it never introduces a heap allocation.
3. Batch bursts of out-of-order active-window insertion. Keep the original
   individual insertion for the first 32 such activations in an update, then
   append remaining activations and sort the active indices once. Ordered
   arrivals need no sort. The song-setup reservation is reused, and expiry,
   seeks, source replacement, activation order and statistics are preserved.

Note-hide splines and ease tails are prepared during song loading. The active
index is updated by the game thread during play. No dependency or unsafe code
was added; public signatures remain unchanged.

## Measurement method

Intel Xeon E5-2696 v4 (22 cores/44 logical processors), Windows x86-64,
Rust/Cargo 1.98.1, release opt-level 3 and full LTO. Six serial rounds alternate
old-first/new-first after validation and builds finish. Each round reports
medians and ranges of seven timing samples. QueryThreadCycleTime records the
calling thread's CPU cycles. Other host activity was not isolated.

The [raw CSV](window-batches-0.5.1687.csv) contains 240 measurements for 20
paired workloads: elapsed time, cycles, useful-item throughput, allocations,
reallocations, frees, requested bytes and freed bytes. The thread-local System
wrapper counts heap traffic in a separate operation after timing. Every heap
sample balances, and counts/bytes are stable across rounds. Requested bytes
measure allocator traffic rather than peak RSS or allocator metadata. Negative
reductions mean slower. Cycle reductions and throughput increases are medians
of paired round ratios; ns/op values are separate six-round medians. Their
ratios can differ. Complete cycle-change round ranges include control cases.

Fixtures are cloned outside both timing and allocation counting, with fresh
mutable state for every operation. Fixture destruction is also outside both.
The equal per-operation clock overhead affects tiny cycle measurements.
Three original function bodies are frozen from the parent and verified modulo
receiver naming and formatting. Shared scalar cutoff, index allocation,
nonfinite fallback and time/source rebuild helpers are unchanged.

These synthetic cases isolate scaling and allocation behavior. They do not
measure end-to-end song-load time, frame latency or gameplay FPS.

## Note-hide spline preparation

One operation builds the complete spline and drops its owning coefficient
buffer. Source windows are prepared outside measurement. Disjoint controls
have short ranges; overlapping cases have many windows covering almost the
whole spline. Throughput counts generated spline points per second. Both
variants retain one exactly sized coefficient allocation.

| Workload | Old -> new ns/op | CPU-cycle reduction (round range) | Throughput increase | Allocations/op | Requested bytes/op |
|---|---:|---:|---:|---:|---:|
| `hide_64_8_disjoint` | 936.4 -> 863.7 | 1.4% (-3.2..19.1) | 1.1% | 1 -> 1 | 1024 -> 1024 |
| `hide_4096_64_disjoint` | 31432.5 -> 32042.5 | -1.9% (-4.7..12.5) | -1.6% | 1 -> 1 | 65536 -> 65536 |
| `hide_4096_1024_overlap` | 1996121.5 -> 76644.1 | 96.3% (95.3..97.7) | 27.5x | 1 -> 1 | 65536 -> 65536 |
| `hide_65536_256_overlap` | 14066601.6 -> 861042.2 | 93.8% (93.2..94.7) | 1520.9% | 1 -> 1 | 1048576 -> 1048576 |
| `hide_65536_1024_overlap` | 55692636.0 -> 882053.2 | 98.4% (98.3..98.5) | 63.4x | 1 -> 1 | 1048576 -> 1048576 |

Overlapping cases reduce median CPU cycles by 93.8-98.4% without changing
coefficient allocation. Disjoint controls show little median change; the
4096-point control uses 1.9% more cycles, with its round range spanning zero.

## Ease tail extension

One operation extends all tails in place. Output vectors and constants are
prepared outside measurement. Plain inputs are already ordered; mixed-target
and reversed inputs need the original index sort. Counts include only sort
scratch. The 257-constant control retains scalar cutoff scans. Throughput
counts ease windows per second. Ordered inputs now need no heap scratch;
unordered inputs keep the original allocation budget.

| Workload | Old -> new ns/op | CPU-cycle reduction (round range) | Throughput increase | Allocations/op | Requested bytes/op |
|---|---:|---:|---:|---:|---:|
| `tails_16_8_plain` | 911.4 -> 876.1 | 3.1% (-2.7..25.8) | 5.4% | 0 -> 0 | 0 -> 0 |
| `tails_128_0_plain` | 1261.3 -> 1228.6 | 4.7% (-3.7..18.4) | 9.6% | 0 -> 0 | 0 -> 0 |
| `tails_1024_1_plain` | 15671.1 -> 13478.9 | 11.1% (4.4..16.2) | 13.5% | 1 -> 0 | 8192 -> 0 |
| `tails_1024_64_plain` | 317747.7 -> 18153.5 | 94.2% (93.9..94.2) | 1679.1% | 1 -> 0 | 8192 -> 0 |
| `tails_4096_256_plain` | 4760386.0 -> 103582.8 | 97.9% (97.7..97.9) | 47.1x | 1 -> 0 | 32768 -> 0 |
| `tails_1024_64_mixed` | 356717.2 -> 68112.5 | 80.3% (79.5..81.3) | 411.0% | 1 -> 1 | 8192 -> 8192 |
| `tails_1024_64_reversed` | 320134.4 -> 24141.8 | 92.6% (91.8..93.3) | 1291.4% | 1 -> 1 | 8192 -> 8192 |
| `tails_1024_257_plain` | 1450542.9 -> 1439323.4 | -1.3% (-3.3..11.7) | -1.2% | 1 -> 0 | 8192 -> 0 |

Indexed cutoff cases reduce median CPU cycles by 80.3-97.9%. Ordered large
batches remove one allocation and 8192-32768 requested bytes per operation.
Mixed and reversed inputs retain their existing scratch budget. The larger
constant-set control saves allocation while showing little CPU change.

## Active-window updates

One operation activates the complete window batch, or performs 1024 individual
boundary updates for the incremental control. Setup builds/reserves the index
and positions it before the first window outside measurement. Because cloning
an empty Vec loses spare capacity, setup explicitly restores the active-vector
reservation before timing. No case allocates, reallocates or frees during
updates. Throughput counts activated windows per second; the idle control's
operation performs 128 updates and reports updates per second.
Fixture windows contain start/end floats, isolating index maintenance from
attack-effect refresh.

| Workload | Old -> new ns/op | CPU-cycle reduction (round range) | Throughput increase | Allocations/op | Requested bytes/op |
|---|---:|---:|---:|---:|---:|
| `active_8_shuffled_1` | 125.8 -> 116.0 | -0.2% (-3.0..20.4) | -2.0% | 0 -> 0 | 0 -> 0 |
| `active_1024_ordered_1` | 4524.6 -> 4787.1 | -2.9% (-10.2..0.4) | -2.0% | 0 -> 0 | 0 -> 0 |
| `active_1024_reverse_1` | 130439.4 -> 15005.9 | 88.5% (87.1..89.2) | 799.2% | 0 -> 0 | 0 -> 0 |
| `active_8192_reverse_1` | 7859993.8 -> 143769.5 | 98.2% (98.0..98.4) | 56.6x | 0 -> 0 | 0 -> 0 |
| `active_8192_shuffled_1` | 3719774.2 -> 137903.9 | 96.2% (96.0..96.7) | 26.5x | 0 -> 0 | 0 -> 0 |
| `active_1024_shuffled_1024` | 96465.6 -> 93520.3 | 0.8% (-5.1..14.8) | 0.3% | 0 -> 0 | 0 -> 0 |
| `active_1024_idle_128` | 1004.7 -> 895.3 | 6.8% (1.5..28.5) | 12.1% | 0 -> 0 | 0 -> 0 |

Out-of-order bursts reduce median CPU cycles by 88.5-98.2% and retain zero
heap traffic. Incremental activation is close to neutral; the ordered-arrival
control uses 2.9% more cycles, with a round range of -10.2..0.4% reduction.
The optimization targets bursts that previously shifted the vector repeatedly.

## Behavior checks and reproduction

2237 debug/profile cases pass (98 ignored). Four new regression
tests compare parent/new coefficient and sample bits; cutoff results around
epsilon boundaries, signed zeros, unusual ends, invalid constants and fallback
thresholds; and active ordering, ties, expiry, seeks, source changes and stats.
Heap checks prove ordered tail extension and gameplay activation have no
churn, unordered tails retain their budget, and spline output still allocates
exactly once. The final targeted run also passes all four new cases.
852 release unit cases pass (13 ignored). Clippy completes
for gameplay, notefield and song Lua; the final gameplay rerun has no warnings
from the new test/baseline files or the new cutoff code.

3 available native multitap parity cases pass. The additional
`multitap::edgar_countdown_onsets_and_hit_commands` case cannot run here: its
attempt to locate the external song corpus fails because `C:\GitHub\lua-songs`
is absent. The unfiltered native run reports that missing-directory error;
the reproducible available-case run below excludes only that test. The
repository's authored native trace fixtures remain unchanged.

```powershell
cargo test -p deadsync-gameplay -p deadsync-notefield -p deadsync-song-lua --locked
cargo clippy -p deadsync-gameplay -p deadsync-notefield -p deadsync-song-lua --all-targets --locked
cargo test -p deadsync --test song_lua_itgmania_semantic_parity multitap --locked -- --skip edgar_countdown_onsets_and_hit_commands
cargo test --release -p deadsync-gameplay --lib --locked
cargo test --release -p deadsync-gameplay --lib window_batches_perf::benchmark_window_batches -- --exact --ignored --nocapture --test-threads=1
$env:DEADSYNC_PERF_REVERSE = '1'
# Repeat with reversed order; alternate for six rounds.
Remove-Item Env:DEADSYNC_PERF_REVERSE
```
