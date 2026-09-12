# Overlay capture storage - 0.5.1172

Parent: `91d50c0d3` (`0.5.1171`). This pass applies the local
`rust-performance.md` guidance on measuring hot paths (M-HOTPATH), reusing
storage (M-MEM-REUSE), and avoiding repeated work (M-THROUGHPUT).

## Three changes

1. **Borrow scheduled values in a fixed target lookup.** Scheduled overlay
   capture used a fresh HashMap for every actor batch, cloning each value into
   the map even though the incoming updates remain available. A stack array of
   borrowed values now provides the last write for each target. The first write
   still takes its starting value from the actor state; later writes use the
   preceding value for that target. Each actor batch starts with an empty lookup.
   The array covers the 77 current target discriminants through `StretchRect`
   and occupies 616 bytes on x86-64. Empty batches return before initializing it.
   Emitted values and easing strings retain their original ownership.
2. **Reuse broadcast names during stateful capture.** Existing broadcast entries
   are found by borrowed string lookup. Owned keys are created only when a name
   first enters each of the two capture maps. This removes two name allocations
   per subsequent write and the unnecessary name clone for unregistered actors.
   Actor-target sets, write order, metadata, and Arc ownership remain intact.
3. **Retain unique actor indices during message replay.** Replay owns reusable
   result and membership buffers, allocated lazily on the first matching command.
   Each actor index enters the result once per advance, so repeated messages no
   longer build and sort a large duplicate list. Every message still executes;
   only result collection changes. The returned indices remain sorted for
   binary search. The compiler borrows this private result until the next
   advance. Flags are cleared for prior results, including partial results left
   by a Lua error, so retry behavior remains unchanged.

These improve song-Lua compilation. They do not establish gains in whole-song
load time or gameplay frame rate. No dependencies or production unsafe code
were added. Replay retains its high-water result capacity and a byte per actor
for membership until compilation ends; cold calls can allocate more than the
parent because of that membership buffer. Lua activity, new broadcast keys,
output growth, and owned easing strings can still allocate.

## Behavior and validation

Two test-only baseline modules freeze the parent capture, message replay, and
stateful-write algorithms. Source audits allow formatting and test visibility;
the stateful method becomes a free function with `self` renamed to `capture`.
A scheduled-lookup oracle extracts the unchanged loop from the parent capture
function. The replay-index microbenchmark uses the original push/sort/dedup
statements, fed the actor indices that the replay loop would collect. Shared
Lua state, timing, and track helpers are unchanged from the parent.

Nine new tests cover:

- All 77 targets, repeated and interleaved writes, missing actor states,
  prefixed output buffers, actor-boundary resets, and exact float bits for
  schedule times, metadata, scalar/vector values, NaNs, infinities, and signed
  zero. Shared vertex colors retain their Arc identity.
- Broadcast switches and revisits, empty names, inactive capture, unregistered
  actors, multiple actors/targets, write order, and owned easing metadata.
- Replay ordering, equal beats and epsilon boundaries, rewinds, missing
  messages, short state arrays, duplicate command names, distinct actor command
  sets, and partial Lua errors followed by retry. A real capture comparison
  spans multiple actors and tween completions with message precedence intact.
- Allocation-free warmed scheduled lookup with preallocated output, repeated
  stateful writes with retained maps/output, and replay-index collection.
  Generated index tests cover up to 1,024 actors and repeated batches.

Validation commands and outcomes:

- `cargo test -p deadsync-song-lua --lib storage --offline`: **12 passed**, two
  manual benchmarks ignored. This filter includes all nine new tests and three
  existing tests. All nine new tests also pass in full debug and release runs.
- `cargo test -p deadsync-song-lua --lib --locked` and the same command with
  `--release`: **424 passed, five failed, 11 ignored** in each profile.
- The full-suite log recorded for the parent `0.5.1171` during the prior pass
  has **415 passed, the same five failed, nine ignored**. Failing test names and
  assertion text match both current profiles. This comparison uses the recorded
  parent run, rather than a fresh parent build. The existing failures are
  `compile_song_lua_extracts_actorproxy_targets`,
  `compile_song_lua_layers_share_init_globals_and_actor_refs`,
  `compile_song_lua_resolves_local_and_hidden_screen_proxy_targets`,
  `compile_song_lua_runs_cmd_queuecommand_builders`, and
  `compile_song_lua_supports_notefield_column_api`.
- `cargo check --all-targets --offline`: passed.
- `cargo clippy -p deadsync-song-lua --all-targets --locked -- -D clippy::perf`:
  passed. Nonperformance warnings remain, including argument-count warnings for
  the frozen baseline functions.
