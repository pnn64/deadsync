# Lua capture allocation reductions - 0.5.1179

Parent: `619d10d4a340b2d5aa9cebe90ea38dc080059338` (`0.5.1178`).
This pass follows `rust-performance.md`: measure CPU and allocation costs
(M-HOTPATH), avoid short-lived strings and collections (M-MEM-REUSE), and reserve
known output sizes (M-INITIAL-CAPACITY).

## Three changes

1. **Retain Lua keys while restoring scalar globals.** Temporary globals are
   removed through their existing Lua string handles. Up to 16 removal keys fit
   on the stack; larger batches spill into one growable buffer. This eliminates
   per-key Rust strings, repeated key conversion on deletion, and an unnecessary
   globals handle clone. All string keys are still UTF-8 validated before any
   writes, including keys holding non-scalar values. Snapshot replay, metatable
   handling, and partial-error behavior remain unchanged.
2. **Build action snapshots directly.** Function-action snapshots reserve exactly
   one or two outer entries after resolving the function environment. They no
   longer allocate an intermediate vector of cloned table handles or clone each
   table merely to enumerate its entries. Collection uses mlua's safe
   `Table::for_each` API: the traversal key stays on Lua's stack instead of being
   cloned into a Rust iterator cursor. This removes a shared-handle allocation
   for every reference-valued key. Both APIs perform raw Lua traversal without
   `__pairs`. Environment lookup still precedes all snapshotting;
   globals/environment aliases still produce one snapshot, and proxy targets
   still select the same table. Entry values remain shallow owned Lua references,
   with the same traversal and restoration order.
3. **Merge ordered cross-actor effects.** Strictly increasing auxiliary and block
   lists merge directly into one output vector, removing the temporary B-tree.
   Unordered or duplicate actor lists retain the original B-tree path and its
   last-write-wins behavior. Source actors are excluded, output remains sorted,
   and block vectors/strings are still independently owned. An ordered merge
   reserves the sum of the two input lengths; when the lists overlap, its output
   retains spare capacity until the caller drops it. This avoids growth and tree
   nodes, but does not minimize the retained output capacity in every case.

No dependency, persistent cache, public API, or production unsafe code changed.
These paths run during actor command preservation and function/message action
capture. The measurements below are scoped benchmarks; they do not establish
whole-song compilation time or gameplay FPS gains.

## Behavior and validation

Four parent functions are frozen in test-only modules. A source audit confirms
their bodies match the parent, only those four production functions changed,
and shared conversions, restoration helpers, and public exports are unchanged.
The workspace version increases exactly once from 0.5.1178 to 0.5.1179; the three
workspace-versioned lockfile package entries follow it without dependency changes.

Ten new tests cover:

- Scalar restoration across scalar and non-scalar values, numeric/boolean keys,
  Unicode/NUL/long string keys, invalid UTF-8 keys and string values, NaN/infinity
  and signed zero, table/function identity, deletion before replay, metatable
  write order, and partial errors. Invalid keys prevent every write. Empty
  restoration has zero allocator churn.
- Native and Lua functions, global environments, distinct environments, proxy
  targets, global aliases, empty/populated snapshots, raw target lookups,
  conversion errors, mixed key types and values, shallow ownership, table
  identity/metatables, traversal order, ignored `__pairs`, and rollback. The
  outer vector has exact capacity; an empty native-function snapshot uses one
  required output allocation. Four reference-valued fields use only two output
  buffer allocations, with no per-key cursor allocations.
- Ordered, disjoint, auxiliary-only, blocks-only, empty-block, unordered and
  duplicate inputs; source filtering; 2,000 generated arbitrary cases and their
  ordered counterparts; last-write-wins behavior; nonfinite auxiliary values and
  signed-zero bits; and owned block/string results. Empty/source-only effects
  have zero churn. A 64-actor auxiliary-only merge has one output allocation.

Validation:

- Fresh parent debug suite before edits: **481 passed, 5 failed, 27 ignored**.
- New targeted tests: **10 passed**, three manual benchmarks ignored.
- New full debug and release suites: **491 passed, 5 failed, 30 ignored** each.
  Failure names and assertion text match the fresh parent debug run. The parent
  release suite was not rebuilt in this pass.
