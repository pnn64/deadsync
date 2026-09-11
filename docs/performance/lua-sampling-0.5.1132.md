# Lua sampling performance — 0.5.1132

This pass reduces allocation traffic during modchart compilation. It applies
`M-HOTPATH`, `M-MEM-REUSE`, and `M-THROUGHPUT` from the supplied
`rust-performance.md`. These are compiler/loading improvements; the measurements
do not establish a change in gameplay FPS.

## Three changes

1. **Reuse actor-state snapshots.** `compile_update_functions` keeps its current,
   message-replayed, and updated state vectors for the compiler run. Copying into
   existing slices and swapping the current/replay buffers removes two full-vector
   allocations per subsequent sample tick. Overlay count is fixed by the input
   slice. The copies remain necessary to preserve independent prior/update state.
2. **Keep unfinished tweens in place.**
   `merge_completed_scheduled_overlay_samples` extracts only completed entries.
   Previously, every pending tick rebuilt the entire pending vector, repeatedly
   growing and freeing it even when no tween had completed. The pending vector
   retains its high-water capacity until compilation ends. Completing tweens can
   still allocate output storage.
3. **Compact sample tracks in place.** `sort_overlay_update_samples` avoids the
   stable sort's temporary allocation when input is already ordered, and removes
   duplicates in the existing vector. The last value **and timestamp** in an
   epsilon-connected run survive. Unordered input still uses a stable sort, which
   can allocate. Dropping superseded owned sample values can still free memory.

All production changes use safe Rust. There are no new dependencies, runtime
switches, global caches, or changes to the sampling frequency.

## Measurements

Windows x86-64, Intel Xeon E5-2696 v4, rustc 1.98.0. Both executables use
`cargo test --release` with the repository's optimization level 3 and full LTO.
The baseline is application source from `e49efcf95`, with the same benchmark
harness and the stale multitap test repair described below.

Each invocation warms up three times, then measures seven batches. Each batch
runs 512 microbenchmark operations or three complete compilations. Each binary
was invoked three times, alternating old/new, new/old, old/new. Values below are
medians of the three invocation medians. Builds and other tests had finished
before these comparisons ran.

The counting allocator wrapper is present in both executables. Counters are
disabled during timing, then enabled for one additional operation. Bytes are
requested allocation traffic, including the full requested size of reallocations,
not peak live memory or process RSS. Frees and freed bytes equal allocations and
allocated bytes in these workloads. Windows cycle counts use
`QueryThreadCycleTime` for the calling thread; other platforms report zero for
this unavailable metric.

| Workload | Old ns/op | New ns/op | Old cycles/op | New cycles/op | Old → new million items/s |
|---|---:|---:|---:|---:|---:|
| Compact 16 ordered samples | 361.3 | 104.1 | 796.1 | 231.1 | 44.3 → 153.7 |
| Compact 256 ordered samples | 2,338.7 | 1,647.9 | 5,136.4 | 3,612.7 | 109.5 → 155.3 |
| Compact 4,096 ordered samples | 39,309.8 | 31,542.2 | 86,042.7 | 69,005.3 | 104.2 → 129.9 |
| Check 16 pending tweens | 691.2 | 18.0 | 1,504.8 | 42.0 | 23.1 → 888.9 |
| Check 256 pending tweens | 5,593.2 | 173.6 | 12,051.1 | 383.7 | 45.8 → 1,474.7 |
| Check 4,096 pending tweens | 84,553.1 | 10,565.2 | 185,144.4 | 23,104.1 | 48.4 → 387.7 |

Compaction includes restoring the original input into a warmed vector and uses
two samples per timestamp. Pending checks call the production completion routine
with future endpoints; each item counted is one inspected tween.

| Workload | Old allocations / reallocations / frees per op | Old requested bytes/op | New allocations / reallocations / frees; bytes |
|---|---:|---:|---:|
| Compact 16 | 1 / 0 / 1 | 512 | 0 / 0 / 0; 0 |
| Compact 256 | 2 / 0 / 2 | 16,384 | 0 / 0 / 0; 0 |
| Compact 4,096 | 2 / 0 / 2 | 262,144 | 0 / 0 / 0; 0 |
| Pending 16 | 1 / 2 / 1 | 3,360 | 0 / 0 / 0; 0 |
| Pending 256 | 1 / 6 / 1 | 60,960 | 0 / 0 / 0; 0 |
| Pending 4,096 | 1 / 10 / 1 | 982,560 | 0 / 0 / 0; 0 |

