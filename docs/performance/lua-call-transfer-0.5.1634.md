# Lua option arguments, state accessors and environment writes - 0.5.1634

Parent: `a960e1c6f` (0.5.1633). Date: 2026-09-30.

This pass applies the local performance guide's M-HOTPATH and M-MEM-REUSE
recommendations to three sources of short-lived allocation:

1. Speed modifier and MusicRate callbacks read only their first two or three
   arguments into fixed Value slots. The parent materialized a heap MultiValue
   containing every argument and cloned selected values. MusicRate now borrows
   its receiver. Missing arguments remain distinct from explicit nils; nil
   still clears speed modifiers. Dot/colon calls, alternate receiver tables,
   speed approaches, aliases and numeric coercions keep their behavior.
2. Numeric and boolean state getters accept unit arguments, avoiding conversion
   of ignored arguments. Setters consume the same fixed argument prefix. Each
   call still reads or writes the captured table, respects metamethods and
   records side effects after the field write. Mutable state is never cached.
3. Chunk environment writes borrow their key and value. Six retained Lua strings
   identify compile globals by pointer, removing owned key text and reference
   clones. Lua 5.4.9 interns these names, all shorter than its 40-byte threshold;
   retained owning handles keep the immutable identities valid. Other strings,
   including long, invalid UTF-8 and embedded NUL keys, remain ordinary writes.
   Target lookup stays dynamic; target writes still precede global propagation.

There is no new dependency or public API change. The argument carrier adds a
small unsafe FromLuaMulti stack conversion that delegates each selected slot
to mlua's Value::from_stack. It uses the original negative stack indices even
when the incoming list exceeds its capacity; mlua supplies the live stack
and produces owning Value handles. No borrowed stack value escapes. The safe
MultiValue conversion remains implemented and tested. Fixed slots use bounded
stack storage, and the proxy retains six extra Lua string handles per instance.

## Measurements

The [raw CSV](lua-call-transfer-0.5.1634.csv) contains 296 rows: 37 workloads,
two implementations and four separate serial runs, alternating old-first and
new-first. Each timing result is the median of seven samples. Percentage ranges
cover all four paired runs; absolute values below use run 1. A warm operation
contains 64 calls or writes, except the Lua state loop (256 calls) and Lua
environment loop (128 writes). Cold operations construct one options table,
one speed callback, four state callbacks or one environment proxy.

| Workload | Thread cycles/op, old -> new | CPU reduction | Throughput gain | Allocations/reallocations, old -> new | Requested bytes, old -> new |
| --- | ---: | ---: | ---: | ---: | ---: |
| 64 speed calls / receiver getter | 64,847.7 -> 52,432.2 | 10.4% to 19.1% | 11.8% to 23.2% | 64/0 -> 0/0 | 2,560 -> 0 |
| 64 speed calls / numeric setter + approach | 123,245.8 -> 112,809.3 | 0.6% to 8.5% | 0.5% to 8.8% | 64/0 -> 0/0 | 7,680 -> 0 |
| 64 MusicRate calls / receiver getter | 50,481.6 -> 34,128.0 | 26.8% to 32.4% | 37.9% to 48.4% | 128/0 -> 0/0 | 3,584 -> 0 |
| 64 MusicRate calls / numeric setter | 53,585.4 -> 38,228.2 | 28.7% to 41.2% | 39.6% to 70.0% | 128/0 -> 0/0 | 6,144 -> 0 |
| 64 state calls / get number, 2 arguments | 43,160.9 -> 25,847.8 | 37.0% to 41.7% | 58.8% to 71.6% | 64/0 -> 0/0 | 5,120 -> 0 |
| 64 state calls / set number, 2 arguments | 86,812.2 -> 80,058.3 | 7.4% to 10.5% | 8.0% to 11.8% | 64/0 -> 0/0 | 5,120 -> 0 |
| 64 state calls / get bool, 2 arguments | 39,819.5 -> 23,687.1 | 40.3% to 43.9% | 67.3% to 78.5% | 64/0 -> 0/0 | 5,120 -> 0 |
| 64 state calls / set bool, 2 arguments | 86,187.2 -> 77,595.8 | 8.1% to 12.7% | 8.6% to 14.8% | 64/0 -> 0/0 | 5,120 -> 0 |
| Lua state loop / 256 accessor calls | 218,816.6 -> 175,761.2 | 19.7% to 23.1% | 24.4% to 30.3% | 256/0 -> 0/0 | 15,360 -> 0 |
| 64 proxy writes / compile key, numeric value | 133,459.4 -> 91,319.7 | 25.1% to 31.6% | 33.3% to 47.2% | 128/0 -> 0/0 | 1,536 -> 0 |
| 64 proxy writes / compile key, table value | 138,601.4 -> 98,553.8 | 25.5% to 29.9% | 32.5% to 41.4% | 192/0 -> 0/0 | 2,560 -> 0 |
| 64 proxy writes / unrelated key, numeric value | 110,851.8 -> 78,698.5 | 23.9% to 29.0% | 31.4% to 40.7% | 128/0 -> 0/0 | 1,600 -> 0 |
| 64 proxy writes / unrelated key, table value | 118,471.7 -> 81,570.8 | 24.8% to 31.4% | 33.0% to 45.8% | 192/0 -> 0/0 | 2,624 -> 0 |
| Lua environment loop / 128 writes | 193,211.4 -> 128,855.9 | 28.0% to 33.3% | 38.0% to 50.0% | 256/0 -> 0/0 | 3,136 -> 0 |

