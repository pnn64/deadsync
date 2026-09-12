# Lua queues, child removal, and message discovery - 0.5.1181

Parent: `ff42a7005d492d6bf2b732ea8a80bcbfa40654b4` (`0.5.1180`).
This pass applies `rust-performance.md` guidance to measure CPU time and
allocator traffic (M-HOTPATH), remove temporary ownership work, and reuse
existing storage (M-MEM-REUSE).

## Three changes

1. **Queued actor commands.** `drain_actor_command_queue` uses mlua's integer
   `Table::raw_remove` to shift each consumed queue entry inside one protected
   Lua operation; a singleton keeps the direct nil write because it needs no
   shifting or protected-call setup. Previously each remaining item crossed into
   Rust as a `Value` and back into Lua. Head conversion still precedes removal, and removal still
   precedes command dispatch. Callbacks see the same shifted queue and can append,
   replace entries, and drain recursively. Startup deferral and command-local
   cursor restoration are unchanged. Total shifting remains quadratic in queue
   length; this removes the repeated Rust/Lua marshalling cost without changing
   the callback-visible array representation.
2. **Removing all actor children.** `remove_all_actor_children` clears the
   named-child registry in place with `Table::clear`, eliminating a temporary
   key vector, iterator-key clones, and per-key deletes. The registry retains
   its identity, aliases, metatable, and allocated capacity for reuse. Clearing
   the actor's numeric sequence still follows the original raw-length boundary,
   and unrelated actor fields are preserved.
3. **Message-command discovery.** Both ordinary and stable cross-actor capture
   use a shared raw visitor that checks for function values before borrowing
   string keys. It copies names only when they end in `MessageCommand`. This
   avoids allocating owned Rust strings for unrelated fields and methods, as
   well as iterator cursor and actor-handle clones. Both callers still collect
   all commands before running any callback. Ordinary capture retains its sort
   and repeated-suffix trimming; cross-actor capture retains traversal order,
   selected function identity, and single-suffix stripping.

All changes use the installed mlua 0.12.1 safe APIs. No dependency, public API,
cache, or production unsafe code was added. `Table::raw_remove` shifts integer
keys with raw Lua operations, and `Table::clear` bypasses metamethods while
retaining table storage. Discovery uses raw traversal and still ignores invalid
UTF-8 names, inherited entries, and non-string keys. Borrowing a matching or
unrelated method's Lua string can still allocate an mlua shared handle; this
pass does not claim zero allocations for all command discovery or dispatch.

## Behavior and validation

Seven parent function bodies are frozen in test-only modules. The queue oracle
includes the complete four-function recursive dispatch chain, so nested drains
use the old implementation. The two message-capture callers are frozen in full;
their other helpers are shared. The discovery microbenchmark uses an audited
exact copy of the ordinary caller's original collection-and-sort prefix. A
source audit reconstructs the parent source from the four changed function
bodies and the new helper and verifies all frozen bodies and the extracted
prefix. The patch changes exactly once from 0.5.1180 to 0.5.1181; the three
workspace-versioned lockfile entries change accordingly, with no dependency
updates.

Twelve new tests cover:

- Queue visibility during callbacks; appended/replaced entries; recursive
  dispatch; ActorFrame child propagation; active-command and cursor restoration;
  startup deferral and deduplication; missing queues; raw metamethod traps;
  holes, numeric names, invalid UTF-8, callback/conversion failures, and matching
  partial progress. An empty warm queue drain has zero allocator churn.
- Child registry aliases, shared groups, mixed key types, invalid UTF-8 keys,
  metatables, sparse sequence boundaries, missing/invalid registries,
  actor/registry self-aliasing, and refill/reuse. Clearing a populated 256-entry
  named registry has zero allocator churn.
- Message filtering, ordering, UTF-8 and embedded NULs, inherited/non-function
  exclusions, repeated suffixes, function identity, sorted capture while a
  callback adds a command, skipped errors, and stable cross-actor effects.
  Scanning 256 unrelated data fields has zero allocator churn.

Validation results:

- Targeted tests: **12 passed**, 3 manual benchmarks ignored.
- Full song-Lua debug and release suites: **513 passed, 5 failed, 36 ignored**
  in each profile. The fresh parent debug run had **501 passed, the same 5
  failures, and 33 ignored**. The failure names and assertion text match in both
  new profiles. The parent was not separately rebuilt in release mode.
