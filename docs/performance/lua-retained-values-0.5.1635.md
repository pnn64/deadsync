# Lua retained text, option metadata and dense sequences - 0.5.1635

Parent: `b27c63730` (0.5.1634). Date: 2026-09-30.

This pass applies the local performance guide's M-HOTPATH, M-MEM-REUSE and
M-INITIAL-CAPACITY recommendations in three areas:

1. Mutable string getters and setters transfer existing Lua strings rather
   than copying them through owned Rust strings and recreating Lua strings.
   Getters ignore arguments directly; setters select only the original method
   value slot. Getter numeric coercions use mlua's original String conversion.
   Invalid UTF-8 still errors in getters, while setter invalid/non-string/nil
   values still write the empty string. Mutable fields and metamethods remain
   live, and side effects are recorded after writes.
2. Generic player-option callbacks capture an immutable Lua key, value kind and
   default when constructed. Each call still resolves the current state and
   speed tables. Numeric/boolean conversion, string defaults, explicit nil
   getter behavior and eager prior-speed reads keep their original semantics.
   This removes repeated method-name classification and Lua key construction,
   including allocation of long custom keys. Capturing immutable metadata is
   separate from reading mutable option values on every call.
3. Split and range builders reserve dense Lua output arrays. Nonempty separator
   splits count their exact output length, using memchr's bulk count when the
   separator is one byte. The empty separator branch remains unchanged.
   Range capacity is an estimate bounded by the existing 10,000-entry
   limit. The original repeated f32 addition and termination checks still
   determine every value and the actual count. NaN/infinity input and stalled
   increments retain their behavior. Table mapping remains unchanged: callbacks
   can produce nil holes, where capacity can alter Lua's length behavior.

The text conversion introduces narrow unsafe FromLua/FromLuaMulti stack paths
and a UTF-8 validator. Before borrowing bytes, each path checks that its live
stack slot is a string. The validator uses Lua's returned pointer and exact
length, runs no Lua operations while the slice exists, and returns only a
validation result. mlua then creates an owning LuaString handle. No borrowed
stack bytes escape. Getter UTF-8 errors retain String's error fields; numeric
coercions and other type errors delegate to String::from_stack. Safe fallback
conversions are implemented and compared with the parent contracts.

There is no new dependency, public API change or global cache. Additional
option metadata and owning Lua key/default handles shift storage to callback
construction. Required output Lua tables still allocate. Zero churn describes
the measured warm text/option paths, not arbitrary inputs or a whole frame.

## Measurements

The [raw CSV](lua-retained-values-0.5.1635.csv) contains 344 rows: 43 workloads,
two implementations and four separate serial runs alternating old-first and
new-first. Each timing value is the median of seven samples. Percentage ranges
cover all four paired runs; absolute values below use run 1. Warm batches have
64 calls, except the text Lua loop (128 calls) and option Lua loop (256 calls).
Sequence operations construct one table; units/s counts produced entries.
Constructor controls install two text methods or one player-option method.

