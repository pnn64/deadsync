# Command names, global snapshots, and auxiliary lookup - 0.5.1182

Parent: `02df6cae276557eb6ddc7db182c93dd5cb24704f` (`0.5.1181`).
This pass follows `rust-performance.md`: measure CPU and allocator traffic
(M-HOTPATH), reserve known string sizes (M-INITIAL-CAPACITY), and avoid temporary
ownership and repeated work (M-MEM-REUSE).

## Three changes

1. **Command-key construction.** Queued commands, broadcasts, and most installed
   actor command methods use `ActorCommandName`. Keys up to 128 UTF-8 bytes, including
   the suffix, live in a stack buffer. Longer keys use one exact-capacity String.
   This removes `format!` allocation/reallocation from common names and avoids
   growth for long names. `playcommand` keeps a compact owned String across
   recursive dispatch, using the same exact-capacity builder to eliminate
   growth; this avoids carrying the larger inline buffer through that path.
   Lua argument conversion, key spelling, lookup
   metamethods, dispatch order, recurrence, and scope restoration are unchanged.
   The helper uses safe UTF-8 validation for inline keys and ordinary String
   access for long keys. Conversion into Lua still creates the same Lua string;
   Lua storage and owned input names can still allocate.
2. **Scalar global snapshots.** `snapshot_scalar_globals` uses `Table::for_each`
   so ignored global functions, tables, and other non-scalars no longer incur
   iterator cursor-key handle allocations. Selected scalar keys remain owned
   Rust strings and selected values preserve their original identity and bits.
   Traversal remains raw, ordered as Lua `next`, and ignores `__pairs` and
   inherited fields. Invalid UTF-8 string keys still error only for selected
   scalar values, just as before.
3. **Previous auxiliary-state lookup.** Large function-action captures build a
   temporary actor-pointer-to-snapshot-index map once, shared by overlay and
   tracked-actor auxiliary reads. This replaces repeated linear searches of the
   snapshot list. The index is used only with at least 32 snapshots and 32
   queries; smaller or sparse batches keep the allocation-free linear path.
   That path also reads the queried actor's pointer once per lookup. The map
   retains the first snapshot for duplicate actors. State-key searches and value
   conversion remain lazy and retain first-key behavior, defaults, error
   suppression, numeric conversion, and NaN/signed-zero behavior.

These changes use existing dependencies and safe Rust/mlua APIs. No public API,
global cache, dependency version, or production unsafe code changed. The index
adds one transient allocation to large batches in exchange for less CPU work;
it is not a claim that all auxiliary capture or command execution allocates
nothing. Runtime costs outside these measured host operations remain.

## Behavior and validation

Ten parent function bodies are frozen in test-only modules. The command oracle
includes the four-function recursive drain/dispatch chain and the original
installed methods and broadcaster. The scalar-snapshot oracle also includes
its complete preserving-command caller. The auxiliary oracle includes the
complete function-action caller and the old lookup. Other helpers are shared
by each pair. The two formatting microbenchmarks use the exact original
`format!` expressions. A source audit checks these bodies/expressions and
reconstructs the parent from the six changed production functions, two helper
types, and test-module declarations.

Twelve new regression tests cover:

- Command and message suffixes; empty, Unicode, NUL-containing, boundary, and
  long names; add/remove/GetCommand; propagation to children and leaves;
  recurring queue intervals; invalid/numeric/missing arguments; lookup and
  callback errors; long broadcast keys, Judgment parameters, and active-scope
  restoration. Short key construction has zero churn; long construction has
  exactly one allocation with a byte budget equal to the resulting key length.
- Global snapshot order, scalar values and non-finite numeric bits, binary
  string values, mixed key types, metatable traps, invalid-key error filtering,
  full command capture and scalar restoration. A snapshot of 512 non-scalar
  globals has zero churn.
