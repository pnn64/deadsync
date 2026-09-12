# Song-Lua timeline compilation - 0.5.1170

Parent: `82e409572` (`0.5.1169`). This pass applies the local
`rust-performance.md` guidance on measured hot paths (M-HOTPATH), reusing
owned storage (M-MEM-REUSE), and avoiding repeated work (M-THROUGHPUT).

## Three changes

1. **Use static speedmod lookup strings.** Each player-option sample previously
   formatted three fixed Lua keys. The keys are now string literals. Raw state
   lookup, speedmod override order, metamethods, and errors retain their behavior.
2. **Reuse mod-window lookup storage and defer owned targets.** During update
   compilation, one tuple/String lookup buffer persists across players and
   samples. Extending an unchanged speed-controlled window no longer constructs
   a temporary owned target or cache key. Changed windows update existing cache
   indices in place; new keys and emitted targets still own their strings.
   Baseline and next-value lookups occur only in the interpolated branch that
   uses them, and empty input states return immediately. The public
   `push_update_mod_targets` signature remains unchanged;
   it delegates with call-local scratch, while the compiler retains scratch
   across its whole conversion loop.
3. **Append ordered overlay samples incrementally.** A sample that follows the
   existing compacted track only needs to compare against its last item.
   Epsilon-close writes replace both its value and timestamp. Overlapping or
   out-of-order samples retain the previous stable sort and compaction path.
   Initial and final track normalization remain in place. For ordered batches,
   the repeated full-prefix scans are removed; sorting the incoming batch is
   unchanged.

These improvements apply to song-Lua loading and update compilation. They do
not establish an improvement in gameplay frame rate or total song-load time.
No dependencies or production unsafe code were added.

## Behavior and validation

The test-only baseline freezes four complete functions from the parent. The
source audit checks their bodies against that commit, permitting formatting
and test visibility changes. Both versions share unchanged helper functions.

Eight new tests compare complete outputs and cache maps, with exact float bits
for window values/times and track timestamps. Coverage includes two-player mod
changes, unsupported and column-specific keys, zero/negative/nonfinite values,
stale cache indices, cleared outputs, speed-unit conversion, Lua overrides and
conversion errors, metamethod lookup order and error short-circuiting,
overlapping/reversed schedules, stable ties, epsilon chains, signed zero,
infinities and NaN beat payloads, prefilled unsorted/empty tracks, and retained
Arc ownership. Deterministic generated cases exercise 24 mod-state sequences
of 32 ticks and 64 overlay sequences of eight batches.

Separate allocation assertions confirm zero measured Rust heap churn for
warmed empty option sampling and for 64 unchanged mod windows extended across
599 further samples. Initial cache/output construction is excluded from those
assertions and included in the cold/batch benchmarks. Emitting new windows,
collecting populated option maps, and growing output tracks can still allocate.

Validation commands and outcomes:

- `cargo test -p deadsync-song-lua --lib update_timeline_perf --locked`:
  **eight passed**, one manual benchmark ignored. All eight also pass in the
  full debug and release runs.
- `cargo test -p deadsync-song-lua --locked` and the serial rerun with
  `-- --test-threads=1`: **407 passed, five failed, eight ignored**.
- `cargo test -p deadsync-song-lua --lib --release --locked`:
  **407 passed, the same five failed, eight ignored**.
- The unchanged parent perframe implementation, tested serially in the same
  workspace, has **399 passed, the same five failed, seven ignored**. An audit
  confirms identical failing assertion text and source locations across parent,
  parallel debug, serial debug, and release runs. These failures predate this
  pass: `compile_song_lua_extracts_actorproxy_targets`,
  `compile_song_lua_layers_share_init_globals_and_actor_refs`,
  `compile_song_lua_resolves_local_and_hidden_screen_proxy_targets`,
  `compile_song_lua_runs_cmd_queuecommand_builders`, and
  `compile_song_lua_supports_notefield_column_api`. They concern existing actor
  initialization and note-column expectations; the full suite is not green.
- `cargo test -p deadsync-song-lua --doc --locked`: **passed**, zero doctests.
- `cargo check --all-targets --offline`: **passed**.
- `cargo clippy -p deadsync-song-lua --all-targets --locked -- -D clippy::perf`:
  **passed**; non-performance warnings remain, including argument-count style
  warnings.
- Three isolated runs of the 23-case release benchmark: **passed**, including
  the output comparisons performed before each mod/overlay timing pair.
- Baseline-body and version/lock audits: **passed**.

## Measurement method and limits