- Root `cargo check --all-targets --offline`: passed.
- `cargo clippy -p deadsync-song-lua --all-targets --locked -- -D clippy::perf`:
  passed, with 53 existing nonperformance warnings and none in the new tests.
- Frozen-source/shared-helper audit, exact +1 version/lock audit, and staged
  whitespace check: passed.

The five existing failures are:

- `compile_song_lua_extracts_actorproxy_targets`: initial visibility.
- `compile_song_lua_layers_share_init_globals_and_actor_refs`: 0 versus 123.
- `compile_song_lua_resolves_local_and_hidden_screen_proxy_targets`: initial visibility.
- `compile_song_lua_runs_cmd_queuecommand_builders`: initial visibility.
- `compile_song_lua_supports_notefield_column_api`: `-96:-135` versus `-96:-125`.

Those failures keep the full-suite commands from returning success.

## Benchmark method

Windows x86-64, Intel Xeon E5-2696 v4 @ 2.20 GHz, Rust 1.98.0
(`88d9e12ae`, LLVM 22.1.8), release optimization level 3 with full LTO.
The existing helper takes seven timing samples after three warmups, followed by
a separately counted allocation operation. Three serial executable runs
alternate old/new order: old first, new first, old first. Each run starts after
compiler/linker work ends. Results aggregate the median of the three per-run
medians. All 35 paired cases are reported, including unchanged fallback paths.
Allocation/reallocation/free counts and requested/freed bytes agree across all three runs.

Windows `QueryThreadCycleTime` measures calling-thread CPU cycles, not retired
instructions. Thread-local counters cover allocations routed through Rust's
System allocator, including Lua allocations in this mlua 0.12.1 / vendored Lua
5.4 build. Other threads and allocations bypassing that allocator are outside
the counters. Requested/freed bytes are cumulative, not peak live memory or RSS.
Lua garbage collection is stopped for deterministic isolated measurements;
temporary Lua objects remain for VM teardown outside timing. Setup is excluded;
Rust input/output destruction during each operation is included.

Operation boundaries and throughput units:

- **Scalar restoration:** one add/restore cycle, 128 operations/sample. Fixtures
  have 0, 16 or 64 saved globals, 0 through 512 temporary globals, and short or
  long keys. The operation adds the temporary fields with pre-created Lua keys,
  clones the saved Rust snapshot input, restores globals, and drops consumed
  Rust input. It excludes the unchanged initial snapshot scan. Units count
  saved plus temporary fields, or operations for the empty case. Lua table
  capacity and keys are warmed. Long-key deletion can still allocate Lua
  strings in the old implementation; their GC cost is outside timing.
- **Action snapshots:** one complete snapshot construction and Rust output drop,
  128 operations/sample. Native functions, globals, distinct environments,
  environment proxies and globals aliases have 0, 16 or 256 scalar entries per
  snapshotted table. Units count captured entries, or snapshots when empty.
  Lookup keys are warmed; each invocation obtains its own globals handle.
- **Cross-actor effects:** one complete merge and owned output drop, 256
  operations/sample. Fixtures have 0, 1, 8, 64 or 512 actors, including auxiliary
  values, owned command blocks with easing strings, disjoint lists, empty block
  vectors, reversed lists, and duplicate inputs. Units count input auxiliary plus
  block-list entries, or operations when empty. The timed source index is absent
  from the inputs; separate tests cover source removal.

Old/new results or table state are compared before timing. Frozen baselines and
new code compile in the same binary with the same dependencies. Black-boxed
inputs and consumed results limit constant folding. Small timing differences
remain subject to machine noise and code layout effects.

## Representative results

- **Scalar rollback, 64 saved globals plus 64 long temporary keys:** 41.97% fewer cycles, 112.745 -> 65.260 us/op; A/R/F 324/4/260 -> 195/1/195, requested bytes 47,842 -> 11,142.
- **Action snapshots, 16 globals plus 16 environment entries:** 42.77% fewer cycles, 8.277 -> 4.755 us/op; A/R/F 38/5/38 -> 3/4/3, requested bytes 5,336 -> 4,576.
- **Cross-actor effects, 64 actors with auxiliary values and command blocks:** 48.03% fewer cycles, 23.637 -> 12.158 us/op; A/R/F 137/4/137 -> 129/0/129, requested bytes 55,790 -> 50,102.