- Auxiliary lookup at both sides of the 32-entry threshold, duplicate actors
  and duplicate state keys, missing snapshots, invalid/numeric/string values,
  NaN/infinity/signed zero, getter errors, lookup order with mutation, and full
  overlay/tracked capture with restoration after success or failure. Small
  numeric batches have zero lookup churn; the large numeric lookup index needs
  one allocation with no reallocation.

Validation results:

- Targeted tests: **12 passed**, 3 manual benchmarks ignored.
- Full song-Lua debug and release suites: **525 passed, 5 failed, 39 ignored**
  in each profile. The fresh parent debug run had **513 passed, the same 5
  failures, and 36 ignored**. Both new profiles match the parent failure names
  and assertion text. The parent was not separately rebuilt in release mode.
- Song-Lua Clippy with `-D clippy::perf` and root `cargo check --all-targets
  --offline`: passed. Existing Clippy diagnostics match the prior pass after
  source-coordinate normalization.
- All 35 paired cases passed in three serial release runs. Source/version and
  staged-whitespace audits passed. The workspace patch increases exactly once
  to 0.5.1182; all three workspace-versioned lockfile entries follow, with no
  dependency updates.

The five existing failures remain:

1. `compile_song_lua_extracts_actorproxy_targets`: initial visibility assertion.
2. `compile_song_lua_layers_share_init_globals_and_actor_refs`: 0.0 versus 123.0.
3. `compile_song_lua_resolves_local_and_hidden_screen_proxy_targets`: initial
   visibility assertion.
4. `compile_song_lua_runs_cmd_queuecommand_builders`: initial visibility assertion.
5. `compile_song_lua_supports_notefield_column_api`: `-96:-135` versus `-96:-125`.

These results establish no new failures in the exercised behavior; the full
suite is not green.

## Measurement method and limits

Windows x86_64, Intel Xeon E5-2696 v4 @ 2.20 GHz, rustc 1.98.0
(`88d9e12ae`, LLVM 22.1.8). The release profile uses opt-level 3 and full LTO.
Old and new implementations run in the same release test executable. All 35
paired cases run serially three times after compiler/linker processes finish;
the middle run reverses pair order. Each case has three warmups and seven
timing samples. Tables show the median of each version's three run medians;
raw logs retain within-run sample ranges.

Windows `QueryThreadCycleTime` measures this thread's CPU cycles, not retired
instructions. Elapsed time, cycles, and throughput are timed separately from
one allocation-counted operation. The shared thread-local allocator counts
Rust and mlua allocation/reallocation/free calls and requested/freed bytes.
Bytes are cumulative allocator traffic, not peak live memory or RSS. No process
memory or whole-song/gameplay improvement is inferred from these microbenchmarks.

Key-construction cases include construction and destruction, with inputs made
outside timing, 512 operations per sample, and one key as the throughput unit.
Those isolated cases exclude borrowing the completed key as `&str` or converting
it into Lua; installed-method/queue cases include inline UTF-8 validation and
Lua key conversion.
Installed-method cases include the Lua-to-Rust call, name conversion, key
construction/lookup, and dispatch, but exclude installation; they use 256
operations per sample. Queue measurements include the same refill step on both
sides and drain 64 distinct missing-command names, 64 operations per sample.

Global snapshot microbenchmarks use 256 operations per sample and report fields
inspected per second; their input globals are controlled fixtures with empty,
all-scalar, all-non-scalar, and mixed cases. Full preserving-command captures
use ordinary Lua globals plus injected non-scalars, 64 operations per sample,
and one captured command per throughput unit.

Auxiliary microbenchmarks include index construction, every lookup, owned
result collection, and index/result destruction. Queries traverse actors in
reverse order to cover the complete range of snapshot-search distances. They
use 256 operations per sample below 64 snapshots and 32 otherwise; throughput
counts queried actors. Sparse controls include one and eight queries against
512 snapshots. Full auxiliary captures include scope snapshots, the actor
writes, lookup, block collection, and state/runtime restoration, with equal
overlay/tracked partitions (eight operations per sample). Their unit is a
captured actor. Function-environment snapshotting is disabled equally in these
two full-capture fixtures; other host capture work is included.

