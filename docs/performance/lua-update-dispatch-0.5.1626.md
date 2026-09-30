# Lua update dispatch - 0.5.1626

Parent: `fddc06faa` (0.5.1625). Date: 2026-09-30.

Three allocation-heavy operations remain in the song compiler and actor command
paths after the previous frame-sampling pass. This pass applies the local guide's
M-HOTPATH, M-MEM-REUSE, M-BOX-DST, and M-THROUGHPUT recommendations:

1. The cached compile update plan uses `Rc<[SongLuaCompileUpdateJob]>`. Each frame
   shares one immutable plan instead of allocating and copying a `Vec` of jobs
   and cloning every actor handle. No Lua app-data borrow spans a callback.
   Invalidation during a callback lets the current frame finish its existing
   jobs, then rebuilds the plan for the next frame with the new actor tree/rates.
2. Recurring command names up to 128 UTF-8 bytes use stack storage instead of a
   new Rust string every tick, including ticks that are only waiting on a sleep.
   Names are still read each time; callback changes and lookup side effects are
   observable at the same points. Longer names retain one owning heap allocation.
3. Actor type comparisons, including bitmap-text detection, use the same inline
   conversion with a 32-byte capacity. Hierarchical message delivery previously
   allocated once or twice for every actor merely to compare its type.

The private conversion copies validated Lua stack bytes into owned inline or
heap storage. Its small unsafe adapter uses mlua's FromLua stack conversion API:
only a type-checked string is read, and no Lua pointer or borrowed slice escapes
the conversion. Numbers and other types use the original String conversion;
UTF-8 errors keep the original String diagnostic. There are no new dependencies
or public API changes. Zero churn applies to the warm plan, waiting short
commands, and short type comparisons; compilation and actual command execution
still allocate for their outputs and Lua state.

## Measurements

All 112 measurement rows, including both execution orders, timing ranges,
throughput, frees and byte counts, are in [the CSV](lua-update-dispatch-0.5.1626.csv).
Absolute values below use run 1; CPU ranges use all four runs.

| Workload | CPU cycles/op old -> new | CPU reduction across runs | Allocations/reallocations old -> new | Requested bytes old -> new |
| --- | ---: | ---: | ---: | ---: |
| 128 actors, 64 callback-only frames | 9,029,146 -> 8,731,120 | 2.0-3.8% | 64/0 -> 0/0 | 327,680 -> 0 |
| 64 waiting-command ticks | 67,742 -> 62,527 | 7.7-21.4% | 64/0 -> 0/0 | 704 -> 0 |
| 128 actor type comparisons | 48,663 -> 35,699 | 26.6-30.4% | 128/0 -> 0/0 | 1,792 -> 0 |
| 32 actors, 64 frames with callbacks and recurring commands | 5,395,132 -> 5,065,991 | 4.3-20.4% | 2,595/0 -> 483/0 | 204,864 -> 32,832 |
| Complete 64-actor frame compiler | 95,366,678 -> 94,619,355 | 0.8-2.4% | 23,805/768 -> 15,941/768 | 2,924,862 -> 2,230,780 |

The three isolated warm paths eliminate all their allocations, frees and requested
bytes. Callback-only dispatch gains 2.0-4.0% throughput with 128 actors and
10.5-20.7% with one actor. Waiting commands gain 8.3-27.2%, and direct type
comparisons gain 36.2-43.6%. The mixed 32-actor dispatch gains 4.5-25.7% throughput,
reduces allocation calls about 81.4%, and reduces requested bytes 83.8-84.0%.

The complete 64-actor compiler gains 0.9-2.2% throughput, reduces allocation calls
33.0-33.2%, and reduces requested bytes 23.7-25.0%. Reallocations remain 768 in both
implementations; these belong to other Lua/compiler work. In run 1, compilation
takes 43.52 -> 43.18 ms. GC causes some separately counted operations to vary;
all counts are retained in the CSV. This pass primarily reduces allocation churn;
its complete-compiler CPU gain is modest.

Controls limit the CPU claim. The 32-actor callback-only case ranges from 0.3%
more CPU cycles to 6.7% fewer. The complete 16-actor compiler ranges from 1.5%
more cycles to 1.4% fewer, so it does not establish a reliable CPU improvement.
GC-disabled message delivery eliminates 48, 1,040, or 4,112 type-string allocations
per batch at 1, 32, or 128 actors, but the 32-actor delivery case uses up to 9.8%
more CPU cycles (the best run uses 2.1% fewer). The 128-actor delivery control
ranges from 0.8% more to 9.0% fewer cycles. Reduced type churn and faster isolated
type comparisons do not imply faster message delivery at every size. These
controls and their regressions are included in the measurements, not hidden
behind a universal speedup claim.