Windows x86-64, Intel Xeon E5-2696 v4 @ 2.20 GHz, Rust 1.98.0
(`88d9e12ae`, LLVM 22.1.8), release optimization with full LTO. The shared
`tests/support/perf.rs` helper records seven timing samples after three warmups,
then measures allocation churn separately. Three executable runs alternate
old/new order (old first, new first, old first), with no concurrent Cargo builds.
Tables report the median of the three per-run medians. Windows
`QueryThreadCycleTime` measures cycles used by the calling thread; it does not
measure retired instructions or provide a cross-machine metric.

Both implementations run in the same binary with the same fixture and
black-box boundaries. Correctness checks run outside timing. Returned output
destruction is included in both versions. Lua can randomize table iteration
order between processes, changing BTreeMap insertion costs. Each old/new pair
shares the same Lua table; allocation ranges are reported when they vary
between runs. Benchmark units are:

- `options_N_speed_B`: one table sample; N state entries, optional three
  speedmods. The same Lua table is initialized and warmed outside timing;
  Lua GC is stopped during the pair and restarted/collected afterward.
- `mods_KxF_change_B_speed_B`: F samples for two players, K keys per player.
  Batch timing includes output/cache/scratch construction and destruction.
  Changing cases alternate two distinct states. Throughput counts sample pairs.
- `mods_warm_K`: one extension of K existing windows per player, with warmed
  output, cache, and lookup storage. Throughput counts individual mod windows.
- `overlay_NxO_overlap_B_ties_B`: N scheduled samples across O overlays.
  The same input clone, sorting, output construction, and destruction are
  included. Throughput counts scheduled samples (one call for the empty case).

Per timing sample, option sampling uses 4,096 iterations; mod batches use 32
(4,096 for the single-sample cold case); warmed mod extensions use 2,048;
overlay batches use 128 (4 for 4,096 samples, 4,096 for zero/one sample).

Allocation counts come from the existing thread-local Rust global allocator
wrapper. They exclude Lua's C allocator and other threads. Requested/freed byte
totals include reallocations and are cumulative churn, not peak live memory,
RSS, or retained heap size. No claim is made about those unmeasured metrics.

## Interpretation

- Empty player-option sampling: **45.05% fewer cycles**, 0.584 -> 0.321 us/op; allocation calls 3 -> 0, cumulative requested bytes 114 -> 0.
- Two-player, 32-key stable mod compilation over 600 samples: **65.87% fewer cycles**, 9,036.900 -> 3,083.522 us/op; allocation calls 75,611 -> 138, cumulative requested bytes 549,152 -> 18,462.
- Extending 64 warmed mod windows: **65.85% fewer cycles**, 15.103 -> 5.157 us/op; allocation calls 126 -> 0, cumulative requested bytes 886 -> 0.
- An ordered batch of 4,096 overlay schedules on one track: **98.15% fewer cycles**, 30,753.125 -> 568.750 us/op; allocation calls 5 -> 5, cumulative requested bytes 1,507,508 -> 1,507,508.

The reusable mod lookup buffer has an initial allocation cost and may grow
when longer keys are first encountered. The authored-speed 16/32-key benchmark
fixtures incur one additional reallocation, despite much lower total calls and
byte churn. Cases with increased cumulative requested bytes are listed here;
all cases remain in the result tables:

- `mods_1x1_change_false_speed_true`: 892 -> 900 requested bytes/op; 6 -> 7 allocation calls/op.

These results support the tested compilation workloads. The ordered overlay
path principally reduces CPU work; its output and input buffers still allocate.
Overlapping schedules retain the old sorting algorithm, and very small/control
cases can be dominated by fixed overhead and timing variation.

## Results

Positive cycle savings indicate an improvement on this workload and machine.