Lua VMs, actors, methods, names, and snapshot inputs are created outside timing.
Rust temporary results are dropped inside each measured operation. Each Lua
fixture is collected before its benchmark group, then GC is stopped until VM
teardown, so deferred Lua GC/teardown and cold stack/table growth are excluded.
Allocation tests distinguish helper costs from entire dispatch/capture costs.

## Interpretation

The 64-command queue uses **9.29% fewer cycles** in the three-run aggregate.
Allocation calls fall from 128 to 64, reallocations from 64 to zero, and
requested/freed bytes from 1,846 to 310 per refill-and-drain operation. Short
`GetCommand` calls use 18.69% fewer cycles and remove one allocation and one
reallocation. `playcommand` retains seven allocation calls but removes its
reallocation and reduces requested bytes from 326 to 317; its aggregate cycle
gain is 5.83%, with variable per-run signs.

A snapshot with 256 ignored globals and eight scalars uses **43.50% fewer
cycles**, reduces allocation calls from 273 to 17, and reduces requested/freed
bytes from 5,056 to 960. The full preserving-command caller with 256 injected
non-scalar globals uses 21.02% fewer cycles and 314 allocations instead of 603.
These snapshot/caller cases improved in every primary paired run.

For 256 actors, auxiliary lookup uses **91.51% fewer cycles**, including index
construction and destruction. This deliberately trades one additional
allocation and 8,720 requested/freed bytes for less search work. The full
128-actor capture uses 12.65% fewer cycles, with one additional allocation and
4,368 additional requested/freed bytes. Its cycle savings were positive in all
three primary and all six additional runs. The index remains local to a capture;
these byte costs are not retained-cache growth.

The controls also expose costs and uncertainty:

- Empty auxiliary batches are slower: 14.8 -> 19.5 ns/op (26.68% more cycles)
  in the primary aggregate. The six extra runs confirm a small fixed setup
  cost; neither implementation allocates in this case.
- One-actor auxiliary lookup has unchanged allocator traffic and a higher
  primary median (182.8 -> 207.4 ns/op). Additional paired runs show a much
  smaller median slowdown of 2.37%, with mixed signs. No speedup is claimed
  for empty or singleton auxiliary work.
- The eight-actor full auxiliary capture was 5.34% higher in cycles in the
  primary aggregate, while the six extra runs were all lower (median paired
  saving 8.84%). This caller's small-workload timing remains inconclusive;
  allocation counts and bytes are unchanged. The isolated eight-actor lookup
  improved in all six extra runs.
- Short installed command calls vary between runs. The six-run paired median
  savings are 17.51% for `GetCommand`, 5.01% for `playcommand`, and 0.22% for
  immediate `queuecommand`, with some negative runs. The 64-command drain
  improves in every extra run (median paired saving 10.71%). Thus the report
  claims reliable batch savings and reduced allocator traffic, rather than
  promising a fixed speedup for every individual method call.

All primary cases and all six additional runs of both affected benchmark
groups are reported below. The extra runs use the final code; preliminary
measurements of the earlier implementation remain only in ignored local logs.

## Results

Positive cycle savings indicate an improvement on this workload and machine.