## Method and reproduction

Windows x86-64, Intel Xeon E5-2696 v4 @ 2.20 GHz, Rust 1.98.1. Both implementations
run in the same release test executable. The scoped allocator counts calling-thread
allocations, reallocations, frees, and requested/freed bytes. QueryThreadCycleTime
counts calling-thread CPU cycles. Timing and allocation accounting are separate.
The byte totals describe allocator churn, not peak resident memory or process RSS.

Each measurement is the median of seven timing samples after warmup, followed by
one separately counted operation. Four runs alternate old-first and new-first.
Dispatch batches run 64 frames with 1, 32, or 128 actors, either callbacks alone
or callbacks plus 250 ms recurring commands. The waiting-command batch runs 64
ticks; the type batch runs 128 comparisons. Message batches send 16 messages to
1, 32, or 128 actors. Message microbenchmarks stop Lua GC in both variants to
isolate delivery from collector phase; their allocation totals include Lua tables
and command guard state as well as type strings. Dispatch benchmarks keep GC on.

The complete compiler uses 1, 16, or 64 actors, each with a callback and 50 ms
recurring command that changes captured coordinates, inherited update rates,
BPM changes, a 1.25 music rate, and a two-second chart. Setup and fixture teardown
are outside measurement; compilation and result destruction are included. Normal
Lua GC stays on. This measures the complete frame compiler with the parent versus
current update dispatcher, not whole-game frame rate.

The test-only dispatcher baseline freezes eight parent function bodies, including
the old plan container, recurring name reader, type predicates, and hierarchical
message helper. The complete compiler baseline freezes the parent compiler and
its runtime clock helper, redirecting that helper to the frozen dispatcher.
Shared code outside these functions is identical in both variants. All ten bodies
were checked against the parent, allowing visibility, formatting, and that one
redirection.

```powershell
cargo test -p deadsync-song-lua --lib update_dispatch -- --test-threads=1
cargo test -p deadsync-song-lua --release --lib update_dispatch -- --test-threads=1
cargo test -p deadsync-song-lua --release --lib update_dispatch -- --ignored --nocapture --test-threads=1
$env:DEADSYNC_PERF_REVERSE = '1'
cargo test -p deadsync-song-lua --release --lib update_dispatch -- --ignored --nocapture --test-threads=1
Remove-Item Env:DEADSYNC_PERF_REVERSE
```

## Behavior validation

Eight new regression tests cover preorder recurring commands, postorder callbacks,
inherited update rates, callback invalidation with next-frame rebuild and GC,
exact sleep boundaries, negative/zero deltas, changed recurring intervals, the
64-run cap, partial command errors, numeric coercions, Unicode/NUL and invalid
UTF-8, both inline spill boundaries and 4096-byte names, dynamic metatable type
lookups, message hierarchy, and complete compiler tracks and timing. Allocation
assertions require zero warm plan/waiting/type churn and reduced message churn.

- Debug and release library suites each report 636 passed, 4 failed, 57 ignored.
- All eight new tests pass in both profiles. The two manual benchmark tests pass
  in all four release runs.
- The previous pass at parent `fddc06faa` recorded 628 passed, the same 4 failed,
  and 55 ignored in both profiles. See [the prior validation report](lua-frame-sampling-0.5.1625.md).
- The four failures retain identical assertions: `compile_song_lua_extracts_actorproxy_targets`,
  `compile_song_lua_resolves_local_and_hidden_screen_proxy_targets`, and
  `compile_song_lua_runs_cmd_queuecommand_builders` expect different initial proxy
  visibility; `compile_song_lua_supports_notefield_column_api` expects `-96:-125`
  while both versions produce `-96:-135`.
- Clippy completes with existing warnings, matching warnings in the frozen
  compiler baseline, and a type-complexity warning for the test fixture's compiler
  result tuple. It reports no warnings in the new production conversion or plan code.
- Formatting, frozen-body audit, version/lock consistency, and `git diff --check` pass.

The workspace version advances exactly once, 0.5.1625 -> 0.5.1626. Cargo.lock
updates the three packages that inherit it. The four excluded files are not
part of the commit.
