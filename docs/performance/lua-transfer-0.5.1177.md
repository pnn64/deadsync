# Lua command calls and table construction - 0.5.1177

Parent: `4c7312013a6143518f0ea5088c046e56afb7c8a4` (`0.5.1176`).
This pass applies `rust-performance.md` guidance on measuring hot paths
(M-HOTPATH), avoiding temporary ownership (M-MEM-REUSE), and reserving known
collection sizes (M-INITIAL-CAPACITY).

## Three changes

1. **Pass command arguments directly to Lua.** `call_actor_function` uses a
   one- or two-element tuple with a borrowed actor table, removing the temporary
   `MultiValue` buffer and actor-handle clone. Omitted parameters still pass one
   argument; explicit `nil` passes two. Script-directory lookup, scoped changes
   and restoration, error propagation, and ignored return values are preserved.
2. **Build semantic snapshot tables directly.** The capture-scope snapshot path
   filters actor entries and deep-copies retained values straight into its Lua
   result. It no longer materializes a Rust vector with owned string keys, then
   converts those keys back into Lua strings. Two-slot snapshot entries reserve
   their size. All string keys are still validated, including excluded keys.
   Traversal order and the existing value-copy routine remain unchanged. The
   public Rust-vector snapshot APIs retain their original implementations.
3. **Reserve fixed-size color and vector arrays.** Twelve constructors/setters
   reserve the known two-to-five array slots before filling a Lua table. This
   covers colors, per-vertex colors, capture vectors, immediate effect vectors,
   size and stretch rectangles. Fresh tables and their contents, write order,
   capture updates, and error behavior are preserved. Earlier aliases continue
   to reference their original values.

These paths are used during song-Lua command execution and capture. No new
cache, dependency, public export, or production unsafe code was added. The
changes remove transient ownership and table growth; Lua-owned output tables
and deep copies still allocate. Whole-song load time and gameplay FPS were not
measured in this pass.

## Behavior and validation

Seventeen parent functions are frozen in test-only baseline modules. A source
audit verifies their bodies and the color helper's original inlining attribute.
Only the intended production functions changed; shared capture, restoration,
script-directory, and value-copy helpers and public exports are unchanged.

Ten new tests cover:

- Exact command argument counts and actor/parameter identity for omitted, nil,
  boolean, numeric, table and invalid-UTF-8 string parameters; discarded return
  values; callback errors; empty/Unicode script directories; lookup errors
  before execution; and nested directory restoration on success and failure.
- Snapshot order, numeric bit patterns, nonfinite values, string bytes,
  numeric/boolean keys, retained/excluded prefixes, raw table access, deep-copy
  isolation, restoration with earlier aliases alive, and invalid UTF-8 even in
  excluded keys. Fixtures vary from no retained fields to 64 retained fields
  and 256 ignored fields, including nested tables.
- Color components and independent table identity; capture blocks and state;
  active overlay updates; nonfinite inputs; previous table aliases; and partial
  errors with the same observable write order for all ten affected setters.
- Zero allocation churn for command calls with omitted/nil/numeric parameters
  after interning the directory lookup key. The actor's Rust handle is still
  unique during this check. First-use Lua string interning is a setup cost.

Validation:

- Fresh parent debug suite before edits: **461 passed, 5 failed, 21 ignored**.
- New targeted tests: **10 passed**, three manual benchmarks ignored.
- New full debug and release suites: **471 passed, 5 failed, 24 ignored** each.
  Both have identical failure names and assertion text to the fresh parent
  debug run. The parent release suite was not rebuilt in this pass.
- Root `cargo check --all-targets --offline`: passed.
- `cargo clippy -p deadsync-song-lua --all-targets --locked -- -D clippy::perf`:
  passed, with 53 existing nonperformance warnings and none in the new tests.
- Frozen-source/shared-helper/public-export audit, exact +1 version/lock audit,
  and staged whitespace check: passed.

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
The existing helper takes seven timing samples after three warmups, then counts
allocation churn in a separate operation. Three isolated serial executable runs
alternate old/new order: old first, new first, old first. No Cargo compiler runs
during measurement. Tables aggregate the median of the three per-run medians.
All 36 paired cases are reported. Allocation/reallocation/free counts and bytes
agree across all three runs.