- `cargo clippy -p deadsync-song-lua --all-targets --locked -- -D clippy::perf`
  and root `cargo check --all-targets --offline`: passed. Existing Clippy
  diagnostics match the prior pass after source-coordinate normalization.
- All 25 paired workloads completed in three release runs. Source/version
  audits and staged whitespace checks passed.

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

Windows x86_64, Intel Xeon E5-2696 v4 @ 2.20 GHz; rustc 1.98.0
(`88d9e12ae`, LLVM 22.1.8). The existing release profile uses opt-level 3 and full
LTO. Old and new implementations run in the same release test executable.
There are 25 paired workloads, each run three times serially with no concurrent
Rust compiler/linker processes. The middle run reverses old/new order. Each
measurement has three warmups and seven timing samples. Tables report medians
of the three run medians; raw logs include each run's sample range.
Small queue/child cases and empty discovery use 512 operations per sample;
larger queue cases use 32, other child/discovery cases 64, and complete message
capture cases 16. Longer batches reduce timer overhead in the small controls.

Timing and allocation accounting are separate. Thread cycles use Windows
`QueryThreadCycleTime`; these are thread CPU cycles, not retired instructions.
The thread-local allocator counts Rust and mlua allocation, reallocation, and
free calls and requested/freed bytes. Bytes measure cumulative allocator
traffic, not peak live memory or process RSS. No RSS reduction is inferred from
them. The allocator instrumentation is shared by both versions.

Lua VMs, keys, methods, child objects, and command functions are built outside
timing. Lua is collected before each benchmark group, then GC is stopped through
the paired measurements; Lua collection/VM teardown is excluded. Rust temporary
outputs are dropped inside the measured closure. Warmed Lua stack and table
capacity are reused. These measurements do not include cold setup or deferred
Lua GC costs.

Queue and child-removal operations mutate their inputs. Their timings and
allocation counts include the same refill step before each operation: existing
keys and references populate the retained tables. Queue timings also reset the
callback hit counter. Queue units are drained entries; child-removal units are
logical children (mixed cases populate both registries). Empty cases use one
operation as the unit. Queue cases include missing-command dispatch and actual
Lua callbacks, with distinct command names to exercise recursive draining.

Discovery units are inspected fields, including commands. The full ordinary
message-capture cases report commands per second and include snapshots, command
dispatch, state restoration, and output destruction; command bodies are noops
to isolate host processing. They show how discovery savings carry through its
caller. These synthetic workloads do not establish whole-song compilation,
gameplay throughput, FPS, or end-to-end memory gains. The full cross-actor path
has behavior coverage but is not separately timed.

## Interpretation

The 64-entry queue uses 65.58% fewer cycles; allocator traffic is unchanged.
With actual Lua callbacks, 8-entry and 32-entry queues use 6.32% and 27.96%
fewer cycles. Clearing 64 named children uses 73.62% fewer cycles and removes
all 65 allocation calls, 4 reallocations, 65 frees, and 5,984 requested/freed
bytes per operation, including the common refill step.

Discovery with 256 unrelated methods and 8 commands uses 36.20% fewer cycles;
allocation calls fall from 529 to 273 and requested bytes from 9,426 to 4,672.
The 256-data-field/no-command case has zero churn after the change. Complete
ordinary message capture has smaller measured gains: 5.09% fewer cycles for
64 data fields/4 commands and 2.72% for 256 methods/8 commands. Each of these
full-capture cases improved in all three paired final-code runs, though the
size of the improvement varied.

Singleton queues retain the original direct delete. They have unchanged
allocator counts, mixed per-run cycle signs, and effectively neutral aggregate
timing: the missing-command case is 0.17% higher in cycles (1,157.4 -> 1,162.9
ns/op), and the callback case is 0.14% lower. These differences do not establish
a CPU improvement or regression for singleton dispatch. The table includes
these controls rather than claiming every workload becomes faster.

## Results

Positive cycle savings indicate an improvement on this workload and machine.