- Three paired release runs: all 25 scenarios completed and output comparisons
  passed. Allocation counts are identical across the three runs.
- Parent-source audit, exact patch/lockfile audit, selective rustfmt checks,
  and `git diff --check`: passed.

## Measurement method and limits

Windows x86-64, Intel Xeon E5-2696 v4 @ 2.20 GHz, Rust 1.98.0
(`88d9e12ae`, LLVM 22.1.8), release optimization with full LTO. The existing
`tests/support/perf.rs` helper measures seven timing samples after three warmups,
with allocation counting in a separate operation. Three isolated executable
runs alternate old/new order (old first, new first, old first). No Cargo build
runs during measurement. Tables use medians of the three per-run medians.

Windows `QueryThreadCycleTime` reports calling-thread cycles, not retired
instructions. The thread-local Rust allocator wrapper counts allocation,
reallocation, free calls, and cumulative requested/freed bytes. It excludes
Lua's C allocator and other threads. These byte totals are not peak live memory,
retained capacity, or RSS.

Fixtures, incoming values, actor tables, and messages are prepared outside
timing. Each pair checks its outputs against the parent implementation.
Benchmark units and ownership boundaries:

- `scheduled_N_targets_D_colors_C`: N scheduled writes over D targets. The
  colors cases use shared Arc values. Both output buffers are preallocated,
  cleared at the start of each operation, and retained across operations.
  Timing includes timestamp conversion and value copies/clones; it isolates
  the actor-batch conversion used by capture. Throughput counts writes.
- `stateful_N_name_L_cold_C`: N writes under an L-byte broadcast name. Warm
  cases clear only the retained output vectors. Cold cases clear both capture
  maps, including destruction of the prior entries, before recording the next
  batch. Inactive and unknown-actor controls perform a single attempted write.
  The Lua object and actor registration map persist. Throughput counts writes
  or attempted writes for the controls.
- `replay_indices_AxE`: E complete actor sweeps, in actor-index order. The old
  result is built, sorted, deduplicated, and destroyed per operation. The new
  result and flags are warmed and retained across operations. This isolates
  result collection; throughput counts input actor indices, not Lua commands.
- `replay_lua_AxE`: a cold replay object processes E simultaneous messages on
  A retained Lua actors. Both versions include constructor buffers, actual Lua
  state writes/reads, result collection, and replay/result destruction. Rust
  states reset before each operation. These cases show the broader cost and
  the tradeoff of a cold membership buffer. Throughput counts actor-message
  pairs, or one replay operation when there are no actors.

Timing samples use 512 scheduled/stateful operations, 256 index batches, or
eight Lua replay operations. Empty-input throughput uses one operation as its
unit. Output ownership is included as described above; retained buffer capacity
is not presented as zero memory usage.

Representative measured improvements:

- **128 scheduled writes over 16 targets: 65.49% fewer thread cycles**, 2.90x throughput; requested bytes fall from 2,044 to 0 per operation with warmed output/scratch.
- **128 stateful writes with a 256-byte name: 63.98% fewer thread cycles**, 2.77x throughput; requested bytes fall from 65,536 to 0 per operation with warmed output/scratch.
- **Replay result collection for 128 actors and 16 messages: 91.75% fewer thread cycles**, 12.14x throughput; requested bytes fall from 32,736 to 0 per operation with warmed output/scratch.

These zero-churn results exclude fixture creation and retained buffer capacity.
The replay-index result isolates collection, so it is not a whole-replay
speedup. The cold Lua replay and cold stateful capture measurements below
include their broader costs. All cases with higher median cycle counts
are listed explicitly; small differences do not establish statistical
significance or gains in unmeasured workloads.

The cold Lua replay fixtures do not show a net CPU gain: four actors with one
message use 2.39% more median cycles, and four actors with 16 messages use
0.27% more. The single-message case adds one 8-byte membership allocation;
the 16-message case removes four result reallocations and reduces requested
bytes by 952. These results limit the 91.75% collection-only improvement above.
Inactive stateful capture remains allocation-free but measures 6.1 -> 8.0 ns
per attempted write (23.90% more cycles). Cold 128-write stateful capture adds
one output-vector reallocation while removing 254 allocation/free pairs;
its total requested bytes and measured CPU cost both decrease.

## Results

Positive cycle savings indicate an improvement on this workload and machine.