All measured new warm option/state calls and proxy writes have zero allocation, reallocation, free and requested/freed-byte churn in every run. Allocator counts agree across all four runs for every workload and implementation. The unchanged proxy-read control retains its previous churn.

64 speed calls / numeric setter + approach: allocations/reallocations/frees change from 64/0/64 to 0/0/0; requested/freed bytes change from 7,680/7,680 to 0/0.

64 MusicRate calls / receiver getter: allocations/reallocations/frees change from 128/0/128 to 0/0/0; requested/freed bytes change from 3,584/3,584 to 0/0.

Lua state loop / 256 accessor calls: allocations/reallocations/frees change from 256/0/256 to 0/0/0; requested/freed bytes change from 15,360/15,360 to 0/0.

64 proxy writes / compile key, table value: allocations/reallocations/frees change from 192/0/192 to 0/0/0; requested/freed bytes change from 2,560/2,560 to 0/0.

64 proxy writes / unrelated key, table value: allocations/reallocations/frees change from 192/0/192 to 0/0/0; requested/freed bytes change from 2,624/2,624 to 0/0.

64 proxy writes / 2,560-byte unrelated key: allocations/reallocations/frees change from 128/0/128 to 0/0/0; requested/freed bytes change from 164,864/164,864 to 0/0.

Lua environment loop / 128 writes: allocations/reallocations/frees change from 256/0/256 to 0/0/0; requested/freed bytes change from 3,136/3,136 to 0/0.

## Controls and limits

Negative percentages mean higher CPU cost or lower throughput. Zero-argument,
large-argument, numeric-key, unchanged-read and cold-construction cases remain
in the results.

