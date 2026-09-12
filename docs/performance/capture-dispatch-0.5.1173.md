# Lua capture dispatch - 0.5.1173

Parent: `01769fb969fb015cffe71b0b9d095e718d8e1d7d` (`0.5.1172`).
This pass follows the local `rust-performance.md` guidance on measuring hot
paths (M-HOTPATH), avoiding temporary allocations (M-MEM-REUSE), and removing
repeated work (M-THROUGHPUT).

## Three changes

1. **Clone write data only when stateful capture needs it.** Immediate and
   scheduled capture first reject unregistered actors. Only active broadcasts
   clone values and easing strings for the stateful write log. Ordinary
   per-frame writes avoid a redundant shared-color reference increment and,
   for scheduled writes with easing, a redundant string allocation and free.
   The final values, scheduled updates, and active stateful writes retain their
   original ownership, order, and metadata.
2. **Build the broadcast command lookup key once per broadcast.** Property
   capture borrows the scoped Lua `MessageCommand` string directly. Both capture
   paths still look up the actor's current handler on every
   write; handler replacement, removal, metatables, and conversion errors keep
   their behavior. Nested broadcasts save and restore the previous name and
   key together, including after an error. This avoids formatting a Rust string
   and converting it to Lua per property update, including the Lua allocations for long strings. The key is prepared
   before changing broadcast scope. The capture object holds one optional Lua
   string reference; nested broadcasts keep outer keys until they return. Key
   preparation still costs a Lua call; long keys can allocate once per broadcast.
3. **Reset overlay actors directly from the existing slice.** Per-frame
   sampling previously built an index vector and a second vector of cloned
   actor handles before resetting them. Reset now borrows each actor in order.
   It still creates fresh Lua capture-block tables, preserves unrelated
   fields, and stops at the first error with the same partial state changes.
   No scratch capacity or actor handles remain after the reset.

These changes target song-Lua compilation. They do not establish a whole-song
load-time or gameplay frame-rate gain. No dependencies or production unsafe
code were added. Owned output easing strings, output growth, new stateful map
entries, command-cache setup, and Lua work can still allocate.

## Behavior and validation

`capture_dispatch_baseline.rs` freezes six functions from the parent: immediate
and scheduled recording, the two property-capture entry points, broadcast
dispatch, and overlay reset. The source audit permits formatting, visibility,
method-to-free-function receiver changes, and routing the old entry points to
the frozen recording functions. Unchanged helpers are shared. Both versions
run in the same executable and use the current capture structure; the old
functions do not use its new command-cache field.

Seven new tests cover:

- Active/inactive broadcasts, unknown actors, repeated target writes, message
  switches, empty names and easing, retained output capacity, signed-zero
  metadata, and shared vertex-color identity.
- Live handler addition/removal, invalid handler values, metatable lookup,
  unsupported targets, missing-runtime fallback, and the different touched-actor
  behavior of normal and immediate capture when a direct handler suppresses recording.
- Nested broadcasts with success, caught inner errors, uncaught errors, and
  restoration of both the Lua global and the capture name/key. Cross-actor
  writes stay attributed to the correct message and retain their order.
- Reset of empty and populated actor slices, preservation of unrelated fields,
  and errors at the first, middle, or last actor.
- Warm allocation budgets: ordinary scalar/shared-color recording without
  easing and repeated long-name handler lookup have no churn; scheduled easing
  is allocated once for output; reset allocates only the required Lua tables. The reset allocation assertion
  stops Lua GC to isolate new allocations from collection of earlier garbage.

Validation results:

- `cargo test -p deadsync-song-lua --lib capture_dispatch --offline`: **7 passed**,
  one manual benchmark ignored.
- Full song-Lua debug and release tests (`cargo test -p deadsync-song-lua --lib
  --locked`, adding `--release` for release): **431 passed, 5 failed, 12 ignored**
  in each configuration. Both runs have the same five test names and assertion
  text as the recorded parent run in `target/overlay-storage-perf/all.log`.
  The parent was not rebuilt for this pass's full-suite comparison.