| Scenario | Old us/op | New us/op | Old cycles/op | New cycles/op | Fewer cycles |
|---|---:|---:|---:|---:|---:|
| stateful_1_name_8_cold_false | 0.1873 | 0.0672 | 413.7 | 149.6 | 63.84% |
| stateful_16_name_8_cold_false | 2.8867 | 0.9340 | 6,329.9 | 2,052.2 | 67.58% |
| stateful_128_name_8_cold_false | 22.3393 | 7.4604 | 48,972.5 | 16,366.5 | 66.58% |
| stateful_128_name_256_cold_false | 26.2291 | 9.4543 | 57,505.1 | 20,714.9 | 63.98% |
| stateful_1_name_8_cold_true | 0.5875 | 0.4707 | 1,291.7 | 1,036.2 | 19.78% |
| stateful_128_name_8_cold_true | 24.0336 | 9.2105 | 52,681.7 | 20,203.0 | 61.65% |
| stateful_inactive | 0.0061 | 0.0080 | 15.9 | 19.7 | -23.90% |
| stateful_unknown | 0.0805 | 0.0293 | 179.2 | 66.9 | 62.67% |
| scheduled_0_targets_1_colors_false | 0.0182 | 0.0096 | 42.0 | 23.2 | 44.76% |
| scheduled_1_targets_1_colors_false | 0.2477 | 0.0412 | 546.6 | 92.6 | 83.06% |
| scheduled_16_targets_1_colors_false | 1.0701 | 0.4002 | 2,351.1 | 881.0 | 62.53% |
| scheduled_128_targets_1_colors_false | 8.1758 | 3.2541 | 17,936.0 | 7,139.3 | 60.20% |
| scheduled_128_targets_16_colors_false | 8.9959 | 3.1031 | 19,731.4 | 6,809.2 | 65.49% |
| scheduled_512_targets_77_colors_false | 36.2395 | 14.2736 | 79,430.7 | 31,284.3 | 60.61% |
| scheduled_128_targets_1_colors_true | 11.8203 | 6.0150 | 25,928.4 | 13,188.9 | 49.13% |
| scheduled_128_targets_16_colors_true | 12.7154 | 5.8566 | 27,888.1 | 12,845.5 | 53.94% |
| replay_indices_0x0 | 0.0070 | 0.0039 | 19.7 | 12.9 | 34.52% |
| replay_indices_1x1 | 0.0633 | 0.0059 | 143.2 | 18.0 | 87.43% |
| replay_indices_16x1 | 0.3422 | 0.0492 | 756.2 | 113.2 | 85.03% |
| replay_indices_128x1 | 0.6727 | 0.3594 | 1,481.6 | 794.0 | 46.41% |
| replay_indices_128x16 | 21.4098 | 1.7637 | 46,966.1 | 3,876.4 | 91.75% |
| replay_indices_1024x16 | 225.5258 | 14.1777 | 494,295.1 | 31,094.4 | 93.71% |
| replay_lua_0x16 | 0.1625 | 0.1250 | 521.2 | 439.0 | 15.77% |
| replay_lua_4x1 | 91.2625 | 93.1125 | 199,827.2 | 204,601.4 | -2.39% |
| replay_lua_4x16 | 1,452.3000 | 1,457.9875 | 3,181,378.1 | 3,190,020.6 | -0.27% |

A/R/F means allocation/reallocation/free calls. Bytes include reallocations and common input/output ownership costs.