Remaining allocation costs include owned snapshot inputs/entries and output block vectors/strings. The full capture pipeline is not allocation-free. See every scenario and any slower cases below.

## Results

Positive cycle savings indicate an improvement on this workload and machine.

| Scenario | Old us/op | New us/op | Old cycles/op | New cycles/op | Fewer cycles |
|---|---:|---:|---:|---:|---:|
| snapshot_native_0 | 0.5516 | 0.1836 | 1,221.0 | 415.0 | 66.01% |
| snapshot_native_16 | 4.4508 | 2.5305 | 9,779.8 | 5,566.4 | 43.08% |
| snapshot_native_256 | 58.1000 | 28.5250 | 127,474.6 | 62,046.5 | 51.33% |
| snapshot_globals_0 | 0.4547 | 0.2922 | 1,008.3 | 649.9 | 35.54% |
| snapshot_globals_16 | 4.2461 | 2.0602 | 9,332.2 | 4,530.6 | 51.45% |
| snapshot_globals_256 | 58.4711 | 27.8195 | 128,105.7 | 60,925.0 | 52.44% |
| snapshot_environment_0 | 0.7070 | 0.3594 | 1,562.2 | 799.1 | 48.85% |
| snapshot_environment_16 | 8.2766 | 4.7547 | 18,167.1 | 10,397.1 | 42.77% |
| snapshot_environment_256 | 149.2320 | 55.3125 | 324,637.1 | 120,690.7 | 62.82% |
| snapshot_proxy_0 | 0.6453 | 0.3453 | 1,425.0 | 766.5 | 46.21% |
| snapshot_proxy_16 | 7.6172 | 5.0617 | 16,731.7 | 10,927.0 | 34.69% |
| snapshot_proxy_256 | 119.4680 | 53.1383 | 261,844.6 | 114,273.8 | 56.36% |
| snapshot_alias_0 | 0.5078 | 0.3031 | 1,123.2 | 673.9 | 40.00% |
| snapshot_alias_16 | 3.6367 | 1.9555 | 7,980.9 | 4,300.8 | 46.11% |
| snapshot_alias_256 | 57.4469 | 28.0594 | 124,808.0 | 61,401.7 | 50.80% |
| effects_both_0 | 0.0336 | 0.0078 | 78.9 | 22.3 | 71.74% |
| effects_both_1 | 0.5812 | 0.2598 | 1,281.8 | 576.2 | 55.05% |
| effects_both_8 | 2.0336 | 1.8879 | 4,456.0 | 4,149.9 | 6.87% |
| effects_both_64 | 23.6375 | 12.1582 | 51,269.5 | 26,643.5 | 48.03% |
| effects_both_512 | 201.6465 | 131.6875 | 439,210.9 | 288,446.2 | 34.33% |
| effects_aux_64 | 2.8703 | 0.6660 | 6,308.9 | 1,466.2 | 76.76% |
| effects_aux_512 | 25.6363 | 5.6141 | 55,907.3 | 12,196.0 | 78.19% |
| effects_blocks_64 | 15.0324 | 11.2707 | 32,078.7 | 24,729.8 | 22.91% |
| effects_disjoint_64 | 19.9215 | 12.8723 | 43,306.7 | 27,990.5 | 35.37% |
| effects_empty_blocks_64 | 4.3332 | 1.3031 | 9,487.4 | 2,865.5 | 69.80% |
| effects_unordered_64 | 15.8312 | 15.8281 | 34,535.2 | 34,699.0 | -0.47% |
| effects_duplicates_64 | 31.3980 | 31.8480 | 68,567.2 | 69,527.5 | -1.40% |
| scalar_0_0_8 | 0.1555 | 0.1039 | 351.5 | 238.4 | 32.18% |
| scalar_16_0_8 | 8.7219 | 8.6445 | 19,142.8 | 18,962.7 | 0.94% |
| scalar_16_8_8 | 12.6945 | 10.5750 | 27,864.5 | 22,589.6 | 18.93% |
| scalar_16_16_8 | 16.5883 | 15.6164 | 36,142.1 | 34,278.0 | 5.16% |
| scalar_16_17_8 | 17.4664 | 15.2750 | 38,323.3 | 33,528.6 | 12.51% |
| scalar_64_64_8 | 67.3297 | 56.6664 | 147,145.6 | 124,357.0 | 15.49% |
| scalar_64_512_8 | 305.5984 | 245.9891 | 669,384.1 | 538,942.8 | 19.49% |
| scalar_64_64_256 | 112.7453 | 65.2602 | 246,610.0 | 143,103.7 | 41.97% |