| Workload | Thread cycles/op, old -> new | CPU reduction | Throughput gain | Allocations/reallocations, old -> new | Requested bytes, old -> new |
| --- | ---: | ---: | ---: | ---: | ---: |
| 64 proxy writes / 2,560-byte unrelated key | 125,984.4 -> 84,090.8 | 33.3% to 38.2% | 49.8% to 61.8% | 128/0 -> 0/0 | 164,864 -> 0 |
| 64 proxy writes / invalid UTF-8 key | 146,258.2 -> 78,359.8 | 42.1% to 46.4% | 72.6% to 86.7% | 192/64 -> 0/0 | 6,272 -> 0 |
| 64 proxy writes / numeric key | 77,937.9 -> 76,333.7 | 1.7% to 6.6% | 1.5% to 7.9% | 0/0 -> 0/0 | 0 -> 0 |
| 64 proxy reads / unchanged control | 92,536.4 -> 95,594.8 | -8.3% to 8.9% | -7.8% to 9.8% | 64/0 -> 64/0 | 1,024 -> 1,024 |
| Cold proxy construction / fresh names | 6,783.9 -> 8,723.4 | -32.3% to -18.8% | -28.9% to -21.4% | 15/0 -> 21/0 | 653 -> 1,054 |
| Cold proxy construction / six names already interned | 7,661.9 -> 9,160.7 | -45.7% to -14.4% | -35.6% to -16.9% | 15/0 -> 15/0 | 653 -> 845 |
| 64 speed calls / explicit nil clear | 65,079.2 -> 57,230.3 | 12.1% to 15.1% | 13.8% to 17.6% | 64/0 -> 0/0 | 5,120 -> 0 |
| 64 speed calls / no-argument getter | 44,794.3 -> 46,511.7 | -5.7% to -1.0% | -6.4% to -0.9% | 0/0 -> 0/0 | 0 -> 0 |
| 64 speed calls / setter, 64 arguments | 299,402.3 -> 154,690.9 | 47.1% to 54.6% | 87.4% to 120.0% | 64/0 -> 0/0 | 163,840 -> 0 |
| 64 speed calls / setter, 257 arguments | 865,487.6 -> 265,067.7 | 69.4% to 70.0% | 224.2% to 233.0% | 64/0 -> 0/0 | 657,920 -> 0 |
| 64 MusicRate calls / no-argument getter | 13,550.7 -> 14,523.9 | -13.0% to -2.0% | -11.6% to -3.1% | 0/0 -> 0/0 | 0 -> 0 |
| 64 MusicRate calls / setter, 257 arguments | 803,386.3 -> 185,935.4 | 73.2% to 77.1% | 271.1% to 336.1% | 128/0 -> 0/0 | 658,944 -> 0 |
| Cold construction / speed callback | 7,609.3 -> 7,348.7 | -2.6% to 3.4% | -3.1% to 3.5% | 16/1 -> 16/1 | 951 -> 951 |
| Cold construction / song options table | 4,717.0 -> 5,018.8 | -6.4% to -2.7% | -7.2% to -4.0% | 7/0 -> 7/0 | 335 -> 335 |
| 64 state calls / get number, 0 arguments | 26,121.4 -> 24,205.9 | 4.6% to 7.3% | 4.7% to 7.9% | 0/0 -> 0/0 | 0 -> 0 |
| 64 state calls / get number, 257 arguments | 794,204.2 -> 172,365.8 | 76.2% to 78.3% | 320.1% to 361.3% | 64/0 -> 0/0 | 657,920 -> 0 |
| 64 state calls / set number, 0 arguments | 66,757.2 -> 68,241.3 | -5.9% to -2.2% | -5.5% to -2.1% | 0/0 -> 0/0 | 0 -> 0 |
| 64 state calls / set number, 257 arguments | 816,492.0 -> 227,325.7 | 70.5% to 72.6% | 239.2% to 264.5% | 64/0 -> 0/0 | 657,920 -> 0 |
| 64 state calls / get bool, 0 arguments | 22,977.2 -> 20,952.0 | 7.2% to 8.8% | 7.7% to 9.7% | 0/0 -> 0/0 | 0 -> 0 |
| 64 state calls / get bool, 257 arguments | 772,401.6 -> 171,423.5 | 77.3% to 78.4% | 341.5% to 363.3% | 64/0 -> 0/0 | 657,920 -> 0 |
| 64 state calls / set bool, 0 arguments | 66,000.0 -> 68,435.1 | -4.2% to 13.9% | -4.1% to 16.1% | 0/0 -> 0/0 | 0 -> 0 |
| 64 state calls / set bool, 257 arguments | 838,045.9 -> 222,218.0 | 71.6% to 73.5% | 252.7% to 277.4% | 64/0 -> 0/0 | 657,920 -> 0 |
| Cold construction / four state callbacks | 8,855.5 -> 9,207.6 | -6.9% to -4.0% | -6.2% to -0.7% | 20/0 -> 20/0 | 988 -> 988 |

Workloads with higher CPU cost in every run: Cold proxy construction / fresh names; Cold proxy construction / six names already interned; 64 speed calls / no-argument getter; 64 MusicRate calls / no-argument getter; Cold construction / song options table; 64 state calls / set number, 0 arguments; Cold construction / four state callbacks.

Workloads with mixed CPU changes: 64 proxy reads / unchanged control; Cold construction / speed callback; 64 state calls / set bool, 0 arguments. No uniform CPU improvement is claimed for these controls; their samples remain in the CSV.

