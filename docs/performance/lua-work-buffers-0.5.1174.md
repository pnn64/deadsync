# Lua work buffers - 0.5.1174

Parent: `b63f6a0c9d58926bb863e2aae5cbc7e94690d476` (`0.5.1173`).
This pass applies the local `rust-performance.md` guidance on measuring hot
paths (M-HOTPATH), reusing allocations (M-MEM-REUSE), and avoiding repeated
work (M-THROUGHPUT).

## Three changes

1. **Avoid copying every mod token to detect speed modifiers.**
   `FromString` checks each token for a speed modifier before parsing other
   mods. Unspaced tokens now borrow their input. Case-insensitive marker
   matching and Rust's float parser preserve the previous result without
   lowercasing into a new String. Tokens with interior Unicode whitespace use
   a 64-byte inline buffer, spilling to the heap for longer compacted input.
   This also removes the temporary allocation for common non-speed tokens.
2. **Use adaptive membership for child-actor traversal.**
   First-seen actor pointers fit in an inline 32-element SmallVec for small
   actor lists. On the 33rd distinct pointer, membership switches to an
   FxHashSet, avoiding quadratic duplicate scans in large actor lists.
   Only membership uses the set; output retains Lua's original sequence,
   named-child, and group traversal order. Duplicate handling and errors are
   unchanged. The set can use more temporary memory than the old pointer Vec
   for large lists; it is released when traversal returns. The deliberate
   inline enum variant has a scoped `large_enum_variant` lint allowance.
   The exported Vec-based helper retains its signature and original behavior;
   the adaptive membership type is private.
3. **Retain completed-tween scratch across sample ticks.**
   Completed scheduled updates drain into the existing per-compilation scratch
   owner. Merging consumes them through a Vec drain, leaving capacity ready
   for the next tick. Stable end-time/start-time sorting, state updates,
   pending order, track normalization, and last-write precedence are preserved.
   Completed strings and values are dropped or moved into output as before;
   only empty backing storage is retained. Its high-water capacity lives until
   compilation ends. Sorting and output growth can still allocate.

The child membership buffer has 256 bytes of inline pointer payload on this
64-bit target, plus container metadata. The speed parser has 64 bytes of inline
payload. Neither adds persistent caches to songs or runtime gameplay objects.
No dependencies or production unsafe code were added. These changes target
song-Lua compilation; isolated benchmark results do not establish whole-song
load-time or gameplay frame-rate gains.

## Behavior and validation

Three test-only modules freeze five parent functions: speed parsing, child
membership/collection, and completed/scheduled tween merging. Audits permit
only test visibility changes. Shared track-sort and ordered-append helpers are
unchanged. An older capture baseline adds `..` to its scratch destructuring
solely to accept the new field; its algorithm is unchanged. A test-only
completion wrapper preserves the old calling convention for existing tests.

Ten new tests cover:

- Prefix/suffix precedence, mixed ASCII case, Unicode whitespace, UTF-8
  boundaries, long inputs, infinities, NaNs, signed zero, numeric overflow and
  underflow, malformed tokens, and 20,000 generated parser inputs. Float
  results are compared by bits against the parent parser.
- Child counts around the inline/hash boundary, repeated and shuffled
  pointers, sequence/named/group overlaps, group-only children, non-table
  values, missing child-table creation, and errors after earlier valid actors.
  Tests compare first-seen identity and order within the same Lua state.
- Tween ties, overlaps, epsilon boundaries, rewinds, NaN/infinite times,
  truncated state slices, preexisting unsorted tracks, metadata lifetime,
  shared color identity, and pending/completed order across multiple ticks.
- No allocation churn for common speed/mod tokens, short spaced tokens, long
  unspaced tokens, small child membership with caller-owned output, and warmed
  small completion batches with retained track and scratch capacity.

Validation results:

- `cargo test -p deadsync-song-lua --lib lua_work --offline`: **10 passed**,
  three manual benchmarks ignored.
- Full debug and release song-Lua tests (`cargo test -p deadsync-song-lua --lib
  --locked`, adding `--release` for release): **441 passed, 5 failed, 15 ignored**
  in each configuration. Both runs have the same five test names and assertion
  text as the recorded parent run in `target/capture-dispatch-perf/all.log`.
  The parent was not rebuilt for this pass's full-suite comparison.