- `cargo check --all-targets --offline`: passed for the root workspace targets.
- `cargo clippy -p deadsync-song-lua --all-targets --locked -- -D clippy::perf`:
  passed; song-Lua reports 53 existing nonperformance warnings, none referring to
  the new test files.
- Frozen-source audit, exact +1 manifest/lock version audit, and
  `git diff --check`: passed.

The five existing failures are:

- `compile_song_lua_extracts_actorproxy_targets`: initial visibility assertion.
- `compile_song_lua_layers_share_init_globals_and_actor_refs`: 0 versus 123.
- `compile_song_lua_resolves_local_and_hidden_screen_proxy_targets`: initial
  visibility assertion.
- `compile_song_lua_runs_cmd_queuecommand_builders`: initial visibility assertion.
- `compile_song_lua_supports_notefield_column_api`: `-96:-135` versus `-96:-125`.

All seven new tests pass in both full-suite runs. The existing failures keep
those full-suite commands from returning success.


## Benchmark method

Windows x86-64, Intel Xeon E5-2696 v4 @ 2.20 GHz, Rust 1.98.0
(`88d9e12ae`, LLVM 22.1.8), release optimization with full LTO.
`tests/support/perf.rs` takes seven timing samples after three warmups and
counts allocations in a separate operation. Three isolated executable runs
alternate old/new order (old first, new first, old first). No Cargo build runs
during measurement. Results use medians of the three per-run medians.

Windows `QueryThreadCycleTime` measures calling-thread cycles, not retired
instructions. The thread-local allocator wrapper counts allocation,
reallocation, and free calls plus cumulative requested/freed bytes. In this
vendored Lua 5.4 / mlua 0.12.1 build, mlua's allocator calls Rust's allocator,
so those Lua allocations are included. Other threads and allocations bypassing
Rust's allocator are outside the counters. Byte totals are not peak live
memory, retained capacity, or process RSS. Lua's normal GC remains enabled in
the benchmarks; it can free objects created by earlier operations.

Fixtures, Lua states, actors, command functions, shared color arrays, message
source strings, and the normal song-runtime table are prepared outside timing.
Both implementations use the same workloads. Each operation includes the stated output cleanup:

- `record_*` and `scheduled_*`: one or 128 calls through the capture object.
  Each batch drains the previous output and clears retained stateful write
  vectors, then writes new values. The Lua app-data borrow, input value clones,
  owned input easing construction, and prior output destruction are included.
  Output vector/map capacity is retained. Unknown-actor throughput counts
  attempted writes; active capture is a control that must still own its log.
- `lookup_*`: 128 calls through the real property-capture entry point under a
  preexisting broadcast key. Cases vary key length, handler presence, and
  immediate versus normal capture. Output is drained at each batch. These
  isolate per-write lookup and exclude broadcast setup. Throughput is writes.
- `broadcast_*`: actual broadcast dispatch through a Lua callback performing
  zero, one, 16, or 128 property writes to its own actor. Includes command
  formatting, cache creation/restoration, actor dispatch, and runtime broadcast
  recording/cleanup. Throughput is writes, or broadcasts for the zero-write
  control. These expose the cache's setup cost as well as its amortized gain.
- `reset_*`: complete reset calls over zero, one, 16, or 128 actors. Both
  versions allocate fresh Lua block tables. The old vectors and cloned handles
  are created and destroyed inside timing. Throughput is actors, or reset
  calls for the empty control.

Recording uses 512 operations per timing sample; lookup, broadcast, and reset
use 256. Comparisons check output before timing, while regression tests cover
reset state and nested/error behavior independently. All 22 scenarios are
reported, including controls with higher cycle counts. Empty-input timings
are dominated by measurement/loop overhead and are not representative gains.

Reproduce:

```powershell
cargo test -p deadsync-song-lua --lib capture_dispatch --locked
cargo test -p deadsync-song-lua --lib --release --locked
# Use the executable path printed by Cargo. Run three times, toggling
# DEADSYNC_PERF_REVERSE=1 for the middle run; leave it unset for runs 1 and 3.
& <release-test-executable> capture_dispatch_bench --ignored --test-threads=1 --nocapture
```

Measured effects on representative workloads:

- `scheduled_easing_128`: 31.49% fewer cycles; 23.162 -> 15.899 us/op; allocation calls 256 -> 128, requested bytes 2,560 -> 1,280.
- `lookup_128_name_256_handler_true_immediate_true`: 82.97% fewer cycles; 105.049 -> 17.885 us/op; allocation calls 256 -> 0, requested bytes 136,064 -> 0.
- `broadcast_128_writes`: 58.10% fewer cycles; 46.038 -> 19.268 us/op; allocation calls 137 -> 9, requested bytes 3,891 -> 435.
- `reset_128_actors`: 2.59% fewer cycles; 123.660 -> 120.423 us/op; allocation calls 130 -> 128, requested bytes 16,256 -> 7,168.

The tables include the setup-cost and empty-input controls. A gain in these
fixtures does not establish a gain for every Lua program or CPU.

## Results

Positive cycle savings indicate an improvement on this workload and machine.

| Scenario | Old us/op | New us/op | Old cycles/op | New cycles/op | Fewer cycles |
|---|---:|---:|---:|---:|---:|
| record_scalar_1 | 0.1518 | 0.1426 | 335.3 | 315.1 | 6.02% |
| record_scalar_128 | 8.9467 | 7.7176 | 19,621.7 | 16,911.4 | 13.81% |
| record_colors_128 | 11.1264 | 9.4387 | 24,398.8 | 20,696.9 | 15.17% |
| scheduled_plain_128 | 9.7762 | 8.6238 | 21,443.7 | 18,915.6 | 11.79% |
| scheduled_easing_1 | 0.2422 | 0.1891 | 533.7 | 417.6 | 21.75% |
| scheduled_easing_128 | 23.1623 | 15.8992 | 50,791.5 | 34,797.6 | 31.49% |
| scheduled_long_colors_128 | 29.0992 | 19.5672 | 63,790.5 | 42,893.4 | 32.76% |
| scheduled_active_128 | 34.1459 | 34.6570 | 74,837.5 | 75,992.0 | -1.54% |
| scheduled_unknown_128 | 25.1762 | 14.0320 | 55,198.7 | 30,774.2 | 44.25% |
| lookup_128_name_8_handler_true_immediate_true | 42.6930 | 15.3438 | 93,571.3 | 33,626.4 | 64.06% |
| lookup_128_name_8_handler_true_immediate_false | 48.9109 | 20.6383 | 107,199.2 | 45,264.2 | 57.78% |
| lookup_128_name_256_handler_true_immediate_true | 105.0488 | 17.8852 | 230,295.8 | 39,223.6 | 82.97% |
| lookup_128_name_8_handler_false_immediate_true | 97.6051 | 67.7195 | 213,949.9 | 148,467.8 | 30.61% |
| lookup_128_name_8_handler_false_immediate_false | 138.3500 | 106.4188 | 303,209.2 | 233,249.6 | 23.07% |
| broadcast_0_writes | 3.5449 | 3.6289 | 7,768.2 | 7,953.4 | -2.38% |
| broadcast_1_writes | 4.0418 | 3.8809 | 8,878.6 | 8,505.6 | 4.20% |
| broadcast_16_writes | 9.0000 | 5.6230 | 19,720.7 | 12,330.6 | 37.47% |
| broadcast_128_writes | 46.0375 | 19.2680 | 100,876.5 | 42,262.3 | 58.10% |
| reset_0_actors | 0.0094 | 0.0004 | 25.7 | 6.0 | 76.65% |
| reset_1_actors | 1.0918 | 0.9402 | 2,401.6 | 2,069.0 | 13.85% |
| reset_16_actors | 15.4926 | 14.8672 | 33,983.1 | 32,609.5 | 4.04% |
| reset_128_actors | 123.6598 | 120.4227 | 270,989.9 | 263,971.0 | 2.59% |

A/R/F means allocation/reallocation/free calls. Bytes include reallocations and common input/output ownership costs.