| Scenario | Old us/op | New us/op | Old cycles/op | New cycles/op | Fewer cycles |
|---|---:|---:|---:|---:|---:|
| aux_0_0_fields_0 | 0.0148 | 0.0195 | 38.6 | 48.9 | -26.68% |
| aux_1_1_fields_0 | 0.1828 | 0.2074 | 405.6 | 461.3 | -13.73% |
| aux_8_8_fields_0 | 1.2746 | 1.2043 | 2,803.8 | 2,649.4 | 5.51% |
| aux_31_31_fields_0 | 8.5660 | 6.3789 | 18,781.8 | 13,993.1 | 25.50% |
| aux_32_32_fields_0 | 8.1336 | 4.1707 | 17,841.2 | 9,146.1 | 48.74% |
| aux_33_33_fields_0 | 10.1402 | 4.0164 | 22,246.7 | 8,809.2 | 60.40% |
| aux_64_64_fields_0 | 24.3656 | 9.0344 | 53,516.8 | 19,871.6 | 62.87% |
| aux_256_256_fields_0 | 363.6125 | 30.7000 | 793,760.1 | 67,427.7 | 91.51% |
| aux_512_512_fields_0 | 1,345.3125 | 70.5438 | 2,943,913.4 | 154,761.2 | 94.74% |
| aux_512_1_fields_0 | 4.6875 | 2.3250 | 10,261.6 | 5,144.5 | 49.87% |
| aux_512_8_fields_0 | 36.1125 | 18.0531 | 79,191.5 | 39,462.0 | 50.17% |
| aux_256_256_fields_16 | 361.2250 | 48.0094 | 791,373.0 | 105,305.1 | 86.69% |
| aux_capture_8 | 74.3750 | 77.9875 | 162,731.9 | 171,429.5 | -5.34% |
| aux_capture_128 | 1,303.9250 | 1,138.7375 | 2,857,368.9 | 2,495,769.9 | 12.65% |
| name_0_Command | 0.0939 | 0.0145 | 208.4 | 34.7 | 83.35% |
| name_8_Command | 0.1768 | 0.0152 | 390.6 | 36.0 | 90.78% |
| name_121_Command | 0.1861 | 0.0148 | 411.1 | 35.2 | 91.44% |
| name_122_Command | 0.1818 | 0.0689 | 401.3 | 153.9 | 61.65% |
| name_4096_Command | 0.6064 | 0.2021 | 1,334.6 | 446.7 | 66.53% |
| name_8_MessageCommand | 0.1998 | 0.0141 | 429.6 | 33.4 | 92.23% |
| name_114_MessageCommand | 0.2057 | 0.0158 | 454.0 | 37.3 | 91.78% |
| name_115_MessageCommand | 0.1873 | 0.0709 | 413.7 | 158.2 | 61.76% |
| method_GetCommand_8 | 0.8547 | 0.6941 | 1,881.2 | 1,529.6 | 18.69% |
| method_GetCommand_200 | 1.6281 | 1.5355 | 3,582.3 | 3,378.2 | 5.70% |
| method_playcommand_8 | 3.1926 | 3.0121 | 7,015.4 | 6,606.4 | 5.83% |
| method_queuecommand_8 | 5.0109 | 4.6414 | 10,983.6 | 10,031.0 | 8.67% |
| named_queue_64 | 115.1609 | 104.7516 | 252,359.8 | 228,914.5 | 9.29% |
| global_snapshot_0_0 | 0.1523 | 0.1199 | 340.4 | 268.4 | 21.15% |
| global_snapshot_0_64 | 22.1605 | 18.9578 | 48,578.9 | 41,519.8 | 14.53% |
| global_snapshot_64_0 | 15.3473 | 8.2992 | 33,669.2 | 18,211.6 | 45.91% |
| global_snapshot_256_0 | 63.7383 | 30.7605 | 139,638.9 | 67,465.4 | 51.69% |
| global_snapshot_256_8 | 66.3500 | 37.4516 | 145,306.4 | 82,097.3 | 43.50% |
| global_snapshot_512_32 | 131.5367 | 71.5934 | 288,178.6 | 156,957.1 | 45.53% |
| global_capture_0 | 25.4609 | 25.1391 | 55,605.5 | 55,115.1 | 0.88% |
| global_capture_256 | 156.4500 | 123.5094 | 342,920.7 | 270,828.7 | 21.02% |

A/R/F means allocation/reallocation/free calls. Bytes include reallocations and common input/output ownership costs.