- `cargo check --all-targets --offline`: passed for the root workspace targets.
- `cargo clippy -p deadsync-song-lua --all-targets --locked -- -D clippy::perf`:
  passed; song-Lua reports 53 existing nonperformance warnings. None refer to
  the new test files.
- Frozen-source/shared-helper/public-API audit, exact +1 manifest/lock version
  audit, and staged whitespace check: passed.

The five existing failures remain:

- `compile_song_lua_extracts_actorproxy_targets`: initial visibility assertion.
- `compile_song_lua_layers_share_init_globals_and_actor_refs`: 0 versus 123.
- `compile_song_lua_resolves_local_and_hidden_screen_proxy_targets`: initial
  visibility assertion.
- `compile_song_lua_runs_cmd_queuecommand_builders`: initial visibility assertion.
- `compile_song_lua_supports_notefield_column_api`: `-96:-135` versus `-96:-125`.

All ten new tests pass in both full-suite runs. These existing failures keep
those full-suite commands from returning success.


## Benchmark method

Windows x86-64, Intel Xeon E5-2696 v4 @ 2.20 GHz, Rust 1.98.0
(`88d9e12ae`, LLVM 22.1.8), release optimization with full LTO.
The existing allocator/benchmark helper takes seven timing samples after three
warmups, then counts allocations in a separate operation. Three isolated
executable runs alternate old/new order (old first, new first, old first).
No Cargo build runs during measurement. Tables use the median of the three
per-run medians. All 23 paired scenarios are reported, including controls.

Windows `QueryThreadCycleTime` measures calling-thread cycles, not retired
instructions. Thread-local counters measure allocations, reallocations, frees,
and cumulative requested/freed bytes. They include Lua allocations routed
through Rust's allocator by this mlua 0.12.1 / vendored Lua 5.4 build. Other
threads and allocations bypassing Rust's allocator are outside the counters.
Byte totals are not peak live memory, retained capacity, or process RSS.

Fixtures, input strings, Lua states/tables, and template samples are prepared
outside timing. All tween output buffers are observed with black_box. Each old/new pair checks equivalent output before timing.
Ownership boundaries and throughput units:

- `speed_*`: 16 scans of each listed token set per operation. Common speed
  tokens, ordinary mods, spaced tokens, empty input, long unspaced input, and
  long ASCII/Unicode spaced input are separate cases. Results are consumed with black_box;
  parser temporary creation/destruction is inside timing. Throughput is tokens.
- `children_N_repeat_R_groups_G`: a full call collecting N distinct child
  actors appearing R times in the sequence, plus N reversed entries in a
  named group when G is true. Output and membership creation/destruction are
  included. Throughput counts visited child entries, or calls for empty input.
- `completed_N_mode_cold_C`: reset existing tracks to their first sample,
  restore actor states, clear/refill the pending sample vector from retained
  templates, then complete updates at beat 4.0. Timing includes template
  clones, prior output cleanup, extraction, sorting, state writes, and track
  merging. Tracks, maps, and pending capacity are warmed in both versions.
  The new completion capacity is retained except in the cold control, which
  discards it at each operation. Cases complete all, none, half, or reversed
  updates. Throughput counts inspected pending samples, or calls when empty.

Each timing sample uses 512 parser operations, 64 child traversals, or 128
completion batches. Empty-input timings are dominated by loop/measurement
overhead and are not representative gains. Allocation savings can trade
against retained scratch or hash-table space; those limits remain relevant
even when cumulative requested bytes decrease.

Reproduce:

```powershell
cargo test -p deadsync-song-lua --lib lua_work --locked
cargo test -p deadsync-song-lua --lib --release --locked
# Use Cargo's printed release test executable. Run three times, setting
# DEADSYNC_PERF_REVERSE=1 only for the middle run.
& <release-test-executable> lua_work_bench --ignored --test-threads=1 --nocapture
```

Measured effects on representative workloads:

- `speed_common`: 65.59% fewer cycles; 12.490 -> 4.294 us/op; allocation calls 128 -> 0, requested bytes 1,024 -> 0.
- `children_1024_repeat_4_groups_true`: 56.39% fewer cycles; 863.648 -> 376.428 us/op; allocation calls 3 -> 7, requested bytes 65,424 -> 84,864.
- `completed_8_all_cold_false`: 32.13% fewer cycles; 1.034 -> 0.701 us/op; allocation calls 1 -> 0, requested bytes 1,440 -> 0.

