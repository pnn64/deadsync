# Lua actor-state performance - 0.5.1133

This pass applies `M-HOTPATH`, `M-MEM-REUSE`, and `M-THROUGHPUT` from the
supplied `rust-performance.md` to Lua modchart compilation. These measurements
cover loading/compiler work; they do not establish a change in gameplay FPS.

## Three optimizations

1. **Filter actor-state keys before copying them.** Snapshot scans borrow key
   text and allocate Rust strings only for retained state. Restoration keeps Lua
   string handles while collecting keys to clear. Previously both scans copied
   every string key, including the many methods and unrelated fields discarded
   immediately afterward. Snapshots still own their retained keys and deep-copy
   values. All string keys still undergo UTF-8 validation, and restoration still
   collects keys before changing the table.
2. **Reuse fixed property names.** Position reads and scalar, vector, and string
   setters share static `__songlua_state_*` keys. Additive methods compute their
   fallback lookup key once when the method is created. Unknown property names
   retain the formatted fallback. Scheduled position getter behavior and the
   eager additive fallback lookup (including its errors) are preserved.
3. **Reuse sampling scratch and index captured tracks directly.** Captured
   properties are marked by their stable, append-only track index. This replaces
   a linear search through all captured properties for every existing track,
   reducing membership work from O(tracks * writes) to O(tracks + writes).
   Reset indices, capture flags, and pending message updates retain vector
   capacity across ticks. Unchanged writes still take precedence over messages;
   flags reset after completed scheduled samples have been merged.

All production changes use safe Rust and add no dependencies. Scratch retains
its high-water capacity until compilation ends. Lua tables, owned output, and
new tracks can still allocate. In particular, mlua 0.12 shares visited string
handles using reference-counted allocations: removing discarded Rust key copies
does not make a complete state scan allocation-free. Warmed position/additive
updates have an allocation-free regression check.

## Measurement method

Windows x86-64, Intel Xeon E5-2696 v4, rustc 1.98.0. Baseline production source
is commit `806ec08e4` (0.5.1132). Both executables include the same benchmark
workloads and counting allocator, built with `cargo test --release` (optimization
level 3, full LTO). The baseline executable was saved before production edits.

Each invocation warms up three times and times seven batches. Batches contain
256 state/position operations, 64 dense capture ticks, or three whole compiler
runs. Each executable is invoked three times in alternating order: old/new,
new/old, old/new. Reported results are medians of the three invocation medians.
Builds and other tests finish before timing begins. Timing counters are disabled
during timing and enabled for one separate allocation-counted operation.

Windows `QueryThreadCycleTime` measures calling-thread CPU cycles. Throughput is
useful items per second. Allocated/freed bytes are requested allocation traffic,
including the full requested size of reallocations, not peak live memory or RSS.

State workloads scan 32/256/1,024 unrelated fields plus eight retained integer
fields. Snapshot output is dropped within the measurement; restoration includes
cloning the same eight-entry input snapshot in both versions. Position workloads
perform 96 x/y/z setters or 32 additive x calls. Dense workloads write four
properties on each actor, then call the complete capture routine, including Lua
capture-table reset. Values stay constant after warmup so sample output does not
grow; units count captured properties. Position keys are excluded from dense
workloads to separate them from the position-key optimization.

| Workload | Old us/op | New us/op | Old cycles/op | New cycles/op | Old -> new million items/s |
|---|---:|---:|---:|---:|---:|
| Snapshot 40 keys | 14.223 | 11.779 | 31,153.6 | 25,769.8 | 2.812 -> 3.396 |
| Restore 40 keys | 16.389 | 13.920 | 35,951.7 | 30,496.8 | 2.441 -> 2.873 |
| Snapshot 264 keys | 93.070 | 70.595 | 204,037.2 | 154,658.3 | 2.837 -> 3.740 |
| Restore 264 keys | 96.816 | 76.458 | 211,570.6 | 167,263.3 | 2.727 -> 3.453 |
| Snapshot 1,032 keys | 343.296 | 263.544 | 752,124.5 | 577,688.8 | 3.006 -> 3.916 |
| Restore 1,032 keys | 367.955 | 277.547 | 806,667.7 | 608,353.7 | 2.805 -> 3.718 |
| 96 position setters | 97.636 | 81.334 | 214,057.9 | 178,365.2 | 0.983 -> 1.180 |
| 32 additive calls | 57.041 | 46.819 | 125,049.8 | 102,647.1 | 0.561 -> 0.683 |
| Capture 128 targets | 200.716 | 189.884 | 439,373.8 | 416,196.0 | 0.638 -> 0.674 |
| Capture 512 targets | 838.205 | 776.127 | 1,836,079.8 | 1,697,942.2 | 0.611 -> 0.660 |
| Capture 2,048 targets | 4,055.698 | 3,103.923 | 8,872,910.2 | 6,795,586.2 | 0.505 -> 0.660 |