| Scenario | Old us/op | New us/op | Old cycles/op | New cycles/op | Fewer cycles |
|---|---:|---:|---:|---:|---:|
| options_0_speed_false | 0.5836 | 0.3210 | 1,280.2 | 703.5 | 45.05% |
| options_0_speed_true | 0.8953 | 0.6228 | 1,963.9 | 1,365.9 | 30.45% |
| options_16_speed_false | 6.2575 | 6.0431 | 13,718.8 | 13,251.6 | 3.41% |
| options_16_speed_true | 6.6438 | 6.3715 | 14,558.3 | 13,971.6 | 4.03% |
| options_64_speed_true | 24.9257 | 24.6059 | 54,649.1 | 53,920.6 | 1.33% |
| mods_1x1_change_false_speed_true | 0.4854 | 0.5467 | 1,062.0 | 1,199.5 | -12.95% |
| mods_1x600_change_false_speed_true | 178.5844 | 43.4906 | 391,725.2 | 95,407.1 | 75.64% |
| mods_16x600_change_false_speed_true | 4,105.7938 | 1,350.9000 | 9,001,722.4 | 2,962,502.4 | 67.09% |
| mods_32x600_change_false_speed_true | 9,036.9000 | 3,083.5219 | 19,806,973.6 | 6,760,874.4 | 65.87% |
| mods_16x128_change_true_speed_true | 1,331.1250 | 861.5750 | 2,916,338.7 | 1,887,940.1 | 35.26% |
| mods_32x128_change_true_speed_false | 1,976.4875 | 2,004.8281 | 4,330,117.8 | 4,390,034.3 | -1.38% |
| mods_0x600_change_false_speed_true | 6.9625 | 3.9531 | 15,323.8 | 8,718.2 | 43.11% |
| mods_warm_1 | 0.2910 | 0.0705 | 638.0 | 155.3 | 75.66% |
| mods_warm_16 | 6.7844 | 2.2635 | 14,863.4 | 4,965.4 | 66.59% |
| mods_warm_32 | 15.1028 | 5.1574 | 33,110.4 | 11,307.1 | 65.85% |
| overlay_0x1_overlap_false_ties_false | 0.0301 | 0.0301 | 66.5 | 66.5 | 0.00% |
| overlay_1x1_overlap_false_ties_false | 0.6717 | 0.6794 | 1,472.5 | 1,490.8 | -1.24% |
| overlay_16x1_overlap_false_ties_false | 2.6586 | 2.2336 | 5,845.9 | 4,914.7 | 15.93% |
| overlay_256x1_overlap_false_ties_false | 131.3234 | 28.2914 | 287,841.7 | 62,025.9 | 78.45% |
| overlay_4096x1_overlap_false_ties_false | 30,753.1250 | 568.7500 | 67,380,792.5 | 1,247,692.8 | 98.15% |
| overlay_4096x16_overlap_false_ties_false | 2,605.8250 | 759.1500 | 5,709,414.2 | 1,663,974.5 | 70.86% |
| overlay_256x1_overlap_true_ties_false | 161.2766 | 163.5023 | 353,600.8 | 358,359.5 | -1.35% |
| overlay_256x1_overlap_false_ties_true | 22.9648 | 23.0844 | 50,341.0 | 50,577.6 | -0.47% |

A/R/F means allocation/reallocation/free calls. Byte totals include reallocations.

| Scenario | Old A/R/F | New A/R/F | Old requested/freed B | New requested/freed B | Old units/s | New units/s |
|---|---:|---:|---:|---:|---:|---:|
| options_0_speed_false | 3/0/3 | 0/0/0 | 114/114 | 0/0 | 1,713,449.1 | 3,115,539.7 |
| options_0_speed_true | 7/0/7 | 4/0/4 | 446/446 | 332/332 | 1,116,928.4 | 1,605,644.8 |
| options_16_speed_false | 38/0/38 | 35/0/35 | 1,560/1,560 | 1,446/1,446 | 159,809.0 | 165,477.6 |
| options_16_speed_true | 41/0/41 | 38/0/38 | 1,572/1,572 | 1,458/1,458 | 150,516.3 | 156,948.7 |
| options_64_speed_true | 144/0/144 | 141/0/141 | 5,008/5,008 | 4,894/4,894 | 40,119.3 | 40,640.7 |
| mods_1x1_change_false_speed_true | 6/0/6 | 7/0/7 | 892/892 | 900/900 | 2,059,947.7 | 1,829,143.0 |
| mods_1x600_change_false_speed_true | 2402/0/2402 | 7/0/7 | 12,872/12,872 | 900/900 | 3,359,756.4 | 13,796,076.7 |
| mods_16x600_change_false_speed_true | 38406/3/38406 | 71/4/71 | 277,416/277,416 | 9,091/9,091 | 146,135.0 | 444,148.3 |
| mods_32x600_change_false_speed_true | 75611/4/75611 | 138/5/138 | 549,152/549,152 | 18,462/18,462 | 66,394.4 | 194,582.7 |
| mods_16x128_change_true_speed_true | 8198/10/8198 | 4135/11/4135 | 911,272/911,272 | 882,851/882,851 | 96,159.3 | 148,565.1 |
| mods_32x128_change_true_speed_false | 16129/11/16129 | 16129/11/16129 | 1,809,248/1,809,248 | 1,809,248/1,809,248 | 64,761.4 | 63,845.9 |
| mods_0x600_change_false_speed_true | 0/0/0 | 0/0/0 | 0/0 | 0/0 | 86,175,942.5 | 151,778,656.1 |
| mods_warm_1 | 4/0/4 | 0/0/0 | 20/20 | 0/0 | 6,872,483.2 | 28,385,308.4 |
| mods_warm_16 | 64/0/64 | 0/0/0 | 448/448 | 0/0 | 4,716,686.5 | 14,137,544.2 |
| mods_warm_32 | 126/0/126 | 0/0/0 | 886/886 | 0/0 | 4,237,629.5 | 12,409,300.9 |
| overlay_0x1_overlap_false_ties_false | 0/0/0 | 0/0/0 | 0/0 | 0/0 | 33,192,868.7 | 33,192,868.7 |
| overlay_1x1_overlap_false_ties_false | 4/1/4 | 4/1/4 | 556/556 | 556/556 | 1,488,804.9 | 1,471,898.8 |
| overlay_16x1_overlap_false_ties_false | 4/4/4 | 4/4/4 | 4,148/4,148 | 4,148/4,148 | 6,018,219.2 | 7,163,343.8 |
| overlay_256x1_overlap_false_ties_false | 5/8/5 | 5/8/5 | 94,388/94,388 | 94,388/94,388 | 1,949,385.5 | 9,048,684.2 |
| overlay_4096x1_overlap_false_ties_false | 5/12/5 | 5/12/5 | 1,507,508/1,507,508 | 1,507,508/1,507,508 | 133,189.7 | 7,201,758.2 |
| overlay_4096x16_overlap_false_ties_false | 23/145/23 | 23/145/23 | 1,999,996/1,999,996 | 1,999,996/1,999,996 | 1,571,863.0 | 5,395,508.1 |
| overlay_256x1_overlap_true_ties_false | 136/8/136 | 136/8/136 | 907,636/907,636 | 907,636/907,636 | 1,587,335.4 | 1,565,726.8 |
| overlay_256x1_overlap_false_ties_true | 5/2/5 | 5/2/5 | 62,132/62,132 | 62,132/62,132 | 11,147,474.1 | 11,089,752.3 |