| Workload | Thread cycles/op, old -> new | CPU reduction | Throughput gain | Allocations/reallocations, old -> new | Requested bytes, old -> new |
| --- | ---: | ---: | ---: | ---: | ---: |
| 64 text getters / 11 bytes | 60,276.8 -> 31,996.4 | 45.0% to 46.9% | 82.0% to 88.6% | 128/0 -> 0/0 | 3,264 -> 0 |
| 64 text setters / 11 bytes | 116,853.7 -> 77,854.8 | 32.1% to 43.2% | 47.2% to 76.1% | 192/0 -> 0/0 | 6,848 -> 0 |
| 64 text getters / 4,096 bytes | 465,408.6 -> 52,258.1 | 88.0% to 89.7% | 732.5% to 871.2% | 192/0 -> 0/0 | 528,448 -> 0 |
| 64 text setters / 4,096 bytes | 498,652.6 -> 90,005.3 | 80.3% to 82.5% | 408.3% to 472.0% | 256/0 -> 0/0 | 532,032 -> 0 |
| Lua text loop / 128 calls | 153,395.3 -> 93,597.0 | 39.0% to 45.4% | 63.9% to 83.3% | 320/0 -> 0/0 | 9,664 -> 0 |
| 64 option calls / numeric, stored getter | 53,209.0 -> 51,247.2 | -0.8% to 8.4% | -1.3% to 9.1% | 0/0 -> 0/0 | 0 -> 0 |
| 64 option calls / numeric, setter + approach | 101,425.3 -> 98,173.1 | 3.2% to 7.3% | 3.2% to 7.9% | 0/0 -> 0/0 | 0 -> 0 |
| 64 option calls / boolean, stored getter | 50,485.0 -> 47,909.3 | -3.2% to 7.8% | -3.2% to 8.4% | 0/0 -> 0/0 | 0 -> 0 |
| 64 option calls / boolean, setter + approach | 96,956.4 -> 93,538.7 | 3.5% to 8.5% | 3.7% to 9.2% | 0/0 -> 0/0 | 0 -> 0 |
| 64 option calls / string, stored getter | 58,928.0 -> 54,252.5 | 2.0% to 7.9% | 2.0% to 8.7% | 0/0 -> 0/0 | 0 -> 0 |
| 64 option calls / string, setter + approach | 103,968.4 -> 97,386.0 | 4.1% to 8.1% | 4.3% to 8.9% | 0/0 -> 0/0 | 0 -> 0 |
| Lua option loop / 256 calls | 289,896.1 -> 279,024.8 | 2.5% to 4.7% | 2.7% to 4.9% | 0/0 -> 0/0 | 0 -> 0 |
| Split construction / 64 entries | 14,958.6 -> 11,988.5 | 19.9% to 22.4% | 25.7% to 33.1% | 2/6 -> 2/0 | 2,088 -> 1,080 |
| Split construction / 1024 entries | 144,674.5 -> 132,965.5 | 8.1% to 10.1% | 8.8% to 11.4% | 2/10 -> 2/0 | 32,808 -> 16,440 |
| Range construction / 64 entries | 11,156.8 -> 8,486.7 | 20.8% to 25.7% | 31.6% to 41.4% | 2/6 -> 2/0 | 2,088 -> 1,080 |
| Range construction / 1024 entries | 96,417.1 -> 86,292.6 | 10.3% to 11.5% | 11.4% to 12.9% | 2/10 -> 2/0 | 32,808 -> 16,440 |
| Range construction / 10,000 entries, capped | 1,020,544.7 -> 820,923.2 | 6.8% to 33.2% | 7.2% to 50.1% | 2/14 -> 2/0 | 524,328 -> 160,056 |

All measured new warm valid-text and player-option batches have zero allocation, reallocation, free and requested/freed-byte churn in every run. Numeric text getter coercions retain their original conversion allocations. Allocator counts agree across all four runs for every workload and implementation. Dense sequence tables require storage; reservation primarily removes array growth and requested-byte churn.

64 text getters / 4,096 bytes: allocations/reallocations/frees change from 192/0/128 to 0/0/0; requested/freed bytes change from 528,448/264,704 to 0/0.

64 text setters / 4,096 bytes: allocations/reallocations/frees change from 256/0/192 to 0/0/0; requested/freed bytes change from 532,032/268,288 to 0/0.

Lua text loop / 128 calls: allocations/reallocations/frees change from 320/0/320 to 0/0/0; requested/freed bytes change from 9,664/9,664 to 0/0.

64 option calls / 96-byte custom key, setter + approach: allocations/reallocations/frees change from 192/0/0 to 0/0/0; requested/freed bytes change from 23,232/0 to 0/0.

Split construction / 1024 entries: allocations/reallocations/frees change from 2/10/0 to 2/0/0; requested/freed bytes change from 32,808/16,368 to 16,440/0.

Range construction / 1024 entries: allocations/reallocations/frees change from 2/10/0 to 2/0/0; requested/freed bytes change from 32,808/16,368 to 16,440/0.

Range construction / 10,000 entries, capped: allocations/reallocations/frees change from 2/14/0 to 2/0/0; requested/freed bytes change from 524,328/262,128 to 160,056/0.

## Controls and limits

Negative percentages mean higher CPU cost or lower throughput. Default reads,
no-argument calls, numeric coercions, long keys, single-entry/empty sequences,
fractional/capped ranges and cold construction remain in the results.

The 96-byte custom-key cases invoke the internal option factory directly with
a synthetic name. They measure the long-key boundary and are separate from
the ordinary Mini/Mirror option loop used to estimate typical callback gains.