| Workload | Old alloc / realloc / free calls | New alloc / realloc / free calls | Old requested / freed bytes | New requested / freed bytes |
|---|---:|---:|---:|---:|
| Snapshot 40 keys | 81 / 1 / 81 | 49 / 1 / 49 | 1,856 / 1,856 | 1,600 / 1,600 |
| Restore 40 keys | 90 / 1 / 90 | 50 / 1 / 50 | 2,080 / 2,080 | 1,632 / 1,632 |
| Snapshot 264 keys | 529 / 1 / 529 | 273 / 1 / 273 | 7,388 / 7,388 | 5,184 / 5,184 |
| Restore 264 keys | 538 / 1 / 538 | 274 / 1 / 274 | 7,612 / 7,612 | 5,216 / 5,216 |
| Snapshot 1,032 keys | 2,065 / 1 / 2,065 | 1,041 / 1 / 1,041 | 26,612 / 26,612 | 17,472 / 17,472 |
| Restore 1,032 keys | 2,074 / 1 / 2,074 | 1,042 / 1 / 1,042 | 26,836 / 26,836 | 17,504 / 17,504 |
| 96 position setters | 96 / 0 / 96 | 0 / 0 / 0 | 3,072 / 3,072 | 0 / 0 |
| 32 additive calls | 64 / 0 / 64 | 0 / 0 / 0 | 2,048 / 2,048 | 0 / 0 |
| Capture 128 targets | 162 / 8 / 130 | 160 / 0 / 128 | 6,944 / 5,152 | 2,432 / 640 |
| Capture 512 targets | 642 / 12 / 514 | 640 / 0 / 512 | 28,064 / 20,896 | 9,728 / 2,560 |
| Capture 2,048 targets | 2,563 / 16 / 2,050 | 2,561 / 0 / 2,048 | 112,599 / 83,872 | 38,967 / 10,240 |

At 1,032 keys, snapshot/restore cycles fall about 23-25% and allocation calls
approximately halve. Position setter/additive cycles fall about 17-18%, with no
warmed allocations, reallocations, or frees. At 2,048 captured targets, cycles
fall 23.4%, throughput rises 30.7%, and requested allocation bytes fall 65.4%.
The 128-target timing improvement is small relative to overlapping batch ranges.
Dense capture retains some Lua allocations and defers some frees to Lua GC;
the counted operation therefore does not have equal allocated and freed bytes.

The whole compiler fixture is the existing two-second workload with 128 static
actors, a scheduled tween, a counter that stops updating, and persistent
messages. VM/host setup and teardown are included.

| Metric per complete compilation | Old | New |
|---|---:|---:|
| Median elapsed time | 169.300 ms | 168.404 ms |
| Median calling-thread cycles | 370,244,549 | 368,539,697 |
| Compilations/s | 5.907 | 5.938 |
| Allocation calls | 382,321 | 379,276 |
| Reallocation calls | 2,959 | 2,959 |
| Free calls | 382,321 | 379,276 |
| Requested/freed allocation bytes | 20,166,351 | 20,143,582 |

This mostly static fixture reduces allocation calls by 0.80% and requested bytes
by 0.11%. Its roughly 0.5% timing change is within variation: individual
batch means range from 156.801-182.033 ms before and 164.564-183.740 ms after.
Treat overall loading throughput as unchanged for this workload. The demonstrated
CPU/throughput improvements are in the isolated workloads above.

## Behavior and validation

- Nine new behavior/allocation tests cover deep-copy restoration, unrelated and
  numeric keys, Unicode property names, invalid UTF-8 error atomicity, typed
  setters, scheduled position getters, additive precedence/errors, unchanged
  writes versus messages, flag reset between ticks, new tracks following tween
  completion, scratch reuse, and allocation budgets.
- Focused debug selection: 22 passed, 5 manual tests ignored.
- Full debug Lua suite: baseline 396 passed, 5 failed, 7 ignored; optimized 399
  passed, the same 5 failed, 7 ignored. The three additional passing tests are
  allocation/storage assertions added after the baseline was captured.
- `cargo check --workspace --bins` passes.
- `cargo clippy -p deadsync-song-lua --all-targets -- -D clippy::perf` passes;
  existing non-performance warnings remain. Formatting and `git diff --check`
  pass.

The focused release selection also passes: 22 passed, 5 manual tests ignored.
The complete old/new release compiler snapshots match byte for byte after
canonicalizing track order and fixture path. SHA-256:
`4a1a2ce03995faf82972ceed065055d8c053897d01e23d33d17e3812b5ccb9ce`.

The five existing failures have identical assertions before and after:

- `compile_song_lua_extracts_actorproxy_targets`
- `compile_song_lua_layers_share_init_globals_and_actor_refs`
- `compile_song_lua_resolves_local_and_hidden_screen_proxy_targets`
- `compile_song_lua_runs_cmd_queuecommand_builders`
- `compile_song_lua_supports_notefield_column_api`

## Reproduction

```powershell
cargo test -p deadsync-song-lua --lib _perf -- --test-threads=1
cargo test --release -p deadsync-song-lua --lib actor_state_hot_path_bench -- --ignored --test-threads=1 --nocapture
cargo test --release -p deadsync-song-lua --lib dense_capture_hot_path_bench -- --ignored --test-threads=1 --nocapture
cargo test --release -p deadsync-song-lua --lib sampling_compile_bench -- --ignored --test-threads=1 --nocapture
$env:DEADSYNC_PERF_SNAPSHOT = "$PWD/target/actor-state-output.txt"
cargo test --release -p deadsync-song-lua --lib sampling_compile_snapshot -- --ignored --test-threads=1
```

For baseline comparison, retain the benchmark workloads but restore the changed
production routines to `806ec08e4`. Adapt the dense fixture to the old capture
signature by removing its scratch argument/field; omit the three new allocation
and scratch assertions from the baseline. Save both executables and run them
serially with all builds stopped. Timing is informational; behavior and
allocation assertions are regression gates.