Cases with increased requested bytes: Cold proxy construction / fresh names (653 -> 1,054 bytes); Cold proxy construction / six names already interned (653 -> 845 bytes).

Cases with increased allocation counts: Cold proxy construction / fresh names (15 -> 21 allocations). Construction therefore qualifies the memory benefit; zero warm churn does not imply lower retained memory for every instance.

Zero churn applies to these warmed numeric/boolean calls and environment writes
with prepared Lua keys and values. Parsing strings still uses the existing
conversion helpers, and arbitrary Lua callbacks can allocate. First calls can
grow Lua tables, stacks or reference pools. Proxy construction creates/retains
the six keys before their first use; fresh and already-interned construction
controls expose this cost. Fixed argument slots avoid an unbounded carrier,
but passing many arguments still entails the caller's Lua stack work.

Lua GC is stopped for benchmarks. Setup, script compilation, input construction
and fixture destruction are outside measured work. Cold construction uses a
fresh Lua VM and target table for every operation, created before measurement;
only helper construction is timed. Prepared proxy construction pre-interns all
six names in globals. Temporary returned Rust handles are dropped inside the
construction work, while Lua outputs stay uncollected until VM destruction
outside measurement. Allocator counts therefore describe
requested/freed byte churn, including Lua's System-allocator calls, rather than
retained memory, peak RSS or deferred garbage collection. No whole-song load,
instruction-count or game frame-rate claim is made.

## Behavior and reproducibility

Seven functions are frozen verbatim from the parent, modulo formatting and
test visibility: two option constructors/installers, four state accessors and
the proxy constructor. Their unchanged numeric conversions, speed state helpers,
metamethod dispatch and side-effect helper are shared. A source audit checks
all seven baselines against `a960e1c6f`.

Ten added tests compare old/new behavior for missing and explicit nil values,
dot/colon calls, foreign receivers, 0 through 257 extra arguments, integer and
float boundaries, NaN/infinity, numeric/boolean strings, invalid UTF-8, arbitrary
Lua reference values, live state changes, speed clears/approaches, exact compile
global names, reference identity, target retargeting, fallback reads, reentry
and partial failures. Allocation tests require zero warm allocation,
reallocation, free and requested/freed bytes and strict reductions against the
parent on allocating paths.

Debug and release library suites each report **704 passed, 4 failed, 70 ignored**.
All ten new tests pass. The same four pre-existing failures occur in both
parent profiles at the same source locations and assertions:

- `compile_song_lua_extracts_actorproxy_targets`
- `compile_song_lua_resolves_local_and_hidden_screen_proxy_targets`
- `compile_song_lua_runs_cmd_queuecommand_builders`
- `compile_song_lua_supports_notefield_column_api`

Clippy completes with the same inherited 75 test warnings (45 duplicates),
with no warnings in MethodArgs or new fixtures. Formatting and diff checks
pass. The application patch changes exactly once: **0.5.1633 -> 0.5.1634**.
The lockfile updates only the three packages inheriting that version.

Environment: Windows x64, Intel Xeon E5-2696 v4 at 2.20 GHz (22 cores / 44
threads), rustc 1.98.1 / LLVM 22.1.8, opt-level 3 and fat LTO, mlua 0.12.1
with vendored Lua 5.4.9. Windows QueryThreadCycleTime counts cycles of the
executing thread, separately from wall time and useful-unit throughput.
The existing thread-local scoped System allocator records allocation,
reallocation, free and requested/freed byte counts in a separate operation.

```powershell
cargo test --locked -p deadsync-song-lua --lib -- --test-threads=1
cargo test --locked -p deadsync-song-lua --release --lib -- --test-threads=1
cargo clippy --locked -p deadsync-song-lua --lib --tests
cargo test --locked -p deadsync-song-lua --release --lib call_transfer_bench -- --ignored --nocapture --test-threads=1
# Alternate this environment variable between runs for reversed pairing:
$env:DEADSYNC_PERF_REVERSE = '1'
```

The four excluded input/helper files are absent from this pass's commit:
`deadsync-song.json.gz`, `rust-performance.md`, `optimize.sh` and `optimize.ps1`.