| Scenario | Old A/R/F | New A/R/F | Old requested/freed B | New requested/freed B | Old units/s | New units/s |
|---|---:|---:|---:|---:|---:|---:|
| aux_0_0_fields_0 | 0/0/0 | 0/0/0 | 0/0 | 0/0 | 67,368,421.1 | 51,200,000.0 |
| aux_1_1_fields_0 | 1/0/1 | 1/0/1 | 8/8 | 8/8 | 5,470,085.5 | 4,821,092.3 |
| aux_8_8_fields_0 | 1/0/1 | 1/0/1 | 64/64 | 64/64 | 6,276,432.7 | 6,642,880.3 |
| aux_31_31_fields_0 | 1/0/1 | 1/0/1 | 248/248 | 248/248 | 3,618,952.1 | 4,859,767.3 |
| aux_32_32_fields_0 | 1/0/1 | 2/0/2 | 256/256 | 1,360/1,360 | 3,934,300.3 | 7,672,567.2 |
| aux_33_33_fields_0 | 1/0/1 | 2/0/2 | 264/264 | 1,368/1,368 | 3,254,362.6 | 8,216,300.3 |
| aux_64_64_fields_0 | 1/0/1 | 2/0/2 | 512/512 | 2,704/2,704 | 2,626,651.3 | 7,084,054.0 |
| aux_256_256_fields_0 | 1/0/1 | 2/0/2 | 2,048/2,048 | 10,768/10,768 | 704,046.2 | 8,338,762.2 |
| aux_512_512_fields_0 | 1/0/1 | 2/0/2 | 4,096/4,096 | 21,520/21,520 | 380,580.7 | 7,257,907.3 |
| aux_512_1_fields_0 | 1/0/1 | 1/0/1 | 8/8 | 8/8 | 213,333.3 | 430,107.5 |
| aux_512_8_fields_0 | 1/0/1 | 1/0/1 | 64/64 | 64/64 | 221,529.9 | 443,136.6 |
| aux_256_256_fields_16 | 1/0/1 | 2/0/2 | 2,048/2,048 | 10,768/10,768 | 708,699.6 | 5,332,291.9 |
| aux_capture_8 | 198/14/126 | 198/14/126 | 8,008/4,400 | 8,008/4,400 | 107,563.0 | 102,580.5 |
| aux_capture_128 | 2842/158/1810 | 2843/158/1811 | 127,888/74,360 | 132,256/78,728 | 98,165.2 | 112,405.2 |
| name_0_Command | 1/0/1 | 0/0/0 | 8/8 | 0/0 | 10,644,490.6 | 69,189,189.2 |
| name_8_Command | 1/1/1 | 0/0/0 | 24/24 | 0/0 | 5,657,458.6 | 65,641,025.6 |
| name_121_Command | 1/1/1 | 0/0/0 | 363/363 | 0/0 | 5,372,507.9 | 67,368,421.1 |
| name_122_Command | 1/1/1 | 1/0/1 | 366/366 | 129/129 | 5,499,462.9 | 14,504,249.3 |
| name_4096_Command | 1/1/1 | 1/0/1 | 12,288/12,288 | 4,103/4,103 | 1,648,953.3 | 4,946,859.9 |
| name_8_MessageCommand | 1/1/1 | 0/0/0 | 30/30 | 0/0 | 5,004,887.6 | 71,111,111.1 |
| name_114_MessageCommand | 1/1/1 | 0/0/0 | 342/342 | 0/0 | 4,862,298.2 | 63,209,876.5 |
| name_115_MessageCommand | 1/1/1 | 1/0/1 | 345/345 | 129/129 | 5,338,894.7 | 14,104,683.2 |
| method_GetCommand_8 | 4/1/4 | 3/0/3 | 128/128 | 104/104 | 1,170,018.3 | 1,440,630.3 |
| method_GetCommand_200 | 5/1/4 | 5/0/4 | 1,128/896 | 735/503 | 614,203.5 | 651,233.8 |
| method_playcommand_8 | 7/1/4 | 7/0/4 | 326/128 | 317/119 | 313,226.5 | 331,993.3 |
| method_queuecommand_8 | 8/2/5 | 6/0/3 | 278/80 | 230/32 | 199,563.5 | 215,451.9 |
| named_queue_64 | 128/64/128 | 64/0/64 | 1,846/1,846 | 310/310 | 555,744.0 | 610,969.4 |
| global_snapshot_0_0 | 0/0/0 | 0/0/0 | 0/0 | 0/0 | 6,564,102.6 | 8,338,762.2 |
| global_snapshot_0_64 | 129/4/129 | 129/4/129 | 9,526/9,526 | 9,526/9,526 | 2,888,015.4 | 3,375,916.9 |
| global_snapshot_64_0 | 64/0/64 | 0/0/0 | 1,024/1,024 | 0/0 | 4,170,124.0 | 7,711,569.2 |
| global_snapshot_256_0 | 256/0/256 | 0/0/0 | 4,096/4,096 | 0/0 | 4,016,424.6 | 8,322,348.8 |
| global_snapshot_256_8 | 273/1/273 | 17/1/17 | 5,056/5,056 | 960/960 | 3,978,899.8 | 7,049,105.1 |
| global_snapshot_512_32 | 577/3/577 | 65/3/65 | 12,822/12,822 | 4,630/4,630 | 4,135,727.3 | 7,598,470.1 |
| global_capture_0 | 91/0/84 | 58/0/51 | 2,146/1,724 | 1,618/1,196 | 39,275.9 | 39,778.7 |
| global_capture_256 | 603/0/596 | 314/0/307 | 10,338/9,916 | 5,714/5,292 | 6,391.8 | 8,096.6 |