| Scenario | Old A/R/F | New A/R/F | Old requested/freed B | New requested/freed B | Old units/s | New units/s |
|---|---:|---:|---:|---:|---:|---:|
| stateful_1_name_8_cold_false | 2/0/2 | 0/0/0 | 16/16 | 0/0 | 5,338,894.7 | 14,883,720.9 |
| stateful_16_name_8_cold_false | 32/0/32 | 0/0/0 | 256/256 | 0/0 | 5,542,625.2 | 17,130,907.6 |
| stateful_128_name_8_cold_false | 256/0/256 | 0/0/0 | 2,048/2,048 | 0/0 | 5,729,823.3 | 17,157,368.4 |
| stateful_128_name_256_cold_false | 256/0/256 | 0/0/0 | 65,536/65,536 | 0/0 | 4,880,075.7 | 13,538,817.5 |
| stateful_1_name_8_cold_true | 7/0/7 | 7/0/7 | 1,816/1,816 | 1,576/1,576 | 1,702,127.7 | 2,124,481.3 |
| stateful_128_name_8_cold_true | 261/5/261 | 7/6/7 | 23,688/23,688 | 21,736/21,736 | 5,325,878.5 | 13,897,111.8 |
| stateful_inactive | 0/0/0 | 0/0/0 | 0/0 | 0/0 | 165,161,290.3 | 124,878,048.8 |
| stateful_unknown | 1/0/1 | 0/0/0 | 16/16 | 0/0 | 12,427,184.5 | 34,133,333.3 |
| scheduled_0_targets_1_colors_false | 0/0/0 | 0/0/0 | 0/0 | 0/0 | 55,053,763.4 | 104,489,795.9 |
| scheduled_1_targets_1_colors_false | 1/0/1 | 0/0/0 | 148/148 | 0/0 | 4,037,854.9 | 24,265,402.8 |
| scheduled_16_targets_1_colors_false | 1/0/1 | 0/0/0 | 148/148 | 0/0 | 14,951,633.5 | 39,980,478.3 |
| scheduled_128_targets_1_colors_false | 1/0/1 | 0/0/0 | 148/148 | 0/0 | 15,655,996.2 | 39,334,973.9 |
| scheduled_128_targets_16_colors_false | 4/0/4 | 0/0/0 | 2,044/2,044 | 0/0 | 14,228,706.7 | 41,248,741.2 |
| scheduled_512_targets_77_colors_false | 6/0/6 | 0/0/0 | 8,412/8,412 | 0/0 | 14,128,248.5 | 35,870,335.7 |
| scheduled_128_targets_1_colors_true | 1/0/1 | 0/0/0 | 148/148 | 0/0 | 10,828,816.9 | 21,279,994.8 |
| scheduled_128_targets_16_colors_true | 4/0/4 | 0/0/0 | 2,044/2,044 | 0/0 | 10,066,510.0 | 21,855,532.6 |
| replay_indices_0x0 | 0/0/0 | 0/0/0 | 0/0 | 0/0 | 142,222,222.2 | 256,000,000.0 |
| replay_indices_1x1 | 1/0/1 | 0/0/0 | 32/32 | 0/0 | 15,802,469.1 | 170,666,666.7 |
| replay_indices_16x1 | 1/2/1 | 0/0/0 | 224/224 | 0/0 | 46,757,990.9 | 325,079,365.1 |
| replay_indices_128x1 | 1/5/1 | 0/0/0 | 2,016/2,016 | 0/0 | 190,290,360.0 | 356,173,913.0 |
| replay_indices_128x16 | 1/9/1 | 0/0/0 | 32,736/32,736 | 0/0 | 95,657,282.6 | 1,161,213,732.0 |
| replay_indices_1024x16 | 1/12/1 | 0/0/0 | 262,112/262,112 | 0/0 | 72,648,013.5 | 1,155,614,823.0 |
| replay_lua_0x16 | 1/0/1 | 1/0/1 | 128/128 | 128/128 | 6,153,846.2 | 8,000,000.0 |
| replay_lua_4x1 | 51/32/3 | 52/32/4 | 5,640/2,888 | 5,648/2,896 | 43,829.6 | 42,958.8 |
| replay_lua_4x16 | 829/516/545 | 830/512/546 | 62,586/46,298 | 61,634/45,346 | 44,068.0 | 43,896.1 |

Per-run paired cycle savings for representative cases:

- `scheduled_128_targets_16_colors_false`: 65.33%, 67.39%, 65.73%.
- `stateful_128_name_256_cold_false`: 64.32%, 60.66%, 64.12%.
- `replay_indices_128x16`: 91.79%, 91.72%, 91.69%.

Cases with higher median cycle counts:

- `stateful_inactive`: 23.90% more cycles; elapsed 6.1 -> 8.0 ns/op.
- `replay_lua_4x1`: 2.39% more cycles; elapsed 91262.5 -> 93112.5 ns/op.
- `replay_lua_4x16`: 0.27% more cycles; elapsed 1452300.0 -> 1457987.5 ns/op.

## Reproduction

```powershell
cargo test -p deadsync-song-lua --lib storage --locked
cargo test -p deadsync-song-lua --lib --locked
cargo test -p deadsync-song-lua --lib --release --locked
cargo test -p deadsync-song-lua --lib --release --locked overlay_storage_bench -- --ignored --test-threads=1 --nocapture
$env:DEADSYNC_PERF_REVERSE = '1'
cargo test -p deadsync-song-lua --lib --release --locked overlay_storage_bench -- --ignored --test-threads=1 --nocapture
Remove-Item Env:DEADSYNC_PERF_REVERSE
cargo test -p deadsync-song-lua --lib --release --locked overlay_storage_bench -- --ignored --test-threads=1 --nocapture
```

The version changes exactly `0.5.1171 -> 0.5.1172`. `Cargo.lock` changes
only the three packages inheriting the workspace version.