The complete two-second compiler fixture contains 128 static actors, a sparse
tween, a dense counter that stops updating, and an actor receiving persistent
messages. It includes Lua VM/host setup, compilation, and teardown, so it measures
the combined effect rather than isolating snapshot-copy timing:

| Metric per complete compilation | Old | New |
|---|---:|---:|
| Median elapsed time | 160.923 ms | 158.900 ms |
| Median calling-thread cycles | 352,054,369 | 347,538,306 |
| Compilations/s | 6.214 | 6.293 |
| Allocation calls | 382,657 | 382,321 |
| Reallocation calls | 2,959 | 2,959 |
| Free calls | 382,657 | 382,321 |
| Requested/freed allocation bytes | 35,993,004 | 20,166,324 |

Allocation traffic falls **43.97%**. The approximately 1.3% end-to-end timing
improvement is small relative to run variation (individual batch means overlapped:
old 157.557–166.181 ms, new 155.372–167.459 ms). Treat it as roughly unchanged
overall loading throughput, not a demonstrated broad speedup. The isolated paths
showed lower elapsed time and cycles at every tested size.

## Behavior and validation

- Six new behavior/allocation tests cover static actor isolation, stopped updates,
  tween endpoints, pending storage reuse, retained track capacity, ownership drops,
  and comparison with frozen pre-change algorithms. Comparison cases include
  interleaved/equal-end tweens, empty input, backward/repeated query beats, signed
  zero, infinities, NaNs, and epsilon-connected duplicate timestamps.
- The complete old/new release compiler snapshots match byte for byte after
  canonicalizing track order and the fixture path. SHA-256:
  `4a1a2ce03995faf82972ceed065055d8c053897d01e23d33d17e3812b5ccb9ce`.
- The focused sampling selection passes in debug and release: 9 passed, 3 manual
  benchmarks/snapshot tests ignored.
- `cargo check --workspace --bins` passes.
- `cargo clippy -p deadsync-song-lua --all-targets -- -D clippy::perf` passes;
  existing non-performance warnings remain. Formatting and `git diff --check` pass.
- Full Lua unit suite: baseline 387 passed, 5 failed, 5 ignored; final 390 passed,
  the same 5 failed, 5 ignored. The three additional passing cases are allocation
  and ownership assertions added after baseline capture. The new oracle and full
  compiler behavior tests were already present in the baseline run.

The five existing failures, with identical assertions before and after, are:

- `compile_song_lua_extracts_actorproxy_targets`
- `compile_song_lua_layers_share_init_globals_and_actor_refs`
- `compile_song_lua_resolves_local_and_hidden_screen_proxy_targets`
- `compile_song_lua_runs_cmd_queuecommand_builders`
- `compile_song_lua_supports_notefield_column_api`

Before either suite could run, an obsolete multitap test referencing the removed
`push_multitap_explosion_message_events` function was replaced with a test of the
current explosion-ease behavior: unrelated lanes are ignored and overlapping
visibility intervals agree with their union. No multitap production code changed.

## Reproduction

```powershell
cargo test -p deadsync-song-lua --lib sampling -- --test-threads=1
cargo test --release -p deadsync-song-lua --lib sampling_hot_path_bench -- --ignored --test-threads=1 --nocapture
cargo test --release -p deadsync-song-lua --lib sampling_compile_bench -- --ignored --test-threads=1 --nocapture
$env:DEADSYNC_PERF_SNAPSHOT = "$PWD/target/sampling-output.txt"
cargo test --release -p deadsync-song-lua --lib sampling_compile_snapshot -- --ignored --test-threads=1
```

For an old/new comparison, use the same test files and compiler/profile in both
checkouts; restore the three production routines to `e49efcf95` for the baseline.
Run benchmarks serially with other builds stopped. Timing is informational;
behavior and allocation assertions are the regression gates.