Allocation counters were identical across all three runs for every scenario.
The large-child case trades additional hash-table space for fewer CPU cycles;
small lists avoid a membership allocation. Completed scratch retains capacity,
and larger batches still allocate stable-sort workspace. All slower controls
are listed below; these results do not imply improvements for every workload.

## Results

Positive cycle savings indicate an improvement on this workload and machine.

| Scenario | Old us/op | New us/op | Old cycles/op | New cycles/op | Fewer cycles |
|---|---:|---:|---:|---:|---:|
| children_0_repeat_1_groups_false | 0.1953 | 0.2047 | 449.3 | 469.9 | -4.58% |
| children_4_repeat_1_groups_false | 0.5984 | 0.5578 | 1,337.6 | 1,241.5 | 7.18% |
| children_32_repeat_1_groups_false | 3.2516 | 3.0734 | 7,157.8 | 6,763.3 | 5.51% |
| children_33_repeat_1_groups_false | 3.5578 | 3.6672 | 7,830.0 | 8,066.6 | -3.02% |
| children_128_repeat_1_groups_false | 12.7562 | 11.4422 | 28,024.0 | 25,081.3 | 10.50% |
| children_1024_repeat_1_groups_false | 184.3734 | 90.2266 | 403,986.3 | 197,694.0 | 51.06% |
| children_1024_repeat_4_groups_true | 863.6484 | 376.4281 | 1,891,733.3 | 825,069.6 | 56.39% |
| children_4_repeat_128_groups_true | 37.9203 | 38.6938 | 83,207.6 | 84,943.1 | -2.09% |
| completed_0_all_cold_false | 0.0500 | 0.0523 | 118.3 | 125.2 | -5.83% |
| completed_1_all_cold_false | 0.3852 | 0.1375 | 859.1 | 312.1 | 63.67% |
| completed_8_all_cold_false | 1.0344 | 0.7008 | 2,279.0 | 1,546.8 | 32.13% |
| completed_128_all_cold_false | 20.9344 | 20.4359 | 45,932.1 | 44,808.9 | 2.45% |
| completed_128_none_cold_false | 1.3438 | 1.3680 | 2,961.5 | 3,016.4 | -1.85% |
| completed_128_half_cold_false | 9.7594 | 9.0516 | 21,396.1 | 19,815.0 | 7.39% |
| completed_128_reverse_cold_false | 19.0703 | 17.8102 | 41,831.9 | 39,077.9 | 6.58% |
| completed_128_all_cold_true | 20.3672 | 20.6484 | 44,654.5 | 45,277.0 | -1.39% |
| speed_common | 12.4904 | 4.2941 | 27,370.6 | 9,417.9 | 65.59% |
| speed_mods | 14.4967 | 5.7795 | 31,792.8 | 12,680.0 | 60.12% |
| speed_spaced | 6.2873 | 3.5146 | 13,793.8 | 7,711.7 | 44.09% |
| speed_empty | 0.4527 | 0.3441 | 996.3 | 758.0 | 23.92% |
| speed_long_plain | 18.8225 | 4.8660 | 41,281.4 | 10,672.8 | 74.15% |
| speed_long_spaced | 12.4787 | 6.0531 | 27,359.0 | 13,281.0 | 51.46% |
| speed_long_unicode | 16.4457 | 10.9066 | 36,078.6 | 23,898.9 | 33.76% |

A/R/F means allocation/reallocation/free calls. Bytes include reallocations and common input/output ownership costs.