| Workload | Thread cycles/op, old -> new | CPU reduction | Throughput gain | Allocations/reallocations, old -> new | Requested bytes, old -> new |
| --- | ---: | ---: | ---: | ---: | ---: |
| 64 option calls / numeric, default getter | 54,423.1 -> 50,260.4 | 0.6% to 7.6% | 0.6% to 8.3% | 0/0 -> 0/0 | 0 -> 0 |
| Cold option construction / numeric | 2,236.1 -> 2,490.0 | -24.2% to -11.4% | -40.3% to -25.8% | 5/0 -> 6/0 | 196 -> 273 |
| 64 option calls / boolean, default getter | 49,691.0 -> 47,033.0 | 1.9% to 5.6% | 1.9% to 6.5% | 0/0 -> 0/0 | 0 -> 0 |
| Cold option construction / boolean | 2,323.0 -> 2,631.8 | -36.6% to -10.5% | -49.0% to -22.0% | 5/0 -> 6/0 | 198 -> 277 |
| 64 option calls / string, default getter | 60,846.1 -> 53,582.0 | 11.9% to 21.1% | 13.0% to 26.8% | 0/0 -> 0/0 | 0 -> 0 |
| Cold option construction / string | 2,231.6 -> 3,214.7 | -69.8% to -31.5% | -63.9% to -45.4% | 5/0 -> 7/0 | 203 -> 324 |
| 64 option calls / 96-byte custom key, stored getter | 89,796.1 -> 55,690.4 | 38.0% to 44.8% | 61.2% to 81.4% | 64/0 -> 0/0 | 7,744 -> 0 |
| 64 option calls / 96-byte custom key, setter + approach | 212,945.9 -> 100,368.9 | 52.9% to 59.4% | 112.4% to 146.5% | 192/0 -> 0/0 | 23,232 -> 0 |
| 64 option calls / 96-byte custom key, default getter | 89,730.9 -> 50,057.1 | 42.5% to 45.3% | 74.1% to 81.8% | 64/0 -> 0/0 | 7,744 -> 0 |
| Cold option construction / 96-byte custom key | 2,366.5 -> 2,942.6 | -24.3% to 0.6% | -34.3% to -19.9% | 5/0 -> 6/0 | 288 -> 457 |
| Split construction / 1 entries | 2,537.9 -> 2,462.5 | -11.7% to 3.0% | -14.6% to 7.7% | 2/0 -> 2/0 | 72 -> 72 |
| Range construction / 1 entries | 2,796.9 -> 3,038.7 | -10.9% to -2.4% | -12.8% to -2.8% | 2/0 -> 2/0 | 72 -> 72 |
| Split construction / 8 entries | 5,022.8 -> 3,712.7 | 23.3% to 41.1% | 47.4% to 103.1% | 2/3 -> 2/0 | 296 -> 184 |
| Range construction / 8 entries | 4,940.4 -> 4,033.3 | 18.4% to 27.7% | 36.3% to 48.6% | 2/3 -> 2/0 | 296 -> 184 |
| Split construction / empty text | 2,845.0 -> 2,970.1 | -4.4% to 2.0% | -3.5% to 20.5% | 3/0 -> 3/0 | 97 -> 97 |
| Split construction / Unicode characters, empty separator | 5,189.1 -> 5,377.7 | -3.6% to -0.5% | -5.1% to -2.4% | 7/0 -> 7/0 | 267 -> 267 |
| Split construction / overlapping separator | 4,089.9 -> 3,748.7 | -0.4% to 8.3% | -0.7% to 10.0% | 4/1 -> 4/0 | 157 -> 141 |
| Range construction / empty output | 2,050.9 -> 2,292.8 | -13.8% to 2.9% | -29.9% to -19.5% | 1/0 -> 1/0 | 56 -> 56 |
| Range construction / fractional step | 5,353.8 -> 4,539.2 | 15.2% to 24.4% | 25.0% to 47.9% | 2/4 -> 2/1 | 552 -> 472 |
| 64 text getters / empty | 51,345.0 -> 30,296.1 | 35.6% to 41.0% | 55.3% to 69.6% | 64/0 -> 0/0 | 2,560 -> 0 |
| 64 text setters / empty | 107,924.5 -> 74,709.7 | 26.4% to 30.8% | 36.1% to 44.3% | 128/0 -> 0/0 | 6,144 -> 0 |
| 64 text calls / stored string, no arguments | 43,193.5 -> 30,241.3 | 23.5% to 33.7% | 30.9% to 50.7% | 64/0 -> 0/0 | 384 -> 0 |
| 64 text calls / numeric getter coercion | 96,744.6 -> 80,909.8 | 10.9% to 16.4% | 12.3% to 18.9% | 192/0 -> 128/0 | 3,712 -> 1,152 |
| 64 text calls / numeric setter default | 86,332.1 -> 72,570.5 | 14.0% to 17.0% | 16.1% to 20.6% | 64/0 -> 0/0 | 5,120 -> 0 |
| 64 text calls / setter, 257 arguments | 784,882.3 -> 242,115.4 | 66.9% to 70.2% | 201.7% to 235.7% | 192/0 -> 0/0 | 659,456 -> 0 |
| Cold construction / two text accessors | 5,238.3 -> 4,934.2 | 3.2% to 5.8% | 0.9% to 5.9% | 11/0 -> 11/0 | 488 -> 488 |

Workloads with higher CPU cost in every run: Cold option construction / numeric; Cold option construction / boolean; Cold option construction / string; Range construction / 1 entries; Split construction / Unicode characters, empty separator.

Workloads with mixed CPU changes: 64 option calls / numeric, stored getter; 64 option calls / boolean, stored getter; Cold option construction / 96-byte custom key; Split construction / 1 entries; Split construction / empty text; Split construction / overlapping separator; Range construction / empty output. No uniform CPU improvement is claimed for these workloads; every sample remains in the CSV.