A/R/F means allocation/reallocation/free calls. Bytes include reallocations and common input/output ownership costs.

| Scenario | Old A/R/F | New A/R/F | Old requested/freed B | New requested/freed B | Old units/s | New units/s |
|---|---:|---:|---:|---:|---:|---:|
| snapshot_native_0 | 3/0/3 | 1/0/1 | 232/232 | 48/48 | 1,813,031.2 | 5,446,808.5 |
| snapshot_native_16 | 20/2/20 | 2/2/2 | 2,728/2,728 | 2,288/2,288 | 3,594,874.5 | 6,322,939.2 |
| snapshot_native_256 | 260/6/260 | 2/6/2 | 44,968/44,968 | 40,688/40,688 | 4,406,196.2 | 8,974,583.7 |
| snapshot_globals_0 | 3/0/3 | 1/0/1 | 232/232 | 48/48 | 2,199,312.7 | 3,422,459.9 |
| snapshot_globals_16 | 20/2/20 | 2/2/2 | 2,728/2,728 | 2,288/2,288 | 3,768,169.3 | 7,766,401.2 |
| snapshot_globals_256 | 260/6/260 | 2/6/2 | 44,968/44,968 | 40,688/40,688 | 4,378,231.8 | 9,202,168.0 |
| snapshot_environment_0 | 4/1/4 | 1/0/1 | 344/344 | 96/96 | 1,414,364.6 | 2,782,608.7 |
| snapshot_environment_16 | 38/5/38 | 3/4/3 | 5,336/5,336 | 4,576/4,576 | 3,866,339.4 | 6,730,200.5 |
| snapshot_environment_256 | 518/13/518 | 3/12/3 | 89,816/89,816 | 81,376/81,376 | 3,430,898.8 | 9,256,497.2 |
| snapshot_proxy_0 | 4/1/4 | 1/0/1 | 344/344 | 96/96 | 1,549,636.8 | 2,895,927.6 |
| snapshot_proxy_16 | 38/5/38 | 3/4/3 | 5,336/5,336 | 4,576/4,576 | 4,201,025.6 | 6,321,963.3 |
| snapshot_proxy_256 | 518/13/518 | 3/12/3 | 89,816/89,816 | 81,376/81,376 | 4,285,667.6 | 9,635,238.2 |
| snapshot_alias_0 | 3/0/3 | 1/0/1 | 232/232 | 48/48 | 1,969,230.8 | 3,298,969.1 |
| snapshot_alias_16 | 20/2/20 | 2/2/2 | 2,728/2,728 | 2,288/2,288 | 4,399,570.4 | 8,182,181.4 |
| snapshot_alias_256 | 260/6/260 | 2/6/2 | 44,968/44,968 | 40,688/40,688 | 4,456,291.1 | 9,123,510.4 |
| effects_both_0 | 0/0/0 | 0/0/0 | 0/0 | 0/0 | 29,767,441.9 | 128,000,000.0 |
| effects_both_1 | 5/0/5 | 3/0/3 | 1,358/1,358 | 782/782 | 3,440,860.2 | 7,699,248.1 |
| effects_both_8 | 19/1/19 | 17/0/17 | 6,872/6,872 | 6,256/6,256 | 7,867,844.8 | 8,475,067.2 |
| effects_both_64 | 137/4/137 | 129/0/129 | 55,790/55,790 | 50,102/50,102 | 5,415,124.3 | 10,527,871.5 |
| effects_both_512 | 1075/7/1075 | 1025/0/1025 | 464,466/464,466 | 401,298/401,298 | 5,078,194.2 | 7,775,984.8 |
| effects_aux_64 | 9/4/9 | 1/0/1 | 10,808/10,808 | 2,560/2,560 | 22,297,223.7 | 96,093,841.6 |
| effects_aux_512 | 51/7/51 | 1/0/1 | 104,128/104,128 | 20,480/20,480 | 19,971,658.9 | 91,199,554.7 |
| effects_blocks_64 | 139/0/139 | 129/0/129 | 52,198/52,198 | 47,542/47,542 | 4,257,464.3 | 5,678,439.0 |
| effects_disjoint_64 | 149/4/149 | 129/0/129 | 64,014/64,014 | 50,102/50,102 | 6,425,224.0 | 9,943,859.4 |
| effects_empty_blocks_64 | 9/4/9 | 1/0/1 | 10,808/10,808 | 5,120/5,120 | 29,539,349.1 | 98,225,419.7 |
| effects_unordered_64 | 137/4/137 | 137/4/137 | 55,790/55,790 | 55,790/55,790 | 8,085,274.4 | 8,086,870.7 |
| effects_duplicates_64 | 266/5/266 | 266/5/266 | 111,012/111,012 | 111,012/111,012 | 8,153,373.4 | 8,038,169.5 |
| scalar_0_0_8 | 1/0/1 | 0/0/0 | 16/16 | 0/0 | 6,432,160.8 | 9,624,060.2 |
| scalar_16_0_8 | 35/0/35 | 34/0/34 | 1,974/1,974 | 1,958/1,958 | 1,834,467.9 | 1,850,881.2 |
| scalar_16_8_8 | 52/1/52 | 42/0/42 | 2,550/2,550 | 2,086/2,086 | 1,890,577.9 | 2,269,503.5 |
| scalar_16_16_8 | 68/2/68 | 50/0/50 | 3,228/3,228 | 2,214/2,214 | 1,929,072.7 | 2,049,127.0 |
| scalar_16_17_8 | 70/3/70 | 52/0/52 | 4,033/4,033 | 2,998/2,998 | 1,889,341.1 | 2,160,392.8 |
| scalar_64_64_8 | 260/4/260 | 195/1/195 | 13,164/13,164 | 11,142/11,142 | 1,901,093.0 | 2,258,833.9 |
| scalar_64_512_8 | 1156/7/1156 | 643/4/643 | 51,656/51,656 | 39,814/39,814 | 1,884,826.4 | 2,341,567.5 |
| scalar_64_64_256 | 324/4/260 | 195/1/195 | 47,842/29,036 | 11,142/11,142 | 1,135,302.2 | 1,961,380.5 |