| Scenario | Old A/R/F | New A/R/F | Old requested/freed B | New requested/freed B | Old units/s | New units/s |
|---|---:|---:|---:|---:|---:|---:|
| children_0_repeat_1_groups_false | 0/0/0 | 0/0/0 | 0/0 | 0/0 | 5,120,000.0 | 4,885,496.2 |
| children_4_repeat_1_groups_false | 2/0/2 | 1/0/1 | 128/128 | 96/96 | 6,684,073.1 | 7,170,868.3 |
| children_32_repeat_1_groups_false | 2/6/2 | 1/3/1 | 1,920/1,920 | 1,440/1,440 | 9,841,422.4 | 10,411,794.6 |
| children_33_repeat_1_groups_false | 2/8/2 | 2/4/2 | 3,968/3,968 | 4,144/4,144 | 9,275,362.3 | 8,998,721.8 |
| children_128_repeat_1_groups_false | 2/10/2 | 3/5/3 | 8,064/8,064 | 9,536/9,536 | 10,034,296.9 | 11,186,672.1 |
| children_1024_repeat_1_groups_false | 2/16/2 | 6/8/6 | 65,408/65,408 | 84,848/84,848 | 5,553,945.4 | 11,349,207.7 |
| children_1024_repeat_4_groups_true | 3/16/3 | 7/8/7 | 65,424/65,424 | 84,864/84,864 | 5,928,338.2 | 13,601,534.2 |
| children_4_repeat_128_groups_true | 3/0/3 | 2/0/2 | 144/144 | 112/112 | 13,607,482.8 | 13,335,487.0 |
| completed_0_all_cold_false | 0/0/0 | 0/0/0 | 0/0 | 0/0 | 20,000,000.0 | 19,104,477.6 |
| completed_1_all_cold_false | 1/0/1 | 0/0/0 | 480/480 | 0/0 | 2,596,348.9 | 7,272,727.3 |
| completed_8_all_cold_false | 1/1/1 | 0/0/0 | 1,440/1,440 | 0/0 | 7,734,139.0 | 11,415,830.5 |
| completed_128_all_cold_false | 3/5/3 | 2/0/2 | 60,960/60,960 | 30,720/30,720 | 6,114,345.4 | 6,263,475.8 |
| completed_128_none_cold_false | 0/0/0 | 0/0/0 | 0/0 | 0/0 | 95,255,814.0 | 93,569,388.9 |
| completed_128_half_cold_false | 3/4/3 | 2/0/2 | 30,240/30,240 | 15,360/15,360 | 13,115,594.0 | 14,141,204.9 |
| completed_128_reverse_cold_false | 3/5/3 | 2/0/2 | 60,960/60,960 | 30,720/30,720 | 6,712,003.3 | 7,186,910.6 |
| completed_128_all_cold_true | 3/5/3 | 3/5/3 | 60,960/60,960 | 60,960/60,960 | 6,284,618.3 | 6,199,016.3 |
| speed_common | 128/0/128 | 0/0/0 | 1,024/1,024 | 0/0 | 10,247,846.0 | 29,808,059.7 |
| speed_mods | 128/16/128 | 0/0/0 | 1,280/1,280 | 0/0 | 8,829,608.1 | 22,147,274.5 |
| speed_spaced | 64/0/64 | 0/0/0 | 512/512 | 0/0 | 10,179,242.6 | 18,209,502.6 |
| speed_empty | 0/0/0 | 0/0/0 | 0/0 | 0/0 | 70,681,622.1 | 92,985,244.0 |
| speed_long_plain | 16/96/16 | 0/0/0 | 16,256/16,256 | 0/0 | 850,048.3 | 3,288,111.1 |
| speed_long_spaced | 16/64/16 | 16/0/16 | 3,968/3,968 | 2,048/2,048 | 1,282,183.7 | 2,643,262.8 |
| speed_long_unicode | 16/64/16 | 16/0/16 | 3,968/3,968 | 2,048/2,048 | 972,898.5 | 1,466,996.2 |

Per-run paired cycle savings for representative cases:

- `speed_common`: 65.06%, 64.71%, 66.26%.
- `children_1024_repeat_4_groups_true`: 57.67%, 54.94%, 56.40%.
- `completed_8_all_cold_false`: 32.88%, 36.70%, 30.95%.

Cases with higher median cycle counts:

- `children_0_repeat_1_groups_false`: 4.58% more cycles; elapsed 195.3 -> 204.7 ns/op.
- `children_33_repeat_1_groups_false`: 3.02% more cycles; elapsed 3557.8 -> 3667.2 ns/op.
- `children_4_repeat_128_groups_true`: 2.09% more cycles; elapsed 37920.3 -> 38693.8 ns/op.
- `completed_0_all_cold_false`: 5.83% more cycles; elapsed 50.0 -> 52.3 ns/op.
- `completed_128_none_cold_false`: 1.85% more cycles; elapsed 1343.8 -> 1368.0 ns/op.
- `completed_128_all_cold_true`: 1.39% more cycles; elapsed 20367.2 -> 20648.4 ns/op.