Per-run paired cycle savings for representative productive cases:

- `options_0_speed_false`: 45.05%, 43.43%, 45.58%.
- `mods_32x600_change_false_speed_true`: 65.51%, 65.93%, 66.05%.
- `mods_warm_32`: 66.09%, 60.33%, 66.33%.
- `overlay_4096x1_overlap_false_ties_false`: 97.96%, 98.15%, 98.26%.

Cases with higher median cycle counts are reported explicitly:

- `mods_1x1_change_false_speed_true`: 12.95% more cycles; elapsed 485.4 to 546.7 ns/op.
- `mods_32x128_change_true_speed_false`: 1.38% more cycles; elapsed 1976487.5 to 2004828.1 ns/op.
- `overlay_1x1_overlap_false_ties_false`: 1.24% more cycles; elapsed 671.7 to 679.4 ns/op.
- `overlay_256x1_overlap_true_ties_false`: 1.35% more cycles; elapsed 161276.6 to 163502.3 ns/op.
- `overlay_256x1_overlap_false_ties_true`: 0.47% more cycles; elapsed 22964.8 to 23084.4 ns/op.

Allocation variation between runs (table entries report medians):

- `options_64_speed_true/old`: 142..144 allocations, 4368..5008 requested bytes/op. Lua's randomized table iteration order changes BTreeMap node counts; old/new share the same table within each run. Every run still removes exactly three allocations and 114 requested bytes per option sample.
- `options_64_speed_true/new`: 139..141 allocations, 4254..4894 requested bytes/op. Lua's randomized table iteration order changes BTreeMap node counts; old/new share the same table within each run. Every run still removes exactly three allocations and 114 requested bytes per option sample.

## Reproduction

```powershell
cargo test -p deadsync-song-lua --locked
cargo test -p deadsync-song-lua --lib --release --locked
cargo test -p deadsync-song-lua --lib --release --locked update_timeline_bench -- --ignored --test-threads=1 --nocapture
$env:DEADSYNC_PERF_REVERSE = '1'
cargo test -p deadsync-song-lua --lib --release --locked update_timeline_bench -- --ignored --test-threads=1 --nocapture
Remove-Item Env:DEADSYNC_PERF_REVERSE
cargo test -p deadsync-song-lua --lib --release --locked update_timeline_bench -- --ignored --test-threads=1 --nocapture
```

The patch version changes exactly `0.5.1169 -> 0.5.1170`. `Cargo.lock`
updates only the three packages inheriting the workspace version.