Windows `QueryThreadCycleTime` counts calling-thread CPU cycles, not retired
instructions. Thread-local counters cover allocations routed through Rust's
System allocator, including Lua allocations in this mlua 0.12.1 / vendored Lua
5.4 build. Allocations on other threads or bypassing that allocator are outside
the counters. Requested/freed bytes are cumulative, not peak live memory or RSS.
Fixtures and lookup keys are warmed; Lua garbage collection is stopped during
measurement. Lua tables remain owned by the VM after their Rust handles drop,
so their reclamation is outside timing. Allocation and free totals therefore
do not always balance. Lua setup, fixture creation, and teardown are excluded.

Operation boundaries and throughput units:

- **Commands:** 64 actual Lua calls/op, 128 operations/sample. Cases cover
  absent, explicit nil, numeric and table parameters, with and without a script
  directory. Work includes directory handling, parameter-handle cloning where
  needed, argument passing, two actor-field writes inside the Lua callback,
  and discarded returns. Throughput is calls/s. Shared callback fields and
  parameter values are checked before timing.
- **Snapshots:** one complete snapshot/op, 32 operations/sample. Work includes
  actor iteration, validation, retained key/value handling, nested deep copies,
  construction of the Lua result, and dropping its Rust handle. Throughput is
  snapshots/s. Names specify ignored/retained fields and nested value presence.
  No-kept-field cases are controls. Both implementations see the same actor and
  are compared before timing.
- **Color constructors:** 16 tables/op, 64 operations/sample. The nested case
  builds 16 outer vertex-color tables plus their 64 inner color tables. The
  throughput unit is one complete constructor result. Creation, field writes,
  and handle drops are timed.
- **Capture arrays:** 16 setter calls/op, 64 operations/sample. Each of the ten
  setters is measured with active overlay-update recording enabled and disabled.
  Work includes fresh vector/color tables, state and capture writes, and update
  recording. Existing capture blocks and recording buffers are warmed and
  reused as in repeated setter calls. Active cases repeatedly replace the same
  target; they do not grow the target list. Throughput is setter calls/s.

The frozen and new functions compile in the same binary with identical
dependencies. Inputs are black-boxed and results/state consumed. Measurements
remain sensitive to process ordering and machine noise; allocation changes are
more deterministic than small timing differences.

## Results

Positive cycle savings indicate an improvement on this workload and machine.