| Scenario | Old us/op | New us/op | Old cycles/op | New cycles/op | Fewer cycles |
|---|---:|---:|---:|---:|---:|
| clear_named_0 | 0.1912 | 0.1738 | 421.9 | 384.1 | 8.96% |
| clear_named_1 | 0.6701 | 0.2545 | 1,473.5 | 549.2 | 62.73% |
| clear_named_8 | 2.8639 | 0.8084 | 6,289.6 | 1,776.6 | 71.75% |
| clear_named_64 | 19.9359 | 5.2547 | 43,793.7 | 11,551.2 | 73.62% |
| clear_named_512 | 167.3625 | 43.2625 | 365,573.8 | 94,933.8 | 74.03% |
| clear_mixed_64 | 24.8969 | 10.1469 | 54,569.8 | 22,217.5 | 59.29% |
| clear_sequence_64 | 4.7719 | 4.5719 | 10,398.8 | 9,959.8 | 4.22% |
| discovery_0_0_data | 0.0471 | 0.0307 | 105.9 | 70.3 | 33.62% |
| discovery_64_0_data | 18.7250 | 5.2875 | 41,046.5 | 11,623.2 | 71.68% |
| discovery_256_0_data | 76.3641 | 19.9047 | 167,433.9 | 43,677.1 | 73.91% |
| discovery_64_4_data | 19.2953 | 6.8719 | 42,305.2 | 15,104.3 | 64.30% |
| discovery_256_8_data | 79.1391 | 25.2953 | 173,432.5 | 55,485.5 | 68.01% |
| discovery_256_8_methods | 81.6250 | 52.0750 | 179,033.1 | 114,222.3 | 36.20% |
| discovery_512_32_mixed | 173.6812 | 82.9453 | 380,393.5 | 181,907.2 | 52.18% |
| discovery_0_64_data | 21.7078 | 18.2703 | 47,566.3 | 40,058.8 | 15.78% |
| message_capture_64_4_data | 237.6688 | 225.5500 | 521,147.9 | 494,602.1 | 5.09% |
| message_capture_256_8_methods | 1,322.0000 | 1,288.8125 | 2,897,948.8 | 2,819,216.8 | 2.72% |
| queue_0_callbacks_false | 0.1881 | 0.1838 | 415.0 | 406.0 | 2.17% |
| queue_1_callbacks_false | 1.1574 | 1.1629 | 2,544.8 | 2,549.1 | -0.17% |
| queue_8_callbacks_false | 11.3508 | 9.6018 | 24,890.5 | 20,997.8 | 15.64% |
| queue_64_callbacks_false | 273.0562 | 93.9094 | 598,370.7 | 205,973.3 | 65.58% |
| queue_256_callbacks_false | 3,594.9469 | 724.6844 | 7,881,456.2 | 1,588,679.2 | 79.84% |
| queue_1_callbacks_true | 2.9174 | 2.9178 | 6,406.7 | 6,397.7 | 0.14% |
| queue_8_callbacks_true | 26.0805 | 24.4389 | 57,165.6 | 53,552.4 | 6.32% |
| queue_32_callbacks_true | 145.6438 | 105.0125 | 319,262.8 | 230,001.7 | 27.96% |

A/R/F means allocation/reallocation/free calls. Bytes include reallocations and common input/output ownership costs.