| Scenario | Old A/R/F | New A/R/F | Old requested/freed B | New requested/freed B | Old units/s | New units/s |
|---|---:|---:|---:|---:|---:|---:|
| record_scalar_1 | 0/0/0 | 0/0/0 | 0/0 | 0/0 | 6,589,446.6 | 7,013,698.6 |
| record_scalar_128 | 0/0/0 | 0/0/0 | 0/0 | 0/0 | 14,306,983.6 | 16,585,514.0 |
| record_colors_128 | 0/0/0 | 0/0/0 | 0/0 | 0/0 | 11,504,204.2 | 13,561,230.0 |
| scheduled_plain_128 | 0/0/0 | 0/0/0 | 0/0 | 0/0 | 13,093,059.5 | 14,842,596.4 |
| scheduled_easing_1 | 2/0/2 | 1/0/1 | 20/20 | 10/10 | 4,129,032.3 | 5,289,256.2 |
| scheduled_easing_128 | 256/0/256 | 128/0/128 | 2,560/2,560 | 1,280/1,280 | 5,526,220.4 | 8,050,710.0 |
| scheduled_long_colors_128 | 256/0/256 | 128/0/128 | 65,536/65,536 | 32,768/32,768 | 4,398,743.5 | 6,541,563.5 |
| scheduled_active_128 | 256/0/256 | 256/0/256 | 2,560/2,560 | 2,560/2,560 | 3,748,620.1 | 3,693,334.2 |
| scheduled_unknown_128 | 256/0/256 | 128/0/128 | 2,560/2,560 | 1,280/1,280 | 5,084,172.5 | 9,121,986.5 |
| lookup_128_name_8_handler_true_immediate_true | 128/128/128 | 0/0/0 | 3,840/3,840 | 0/0 | 2,998,151.8 | 8,342,158.9 |
| lookup_128_name_8_handler_true_immediate_false | 128/128/128 | 0/0/0 | 3,840/3,840 | 0/0 | 2,617,001.6 | 6,202,066.9 |
| lookup_128_name_256_handler_true_immediate_true | 256/128/211 | 0/0/0 | 136,064/122,789 | 0/0 | 1,218,481.0 | 7,156,772.8 |
| lookup_128_name_8_handler_false_immediate_true | 128/128/128 | 0/0/0 | 3,840/3,840 | 0/0 | 1,311,407.2 | 1,890,148.9 |
| lookup_128_name_8_handler_false_immediate_false | 128/128/128 | 0/0/0 | 3,840/3,840 | 0/0 | 925,189.7 | 1,202,795.6 |
| broadcast_0_writes | 9/1/6 | 9/1/6 | 435/237 | 435/237 | 282,093.7 | 275,565.1 |
| broadcast_1_writes | 10/2/7 | 9/1/6 | 462/264 | 435/237 | 247,414.7 | 257,674.9 |
| broadcast_16_writes | 25/17/22 | 9/1/6 | 867/669 | 435/237 | 1,777,777.8 | 2,845,432.4 |
| broadcast_128_writes | 137/129/134 | 9/1/6 | 3,891/3,693 | 435/237 | 2,780,342.1 | 6,643,149.7 |
| reset_0_actors | 0/0/0 | 0/0/0 | 0/0 | 0/0 | 106,666,666.7 | 2,560,000,000.0 |
| reset_1_actors | 3/0/2 | 1/0/0 | 192/136 | 56/0 | 915,921.3 | 1,063,564.6 |
| reset_16_actors | 18/2/2 | 16/0/0 | 1,920/1,024 | 896/0 | 1,032,752.6 | 1,076,195.5 |
| reset_128_actors | 130/5/2 | 128/0/0 | 16,256/9,088 | 7,168/0 | 1,035,098.2 | 1,062,922.9 |

Per-run paired cycle savings for representative cases:

- `scheduled_easing_128`: 26.93%, 32.37%, 31.38%.
- `broadcast_128_writes`: 58.13%, 57.51%, 58.16%.
- `reset_128_actors`: 2.15%, 3.13%, 6.38%.

Cases with higher median cycle counts:

- `scheduled_active_128`: 1.54% more cycles; elapsed 34145.9 -> 34657.0 ns/op.
- `broadcast_0_writes`: 2.38% more cycles; elapsed 3544.9 -> 3628.9 ns/op.