Cases with increased requested bytes: Cold option construction / numeric (196 -> 273 bytes); Cold option construction / boolean (198 -> 277 bytes); Cold option construction / string (203 -> 324 bytes); Cold option construction / 96-byte custom key (288 -> 457 bytes).

Cases with increased allocation counts: Cold option construction / numeric (5 -> 6 allocations); Cold option construction / boolean (5 -> 6 allocations); Cold option construction / string (5 -> 7 allocations); Cold option construction / 96-byte custom key (5 -> 6 allocations). Cold construction and tiny sequence cases qualify the overall CPU/memory benefit.

Numeric getter coercions still allocate through mlua's original String
conversion. Invalid UTF-8 errors and user metamethods can allocate. Warm valid
text values use existing Lua strings; first calls may grow Lua reference pools,
tables or stacks. Option constructors now retain metadata and keys/defaults
before their first use, including unused methods. The split capacity count is
an extra traversal; range estimates may reserve more space than the actual
output or grow if f32 rounding produces more entries than estimated.

Lua GC is stopped during benchmarks. Prepared arguments, input strings,
scripts, Lua VM setup and fixture destruction are outside measurements. The
long text timing cases use fewer iterations for both variants to bound
uncollected output. Parent setters/getters recreate long strings, and parent
long option keys remain uncollected until fixture destruction. Reported frees
do not include this deferred collection. Cold construction uses a fresh Lua
VM/owner per operation. Sequence benchmarks also use fresh VMs, with repeated
split token text already interned before measurement in both variants.
Temporary returned Rust handles are dropped inside measured work; Lua output
tables and strings are collected at VM destruction outside measurements.

Requested/freed bytes are allocator churn, not retained memory or peak RSS.
No instruction-count, whole-song loading or game frame-rate claim is made.
Windows thread cycle counts measure the executing thread rather than TSC ticks.

## Behavior and reproducibility

Five functions are frozen from the parent, modulo formatting and visibility:
the two mutable string accessors, generic player-option method factory, split
builder and range builder. Numeric/boolean coercion, default classification,
live state/speed lookup and number representation helpers remain shared and
unchanged. A source audit checks every frozen function against `b27c63730`.

Ten new regression tests compare all player-option capability names and custom
names, missing/nil/dot/colon arguments through 257 entries, exact getter UTF-8
and type errors, safe conversion fallbacks, numeric/boolean/string coercions,
long strings, UTF-8/NUL bytes, live field and state-table replacements,
metamethod reads/writes, eager speed fallback errors and partial writes.
Split tests cover empty/overlapping/Unicode separators and randomized text.
Range tests compare every value's float bits and Lua length for signed zero,
direction changes, fractional increments, tiny steps, stalled increments,
NaN/infinity, the 10,000 cap and randomized float inputs. Allocation assertions
require zero warm text/option churn and bound dense output construction to
the required Lua table plus array storage without growth reallocations.

Debug and release library suites each report **714 passed, 4 failed, 73 ignored**.
All ten new tests pass. Parent and new profiles have the same four inherited
failures at the same assertions and source locations:

- `compile_song_lua_extracts_actorproxy_targets`
- `compile_song_lua_resolves_local_and_hidden_screen_proxy_targets`
- `compile_song_lua_runs_cmd_queuecommand_builders`
- `compile_song_lua_supports_notefield_column_api`

Clippy completes with inherited warnings and no warnings in new code/fixtures.
Formatting and diff checks pass. DeadSync changes exactly once:
**0.5.1634 -> 0.5.1635**. Cargo.lock updates only the three packages inheriting
that workspace version.

Environment: Windows x64, Intel Xeon E5-2696 v4 at 2.20 GHz (22 cores / 44
threads), rustc 1.98.1 / LLVM 22.1.8, opt-level 3 and fat LTO, mlua 0.12.1
with vendored Lua 5.4.9. The existing thread-local scoped System allocator
records allocation, reallocation, free and requested/freed-byte counts in a
separate operation from timing. QueryThreadCycleTime supplies CPU cycles;
wall time independently supplies useful-unit throughput.

```powershell
cargo test --locked -p deadsync-song-lua --lib -- --test-threads=1
cargo test --locked -p deadsync-song-lua --release --lib -- --test-threads=1
cargo clippy --locked -p deadsync-song-lua --lib --tests
cargo test --locked -p deadsync-song-lua --release --lib retained_values_bench -- --ignored --nocapture --test-threads=1
# Reverse implementation order on alternating runs:
$env:DEADSYNC_PERF_REVERSE = '1'
```

Excluded from the commit: `deadsync-song.json.gz`, `rust-performance.md`,
`optimize.sh` and `optimize.ps1`.