| Scenario | Old us/op | New us/op | Old cycles/op | New cycles/op | Fewer cycles |
|---|---:|---:|---:|---:|---:|
| color_nested_false | 12.9234 | 7.0766 | 28,233.2 | 15,563.9 | 44.87% |
| color_nested_true | 67.5297 | 37.2531 | 147,106.2 | 81,455.1 | 44.63% |
| array_color_active_false | 25.8625 | 18.1328 | 57,231.2 | 39,753.5 | 30.54% |
| array_vec2_active_false | 17.6547 | 15.3922 | 38,776.0 | 33,724.1 | 13.03% |
| array_vec3_active_false | 20.4312 | 15.9125 | 44,798.6 | 34,955.4 | 21.97% |
| array_vec4_active_false | 20.1188 | 15.2891 | 44,201.8 | 33,501.2 | 24.21% |
| array_vec5_active_false | 27.0422 | 15.0000 | 59,319.9 | 32,839.3 | 44.64% |
| array_size_active_false | 15.7609 | 15.2531 | 34,633.0 | 33,504.6 | 3.26% |
| array_stretch_active_false | 46.2125 | 43.3469 | 101,313.0 | 95,122.4 | 6.11% |
| array_immediate3_active_false | 21.6047 | 17.2906 | 47,364.0 | 37,901.5 | 19.98% |
| array_immediate4_active_false | 22.5984 | 17.6172 | 49,555.6 | 38,697.2 | 21.91% |
| array_immediate5_active_false | 25.3844 | 20.0656 | 55,653.5 | 44,078.3 | 20.80% |
| array_color_active_true | 27.7766 | 22.5891 | 61,003.9 | 49,620.7 | 18.66% |
| array_vec2_active_true | 21.6609 | 19.3031 | 47,494.3 | 42,404.7 | 10.72% |
| array_vec3_active_true | 33.7047 | 22.9703 | 73,772.6 | 50,365.0 | 31.73% |
| array_vec4_active_true | 25.2406 | 20.4422 | 55,437.5 | 44,894.6 | 19.02% |
| array_vec5_active_true | 30.0484 | 21.5344 | 65,904.9 | 47,295.4 | 28.24% |
| array_size_active_true | 20.9781 | 18.6422 | 46,064.1 | 40,950.5 | 11.10% |
| array_stretch_active_true | 65.5953 | 58.6703 | 143,666.2 | 128,428.1 | 10.61% |
| array_immediate3_active_true | 19.2562 | 13.9688 | 42,212.6 | 30,678.5 | 27.32% |
| array_immediate4_active_true | 20.1031 | 14.8797 | 44,074.9 | 32,602.6 | 26.03% |
| array_immediate5_active_true | 24.8094 | 19.6578 | 54,391.4 | 42,932.8 | 21.07% |
| command_none_dir_false | 28.1727 | 17.3758 | 61,785.8 | 38,110.7 | 38.32% |
| command_nil_dir_false | 27.6469 | 18.0930 | 60,669.5 | 39,693.5 | 34.57% |
| command_number_dir_false | 26.3648 | 17.8195 | 57,822.8 | 39,100.2 | 32.38% |
| command_table_dir_false | 27.9703 | 18.4820 | 61,379.4 | 40,530.3 | 33.97% |
| command_none_dir_true | 55.6039 | 44.6633 | 121,885.9 | 97,979.3 | 19.61% |
| command_nil_dir_true | 57.0312 | 46.2617 | 125,077.3 | 101,345.6 | 18.97% |
| command_number_dir_true | 57.0484 | 47.5414 | 125,039.5 | 104,281.4 | 16.60% |
| command_table_dir_true | 56.1594 | 48.1789 | 123,204.7 | 105,648.1 | 14.25% |
| snapshot_ignored_0_kept_0_nested_false | 0.1719 | 0.1656 | 418.4 | 425.3 | -1.65% |
| snapshot_ignored_128_kept_0_nested_false | 28.2000 | 27.7594 | 61,947.0 | 60,979.8 | 1.56% |
| snapshot_ignored_64_kept_4_nested_false | 17.2719 | 16.1281 | 37,740.3 | 35,449.2 | 6.07% |
| snapshot_ignored_64_kept_16_nested_false | 28.7594 | 23.7500 | 62,969.1 | 52,179.3 | 17.14% |
| snapshot_ignored_256_kept_64_nested_false | 115.6906 | 105.0281 | 251,890.0 | 227,175.6 | 9.81% |
| snapshot_ignored_64_kept_16_nested_true | 43.7250 | 39.9938 | 95,852.9 | 87,868.6 | 8.33% |

A/R/F means allocation/reallocation/free calls. Bytes include reallocations and common input/output ownership costs.