Per-run paired cycle savings for representative cases:

- `method_GetCommand_8`: 28.38%, 16.20%, 20.69%.
- `named_queue_64`: 6.45%, 5.49%, 9.29%.
- `global_snapshot_256_8`: 43.29%, 43.66%, 45.51%.
- `global_capture_256`: 27.27%, 20.59%, 20.76%.
- `aux_256_256_fields_0`: 89.97%, 91.86%, 90.72%.
- `aux_capture_128`: 9.34%, 19.55%, 7.27%.

Cases with higher median cycle counts:

- `aux_0_0_fields_0`: 26.68% more cycles; elapsed 14.8 -> 19.5 ns/op.
- `aux_1_1_fields_0`: 13.73% more cycles; elapsed 182.8 -> 207.4 ns/op.
- `aux_capture_8`: 5.34% more cycles; elapsed 74375.0 -> 77987.5 ns/op.

## Additional control measurements

Six additional runs of all 27 command-name/auxiliary cases alternate old/new order, using the same final
binary and fixtures. They investigate small/noisy controls; the original three
final-code runs above remain included. Positive percentages mean fewer cycles.

| Scenario | Per-run paired cycle savings | Median paired savings |
|---|---|---:|
| name_0_Command | 82.96%, 86.52%, 82.42%, 83.37%, 89.06%, 83.07% | 83.22% |
| name_8_Command | 90.66%, 90.89%, 90.83%, 90.55%, 89.02%, 90.93% | 90.75% |
| name_121_Command | 93.80%, 91.47%, 92.12%, 94.78%, 93.86%, 91.13% | 92.96% |
| name_122_Command | 59.42%, 61.79%, 61.62%, 62.93%, 64.13%, 51.39% | 61.71% |
| name_4096_Command | 65.91%, 42.65%, 67.76%, 43.11%, 64.49%, 43.41% | 53.95% |
| name_8_MessageCommand | 91.37%, 89.52%, 91.32%, 91.07%, 91.06%, 91.30% | 91.19% |
| name_114_MessageCommand | 93.86%, 94.29%, 94.34%, 91.04%, 94.47%, 94.78% | 94.31% |
| name_115_MessageCommand | 59.27%, 61.18%, 62.80%, 71.04%, 62.45%, 60.71% | 61.82% |
| method_GetCommand_8 | 17.54%, 15.71%, 46.56%, 18.02%, 17.49%, -8.67% | 17.51% |
| method_GetCommand_200 | 38.83%, -8.94%, 8.60%, 6.04%, 20.72%, 11.20% | 9.90% |
| method_playcommand_8 | 12.22%, 11.94%, 3.78%, -12.04%, 6.24%, -12.38% | 5.01% |
| method_queuecommand_8 | 7.77%, 3.19%, -17.02%, -4.94%, 11.72%, -2.75% | 0.22% |
| named_queue_64 | 8.95%, 11.75%, 10.30%, 7.00%, 11.12%, 13.18% | 10.71% |
| aux_0_0_fields_0 | -27.32%, -27.91%, -15.79%, -24.01%, -24.76%, -26.14% | -25.45% |
| aux_1_1_fields_0 | 0.41%, -5.45%, 0.99%, -2.43%, -2.36%, -2.39% | -2.37% |
| aux_8_8_fields_0 | 4.31%, 6.34%, 13.69%, 12.72%, 2.58%, 11.51% | 8.93% |
| aux_31_31_fields_0 | 23.80%, 24.15%, 31.37%, 18.17%, 29.35%, 25.46% | 24.81% |
| aux_32_32_fields_0 | 49.29%, 55.13%, 55.52%, 43.32%, 34.98%, 47.42% | 48.35% |
| aux_33_33_fields_0 | 58.69%, 56.74%, 53.87%, 59.12%, 56.13%, 57.29% | 57.02% |
| aux_64_64_fields_0 | 70.34%, 77.35%, 71.51%, 64.88%, 69.35%, 69.18% | 69.85% |
| aux_256_256_fields_0 | 91.15%, 90.75%, 90.91%, 90.82%, 90.66%, 91.46% | 90.87% |
| aux_512_512_fields_0 | 94.49%, 94.12%, 94.62%, 94.88%, 93.51%, 94.60% | 94.55% |
| aux_512_1_fields_0 | 52.09%, 38.11%, 48.13%, 50.50%, 47.88%, 35.08% | 48.00% |
| aux_512_8_fields_0 | 58.39%, 48.33%, 30.49%, 59.39%, 45.53%, 57.74% | 53.03% |
| aux_256_256_fields_16 | 86.76%, 86.29%, 86.84%, 85.07%, 85.12%, 86.74% | 86.52% |
| aux_capture_8 | 7.07%, 0.33%, 2.26%, 22.04%, 11.12%, 10.62% | 8.84% |
| aux_capture_128 | 8.18%, 11.38%, 13.12%, 5.72%, 6.35%, 12.96% | 9.78% |

## Reproduction

```powershell
cargo test -p deadsync-song-lua --lib lua_command_ --locked
cargo clippy -p deadsync-song-lua --all-targets --locked -- -D clippy::perf
cargo check --all-targets --offline
cargo test -p deadsync-song-lua --lib --locked
cargo test -p deadsync-song-lua --lib --release --locked
cargo test -p deadsync-song-lua --lib --release lua_command_bench --locked -- --ignored --test-threads=1 --nocapture
$env:DEADSYNC_PERF_REVERSE = '1'
cargo test -p deadsync-song-lua --lib --release lua_command_bench --locked -- --ignored --test-threads=1 --nocapture
Remove-Item Env:DEADSYNC_PERF_REVERSE
cargo test -p deadsync-song-lua --lib --release lua_command_bench --locked -- --ignored --test-threads=1 --nocapture
```

Recorded runs invoke the already-built executable directly, with no concurrent
Rust compiler/linker processes. Local logs, extracted metrics, and source audits
are under ignored `target/lua-command-perf/`. Committed test/oracle sources are
`crates/deadsync-song-lua/tests/perf/{command_names,global_snapshot,aux_lookup}*.rs`.
The excluded song, guide, and optimization scripts are absent from this commit.