| Scenario | Old A/R/F | New A/R/F | Old requested/freed B | New requested/freed B | Old units/s | New units/s |
|---|---:|---:|---:|---:|---:|---:|
| clear_named_0 | 0/0/0 | 0/0/0 | 0/0 | 0/0 | 5,229,826.4 | 5,752,809.0 |
| clear_named_1 | 2/0/2 | 0/0/0 | 176/176 | 0/0 | 1,492,276.3 | 3,929,393.7 |
| clear_named_8 | 9/1/9 | 0/0/0 | 608/608 | 0/0 | 2,793,425.6 | 9,896,110.2 |
| clear_named_64 | 65/4/65 | 0/0/0 | 5,984/5,984 | 0/0 | 3,210,282.9 | 12,179,601.5 |
| clear_named_512 | 513/7/513 | 0/0/0 | 48,992/48,992 | 0/0 | 3,059,227.7 | 11,834,729.8 |
| clear_mixed_64 | 65/4/65 | 0/0/0 | 5,984/5,984 | 0/0 | 2,570,603.7 | 6,307,360.6 |
| clear_sequence_64 | 0/0/0 | 0/0/0 | 0/0 | 0/0 | 13,411,918.8 | 13,998,632.9 |
| discovery_0_0_data | 0/0/0 | 0/0/0 | 0/0 | 0/0 | 21,244,813.3 | 32,611,465.0 |
| discovery_64_0_data | 128/0/128 | 0/0/0 | 2,166/2,166 | 0/0 | 3,417,890.5 | 12,104,018.9 |
| discovery_256_0_data | 512/0/512 | 0/0/0 | 8,850/8,850 | 0/0 | 3,352,362.2 | 12,861,292.1 |
| discovery_64_4_data | 137/0/137 | 9/0/9 | 2,406/2,406 | 240/240 | 3,524,172.0 | 9,895,407.0 |
| discovery_256_8_data | 529/1/529 | 17/1/17 | 9,426/9,426 | 576/576 | 3,335,900.0 | 10,436,716.3 |
| discovery_256_8_methods | 529/1/529 | 273/1/273 | 9,426/9,426 | 4,672/4,672 | 3,234,303.2 | 5,069,611.1 |
| discovery_512_32_mixed | 1089/3/1089 | 321/3/321 | 20,424/20,424 | 6,710/6,710 | 3,132,174.6 | 6,558,538.2 |
| discovery_0_64_data | 129/4/129 | 129/4/129 | 5,334/5,334 | 5,334/5,334 | 2,948,247.3 | 3,502,950.5 |
| message_capture_64_4_data | 1065/0/1029 | 933/0/897 | 20,310/18,174 | 18,064/15,928 | 16,830.1 | 17,734.4 |
| message_capture_256_8_methods | 5517/1/5445 | 5257/1/5185 | 95,330/91,058 | 90,496/86,224 | 6,051.4 | 6,207.3 |
| queue_0_callbacks_false | 0/0/0 | 0/0/0 | 0/0 | 0/0 | 5,316,718.6 | 5,441,020.2 |
| queue_1_callbacks_false | 2/1/2 | 2/1/2 | 26/26 | 26/26 | 863,989.2 | 859,926.1 |
| queue_8_callbacks_false | 16/8/16 | 16/8/16 | 208/208 | 208/208 | 704,797.3 | 833,180.8 |
| queue_64_callbacks_false | 128/64/128 | 128/64/128 | 1,718/1,718 | 1,718/1,718 | 234,383.9 | 681,508.1 |
| queue_256_callbacks_false | 512/256/512 | 512/256/512 | 7,058/7,058 | 7,058/7,058 | 71,211.1 | 353,257.2 |
| queue_1_callbacks_true | 5/1/2 | 5/1/2 | 224/26 | 224/26 | 342,773.0 | 342,727.1 |
| queue_8_callbacks_true | 40/8/16 | 40/8/16 | 1,792/208 | 1,792/208 | 306,743.0 | 327,347.4 |
| queue_32_callbacks_true | 160/32/64 | 160/32/64 | 7,190/854 | 7,190/854 | 219,714.2 | 304,725.6 |

Per-run paired cycle savings for representative cases:

- `queue_64_callbacks_false`: 65.74%, 65.32%, 65.34%.
- `clear_named_64`: 76.51%, 73.41%, 73.62%.
- `discovery_256_8_methods`: 42.02%, 37.03%, 35.62%.
- `queue_1_callbacks_false`: 7.41%, -6.52%, -0.17%.
- `queue_1_callbacks_true`: -1.53%, -0.13%, 1.11%.
- `message_capture_64_4_data`: 2.53%, 4.72%, 8.03%.
- `message_capture_256_8_methods`: 1.95%, 4.90%, 5.05%.

Cases with higher median cycle counts:

- `queue_1_callbacks_false`: 0.17% more cycles; elapsed 1157.4 -> 1162.9 ns/op.

## Reproduction

```powershell
cargo test -p deadsync-song-lua --lib lua_state_ --locked
cargo clippy -p deadsync-song-lua --all-targets --locked -- -D clippy::perf
cargo check --all-targets --offline
cargo test -p deadsync-song-lua --lib --locked
cargo test -p deadsync-song-lua --lib --release --locked
cargo test -p deadsync-song-lua --lib --release lua_state_bench --locked -- --ignored --test-threads=1 --nocapture
$env:DEADSYNC_PERF_REVERSE = '1'
cargo test -p deadsync-song-lua --lib --release lua_state_bench --locked -- --ignored --test-threads=1 --nocapture
Remove-Item Env:DEADSYNC_PERF_REVERSE
cargo test -p deadsync-song-lua --lib --release lua_state_bench --locked -- --ignored --test-threads=1 --nocapture
```

Run benchmarks without competing compiler processes. The recorded runs invoke
the already-built executable directly. Local logs, extracted metrics, source
audits, and orchestration scripts live under ignored `target/lua-state-perf/`.
Frozen implementations and regression/benchmark tests are committed under
`crates/deadsync-song-lua/tests/perf/{queue_drain,children_clear,message_discovery}*.rs`.
The excluded song, guide, and optimization scripts are absent from this commit.