Per-run paired cycle savings for representative cases:

- `scalar_64_64_256`: 37.28%, 39.98%, 47.49%.
- `snapshot_environment_16`: 48.37%, 50.00%, 16.73%.
- `effects_both_64`: 42.95%, 48.01%, 50.00%.

Cases with higher median cycle counts:

- `effects_unordered_64`: 0.47% more cycles; elapsed 15831.2 -> 15828.1 ns/op.
- `effects_duplicates_64`: 1.40% more cycles; elapsed 31398.0 -> 31848.0 ns/op.


## Reproduce

```powershell
cargo test -p deadsync-song-lua --lib lua_capture_ --locked
cargo test -p deadsync-song-lua --lib --release --locked lua_capture_bench -- --ignored --test-threads=1 --nocapture
$env:DEADSYNC_PERF_REVERSE = '1'
cargo test -p deadsync-song-lua --lib --release --locked lua_capture_bench -- --ignored --test-threads=1 --nocapture
Remove-Item Env:DEADSYNC_PERF_REVERSE
cargo test -p deadsync-song-lua --lib --release --locked lua_capture_bench -- --ignored --test-threads=1 --nocapture
```

Run benchmarks without concurrent compiler/linker work. Machine-local raw logs
and audit scripts are under ignored `target/lua-capture-perf/`. Frozen baselines,
tests, benchmark cases and this results report are committed. The four excluded
files (`deadsync-song.json.gz`, `rust-performance.md`, `optimize.sh`, and
`optimize.ps1`) are not part of the commit.
