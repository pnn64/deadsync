# Timing queries and message lookup performance 0.5.1685

Parent: `5abf1dd0b` (0.5.1684). Date: 2026-10-02.

This pass applies the M-HOTPATH and M-THROUGHPUT guidance in
`rust-performance.md`: measure CPU work as well as allocation traffic, then
remove repeated work while keeping the lookup paths free of allocations.
These paths already allocated nothing in the parent implementation.

1. Validate quantized stop/delay row order once during timing construction.
   The existing beat sort and a non-NaN check establish monotonic rounded rows,
   including saturated infinities. Warp judgment exceptions use a binary
   lower bound and inspect only matching duplicate rows. Tables with at most
   eight entries or invalid row
   order retain the scan. A valid pause in the first entry keeps its immediate
   return. Finite negative durations still count; zero and nonfinite durations
   retain their prior handling.
2. Locate the scroll prefix with binary search when the displayed-beat cursor
   starts or resets after a rewind. Its ordinary forward cursor still advances
   across prefixes once. Validate beat order at construction and retain the
   original scan for tiny tables and malformed prefixes. Nonfinite queries
   retain their existing uncached conversion path and cache state.
3. Fold ASCII case once into a 128-byte stack buffer for short uppercase or
   mixed-case message names, then compare byte slices during binary search.
   Lowercase names keep their direct comparison. Longer names keep the
   streaming comparison with no new length limit. UTF-8 bytes, embedded NULs,
   duplicate command precedence and empty-name behavior are unchanged. This
   helper runs when compiling message events during song loading.

No dependency or unsafe production code was added. TimingData retains three
boolean flags; short uppercase message lookups use 128 bytes of temporary
stack storage. No extra heap index or persistent query buffer is introduced.

## Measurement method

Windows 11 Pro 10.0.26100, Intel Xeon E5-2696 v4 (22 cores/44 logical
processors), Rust/Cargo 1.98.1, release opt-level 3 and full LTO. Our builds
and checks finish before timing. Six serial rounds alternate old-first and
new-first. Each recorded value contains seven timing batches, with their
median and min/max. Windows QueryThreadCycleTime measures calling-thread CPU
cycles. Other host activity was not isolated; complete round ranges are
reported to expose timing variation.

The [raw CSV](timing-queries-0.5.1685.csv) contains 252 measurements across
21 paired workloads, including elapsed time, cycles, throughput, allocations,
reallocations, frees, requested bytes and freed bytes. Allocator accounting
uses the repository's thread-local System wrapper in separate operations
after timing. All allocation/free counts and bytes balance in every sample;
counts are identical across rounds. Requested bytes are allocator traffic,
not peak live memory, RSS or allocator metadata.

Old/new ns/op columns are separate six-round medians. CPU reductions and
throughput increases are medians of paired round ratios; dividing the summary
ns/op columns can give a different ratio. Negative reductions mean slower.
These isolated benchmarks do not establish whole-game FPS or song-load gains.

All four baseline function bodies are frozen from `5abf1dd0b` and checked
against that commit. The old constructor defaults the new flags solely for
layout compatibility and does not perform their validation. Other timing and
message-index construction helpers are unchanged in this pass.

## Pause rows

One operation performs 128 queries against prepared timing data. Each fixture
contains the indicated number of stops and delays, including zero-duration
entries. Cases cover absent rows, a late hit, and an actual valid first hit.
Fixture construction and destruction are outside query measurements.

| Workload | Old -> new ns/op | CPU-cycle reduction (round range) | Throughput increase | Allocations/op | Requested bytes/op |
|---|---:|---:|---:|---:|---:|
| `pause_4_miss` | 4042.1 -> 4377.0 | -7.2% (-12.7..-1.8) | -6.6% | 0 -> 0 | 0 -> 0 |
| `pause_64_miss` | 61747.3 -> 16016.4 | 74.4% (72.9..76.8) | 291.3% | 0 -> 0 | 0 -> 0 |
| `pause_4096_miss` | 3738749.6 -> 32023.1 | 99.2% (99.1..99.2) | 119.5x | 0 -> 0 | 0 -> 0 |
| `pause_4096_late_hit` | 3736689.8 -> 32849.2 | 99.1% (99.1..99.3) | 113.9x | 0 -> 0 | 0 -> 0 |
| `pause_4096_first_hit` | 750.8 -> 690.2 | 9.3% (-22.5..18.7) | 10.3% | 0 -> 0 | 0 -> 0 |

The four-entry miss control uses 7.2% more CPU cycles, approximately 2.6 ns
more per query from the elapsed-time summary medians. The first-hit range
crosses zero, so its median does not establish a consistent gain. The large
miss and late-hit gains hold in every round; all queries retain zero churn.

## Displayed-beat cursor

One operation performs 128 queries against prepared scroll prefixes. Rewinds
decrease the query beat near the end of the table; initial queries explicitly
reset the cursor each time. Steady repeats one beat and forward advances by
0.25 beats. This change targets initial positioning and rewinds; initialized
forward jumps keep their existing cursor traversal.

The current production callers precompute note heads and hold tails in
`runtime_init.rs`. Late initial notes and decreasing hold-end beats exercise
the optimized positioning paths during chart initialization.