| Scenario | Old A/R/F | New A/R/F | Old requested/freed B | New requested/freed B | Old units/s | New units/s |
|---|---:|---:|---:|---:|---:|---:|
| color_nested_false | 32/32/0 | 32/0/0 | 2,688/768 | 1,920/0 | 1,238,060.7 | 2,260,984.8 |
| color_nested_true | 160/160/0 | 160/0/0 | 13,440/3,840 | 9,600/0 | 236,932.8 | 429,494.2 |
| array_color_active_false | 48/32/16 | 48/0/16 | 2,944/1,024 | 2,176/256 | 618,656.4 | 882,378.3 |
| array_vec2_active_false | 48/16/16 | 48/0/16 | 1,920/512 | 1,664/256 | 906,274.9 | 1,039,488.4 |
| array_vec3_active_false | 48/32/16 | 48/0/16 | 2,944/1,024 | 1,920/256 | 783,114.1 | 1,005,498.8 |
| array_vec4_active_false | 48/32/16 | 48/0/16 | 2,944/1,024 | 2,176/256 | 795,278.0 | 1,046,499.7 |
| array_vec5_active_false | 48/48/16 | 48/0/16 | 4,992/2,048 | 2,432/256 | 591,668.1 | 1,066,666.7 |
| array_size_active_false | 48/16/16 | 48/0/16 | 1,920/512 | 1,664/256 | 1,015,168.0 | 1,048,965.4 |
| array_stretch_active_false | 48/32/16 | 48/0/16 | 2,944/1,024 | 2,176/256 | 346,226.7 | 369,115.4 |
| array_immediate3_active_false | 48/32/16 | 48/0/16 | 2,944/1,024 | 1,920/256 | 740,580.0 | 925,356.9 |
| array_immediate4_active_false | 48/32/16 | 48/0/16 | 2,944/1,024 | 2,176/256 | 708,013.6 | 908,204.0 |
| array_immediate5_active_false | 48/48/16 | 48/0/16 | 4,992/2,048 | 2,432/256 | 630,309.0 | 797,383.6 |
| array_color_active_true | 48/32/16 | 48/0/16 | 2,944/1,024 | 2,176/256 | 576,025.2 | 708,307.4 |
| array_vec2_active_true | 32/16/0 | 32/0/0 | 1,664/256 | 1,408/0 | 738,656.9 | 828,881.3 |
| array_vec3_active_true | 32/32/0 | 32/0/0 | 2,688/768 | 1,664/0 | 474,711.4 | 696,551.3 |
| array_vec4_active_true | 32/32/0 | 32/0/0 | 2,688/768 | 1,920/0 | 633,898.7 | 782,695.1 |
| array_vec5_active_true | 32/48/0 | 32/0/0 | 4,736/1,792 | 2,176/0 | 532,473.6 | 742,998.1 |
| array_size_active_true | 32/16/0 | 32/0/0 | 1,664/256 | 1,408/0 | 762,699.2 | 858,268.4 |
| array_stretch_active_true | 32/32/0 | 32/0/0 | 2,688/768 | 1,920/0 | 243,919.9 | 272,710.3 |
| array_immediate3_active_true | 32/32/0 | 32/0/0 | 2,688/768 | 1,664/0 | 830,899.1 | 1,145,413.9 |
| array_immediate4_active_true | 32/32/0 | 32/0/0 | 2,688/768 | 1,920/0 | 795,896.2 | 1,075,291.4 |
| array_immediate5_active_true | 32/48/0 | 32/0/0 | 4,736/1,792 | 2,176/0 | 644,917.5 | 813,925.8 |
| command_none_dir_false | 64/0/64 | 0/0/0 | 10,240/10,240 | 0/0 | 2,271,706.3 | 3,683,287.6 |
| command_nil_dir_false | 64/0/64 | 0/0/0 | 10,240/10,240 | 0/0 | 2,314,909.0 | 3,537,285.7 |
| command_number_dir_false | 64/0/64 | 0/0/0 | 10,240/10,240 | 0/0 | 2,427,475.0 | 3,591,564.7 |
| command_table_dir_false | 64/0/64 | 0/0/0 | 10,240/10,240 | 0/0 | 2,288,140.3 | 3,462,822.8 |
| command_none_dir_true | 128/0/128 | 64/0/64 | 11,264/11,264 | 1,024/1,024 | 1,150,998.3 | 1,432,944.4 |
| command_nil_dir_true | 128/0/128 | 64/0/64 | 11,264/11,264 | 1,024/1,024 | 1,122,191.8 | 1,383,433.3 |
| command_number_dir_true | 128/0/128 | 64/0/64 | 11,264/11,264 | 1,024/1,024 | 1,121,853.7 | 1,346,194.9 |
| command_table_dir_true | 128/0/128 | 64/0/64 | 11,264/11,264 | 1,024/1,024 | 1,139,613.8 | 1,328,382.2 |
| snapshot_ignored_0_kept_0_nested_false | 1/0/0 | 1/0/0 | 56/0 | 56/0 | 5,818,181.8 | 6,037,735.8 |
| snapshot_ignored_128_kept_0_nested_false | 129/0/128 | 129/0/128 | 2,104/2,048 | 2,104/2,048 | 35,461.0 | 36,023.9 |
| snapshot_ignored_64_kept_4_nested_false | 83/6/73 | 78/2/68 | 2,020/1,548 | 1,608/1,136 | 57,897.6 | 62,003.5 |
| snapshot_ignored_64_kept_16_nested_false | 131/22/97 | 114/4/80 | 5,662/3,942 | 3,240/1,520 | 34,771.3 | 42,105.3 |
| snapshot_ignored_256_kept_64_nested_false | 515/74/385 | 450/6/320 | 23,326/16,614 | 12,840/6,128 | 8,643.7 | 9,521.3 |
| snapshot_ignored_64_kept_16_nested_true | 195/38/113 | 178/20/96 | 7,710/4,454 | 5,288/2,032 | 22,870.2 | 25,003.9 |