| Workload | Old -> new ns/op | CPU-cycle reduction (round range) | Throughput increase | Allocations/op | Requested bytes/op |
|---|---:|---:|---:|---:|---:|
| `display_4_rewind` | 960.1 -> 1080.5 | -10.6% (-60.9..1.1) | -9.6% | 0 -> 0 | 0 -> 0 |
| `display_64_rewind` | 5698.4 -> 2174.2 | 60.9% (54.9..72.6) | 157.9% | 0 -> 0 | 0 -> 0 |
| `display_4096_rewind` | 300565.6 -> 4431.2 | 98.6% (98.3..98.7) | 72.2x | 0 -> 0 | 0 -> 0 |
| `display_65536_initial` | 4732726.6 -> 5691.4 | 99.9% (99.9..99.9) | 830.9x | 0 -> 0 | 0 -> 0 |
| `display_4096_steady` | 740.7 -> 752.4 | 0.8% (-4.6..7.0) | 3.2% | 0 -> 0 | 0 -> 0 |
| `display_4096_forward` | 818.8 -> 800.0 | 2.5% (-9.0..10.0) | 2.5% | 0 -> 0 | 0 -> 0 |

The four-entry rewind control uses 10.6% more CPU cycles, approximately
0.9 ns more per query from the summary medians. Steady and forward controls
have ranges spanning zero, so this pass claims gains for large positioning
and rewind queries rather than ordinary cursor advancement.

## Message commands

One operation performs 128 lookups in a prepared index. Names share a prefix
and vary by command number. Cases cover uppercase, mixed case, lowercase,
misses, and 260-byte names exercising the streaming fallback. Index building,
query strings and destruction are outside measurements.

| Workload | Old -> new ns/op | CPU-cycle reduction (round range) | Throughput increase | Allocations/op | Requested bytes/op |
|---|---:|---:|---:|---:|---:|
| `message_8_upper` | 13347.5 -> 6306.1 | 55.3% (49.0..58.2) | 123.7% | 0 -> 0 | 0 -> 0 |
| `message_64_upper` | 25731.5 -> 8833.8 | 64.8% (60.9..66.6) | 184.6% | 0 -> 0 | 0 -> 0 |
| `message_1024_upper` | 35381.7 -> 14591.6 | 57.7% (49.7..63.2) | 136.4% | 0 -> 0 | 0 -> 0 |
| `message_64_lower` | 10530.6 -> 10466.2 | -4.8% (-8.7..10.6) | -4.5% | 0 -> 0 | 0 -> 0 |
| `message_64_mixed` | 24565.8 -> 9241.0 | 62.2% (57.3..64.8) | 164.9% | 0 -> 0 | 0 -> 0 |
| `message_64_long` | 213126.5 -> 218561.4 | -2.2% (-13.6..0.7) | -2.2% | 0 -> 0 | 0 -> 0 |
| `message_64_miss` | 21769.3 -> 9380.9 | 57.9% (48.7..64.4) | 137.4% | 0 -> 0 | 0 -> 0 |

Short uppercase lookups reduce CPU cycles by 55.3-64.8%; mixed case reduces
them by 62.2%. The lowercase and long-name controls use 4.8% and 2.2% more
cycles in their paired medians, with ranges spanning zero. The improvement
is specific to names that use the stack-folded comparison.

## Timing construction cost

One operation constructs and drops complete timing data with one BPM and the
indicated number of stop/delay/scroll triplets, using empty note-row input.
Throughput here counts triplets per second, rather than queries. This control
includes the extra linear validation done at construction.

| Workload | Old -> new ns/op | CPU-cycle reduction (round range) | Throughput increase | Allocations/op | Requested bytes/op |
|---|---:|---:|---:|---:|---:|
| `timing_build_4` | 798.0 -> 835.5 | -1.0% (-5.3..4.5) | -1.1% | 10 -> 10 | 352 -> 352 |
| `timing_build_64` | 1700.8 -> 1749.2 | -11.7% (-49.2..32.9) | -1.5% | 10 -> 10 | 2512 -> 2512 |
| `timing_build_4096` | 127743.8 -> 129868.8 | -2.1% (-21.6..11.4) | -1.8% | 10 -> 10 | 147664 -> 147664 |

Construction carries the validation cost: paired CPU medians are 1.0%,
11.7% and 2.1% higher for the three sizes, with wide ranges. Each variant
still makes ten allocations and ten frees, no reallocations, and requests
the same number of bytes. This is a query-speed tradeoff, with construction
and small-input costs retained explicitly in the results.

## Behavior checks and reproduction

2351 debug/profile test cases pass (101 ignored), including
six new regression tests covering quantized duplicate rows, zero/nonfinite
and negative pause durations, malformed row order, warp exceptions, cache
values and state across rewinds/nonfinite inputs, malformed scroll prefixes,
message casing/UTF-8/duplicates/NULs/empty names, the 128-byte boundary and
long names. Allocation assertions verify zero churn for the optimized queries.
2 native spline parity cases pass. 966 release unit
cases pass (16 ignored). Clippy completes for all four affected
crates and their test targets with existing repository warnings.

```powershell
cargo test -p deadsync-rules -p deadsync-gameplay -p deadsync-notefield -p deadsync-song-lua --locked
cargo clippy -p deadsync-rules -p deadsync-gameplay -p deadsync-notefield -p deadsync-song-lua --all-targets --locked
cargo test -p deadsync --test song_lua_itgmania_semantic_parity position_spline --locked
cargo test --release -p deadsync-rules -p deadsync-gameplay --lib --locked
cargo test --release -p deadsync-rules --lib timing::timing_queries_perf::benchmark_timing_queries -- --exact --ignored --nocapture --test-threads=1
cargo test --release -p deadsync-gameplay --lib message_lookup_perf::benchmark_message_lookup -- --exact --ignored --nocapture --test-threads=1
$env:DEADSYNC_PERF_REVERSE = '1'
# Repeat the two benchmark commands with reversed order; alternate for six rounds.
Remove-Item Env:DEADSYNC_PERF_REVERSE
```