Per-run paired cycle savings for representative cases:

- `command_none_dir_false`: 35.68%, 38.32%, 34.65%.
- `snapshot_ignored_64_kept_16_nested_false`: 17.76%, 16.08%, 18.74%.
- `color_nested_true`: 44.63%, 42.75%, 45.51%.

Cases with higher median cycle counts:

- `snapshot_ignored_0_kept_0_nested_false`: 1.65% more cycles; elapsed 171.9 -> 165.6 ns/op.


## Interpretation and limits

**Command calls:** all four no-directory cases eliminate 64 allocations/frees
and 10,240 requested/freed bytes per 64 calls (160 bytes per call here). Cycle
savings range from 32.38% to 38.32%; the no-parameter case rises from 2.27 to
3.68 million calls/s. With a script directory, cycle savings are 14.25-19.61%
and allocation/free calls fall from 128 to 64. The remaining allocation is the
existing owned directory string. A first lookup can also intern its Lua key;
that cold setup cost is excluded from the warmed measurements. Lua callbacks
with their own allocating argument decoders or bodies can still allocate.

**Semantic snapshots:** the 64-ignored/16-retained scalar case uses 17.14%
fewer cycles and improves from 34,771 to 42,105 snapshots/s. Allocations drop
from 131 to 114, reallocations from 22 to 4, and requested bytes from 5,662 to
3,240. The removed work is the intermediate Rust collection and owned keys,
plus growth of two-slot Lua entries. With 64 retained fields, requested bytes
fall from 23,326 to 12,840 and cycles by 9.81%. Nested deep-copy work remains;
the nested 16-field case improves by 8.33%. Iterating/validating Lua string keys
still incurs shared-handle costs, even for ignored fields, and output tables
must still be allocated.

**Fixed arrays:** plain and vertex-color constructors use 44.87% and 44.63%
fewer cycles. For 16 vertex-color results, all 160 growth reallocations are
removed and cumulative requested bytes fall from 13,440 to 9,600. The 160
initial allocations remain because the VM still creates fresh outer/inner
tables and array storage. The ten setter cases improve by 3.26-44.64% with
capture blocks and 10.61-31.73% with active update recording. Every measured
fixed-array case eliminates its growth reallocations. Exact three- and
five-element reservations also avoid the previous spare array slots. State
handle ownership, callback recording, and fresh Lua output still have costs.

The empty snapshot control has unchanged allocation counts and bytes, with
1.65% more aggregate thread cycles despite lower elapsed time (171.9 to 165.6
ns/op). This tiny case is near measurement overhead; no improvement is claimed
for empty snapshots. All other aggregate cycle medians improve, but individual
runs still vary. The smaller timing gains should be interpreted cautiously.
Allocation counters agree across all three runs. No whole-song profile with
normal garbage collection, hardware instruction count, or peak-memory/RSS
measurement was performed; those are needed to quantify application-wide gains.


## Reproduce

```powershell
cargo test -p deadsync-song-lua --lib lua_transfer --locked
cargo test -p deadsync-song-lua --lib --locked
cargo test -p deadsync-song-lua --lib --release --locked
cargo check --all-targets --offline
cargo clippy -p deadsync-song-lua --all-targets --locked -- -D clippy::perf
cargo test -p deadsync-song-lua --lib --release --locked lua_transfer_bench -- --ignored --test-threads=1 --nocapture
$env:DEADSYNC_PERF_REVERSE = '1'
cargo test -p deadsync-song-lua --lib --release --locked lua_transfer_bench -- --ignored --test-threads=1 --nocapture
Remove-Item Env:DEADSYNC_PERF_REVERSE
cargo test -p deadsync-song-lua --lib --release --locked lua_transfer_bench -- --ignored --test-threads=1 --nocapture
```

The recorded runs invoke the built release executable directly after all
compiler work ends. Raw logs and machine-readable results are stored locally
under `target/lua-transfer-perf/`. Frozen baselines, fixtures, and this report are
committed so the measurements can be reproduced without those ignored logs.
