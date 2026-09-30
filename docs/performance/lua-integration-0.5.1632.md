# Lua argument transfer, loader keys and JSON tables - 0.5.1632

Parent: `c7747938f` (0.5.1631). Date: 2026-09-30.

This pass follows the local performance guide's M-HOTPATH, M-MEM-REUSE,
M-INITIAL-CAPACITY and M-THROUGHPUT recommendations with three changes:

1. Runtime `cmd` builders reuse the incoming MultiValue argument buffer after
   consuming the command name. Invocation passes a borrowed actor and argument
   list directly to mlua, removing the temporary argument vector, repeated
   value-handle clones and its growth. The captured list is immutable during
   calls, including reentrant calls. Name-only builders release the incoming
   buffer. Method lookup remains dynamic; missing methods return the actor,
   explicit nil arguments keep their positions and count, return values are
   ignored, and method/lookup errors still propagate in the same order.
2. Loader environment keys format pointer text into a stack buffer sized for
   the target's hexadecimal pointer width, then create the Lua string directly.
   Existing `0x...` registry keys, environment replacement, aliases, missing
   entries, metamethods and errors remain compatible. Interned keys avoid the
   temporary Rust String allocation on warm registration and retargeting.
3. JSON conversion reserves Lua array storage for dense arrays and hash storage
   for non-null object fields. Arrays containing nulls retain their original
   growth path because Lua length can depend on sparse-table layout. Null
   object entries still disappear. Numeric conversions, recursive values,
   insertion order and independently owned output tables remain unchanged.

The changes add no dependency, public API change, unsafe code or persistent
cache. Command names and captured arguments still need owned storage. The
borrowed call path itself creates no Rust argument buffer at any arity; Lua
methods and Rust callbacks can still allocate for their own work. Lua strings
and JSON output tables retain their required storage.

## Measurements

The [raw CSV](lua-integration-0.5.1632.csv) contains 208 rows: 26 workloads,
two implementations and four independent runs. Percentage ranges cover all
four paired runs. Absolute figures use run 1.

| Workload | Thread cycles/op, old -> new | CPU reduction | Throughput gain | Allocations/reallocations, old -> new | Requested bytes, old -> new |
| --- | ---: | ---: | ---: | ---: | ---: |
| 64 command calls / 0 arguments | 75,165.7 -> 52,462.9 | 26.1% to 33.4% | 35.3% to 50.3% | 128/0 -> 0/0 | 11,264 -> 0 |
| 64 command calls / 1 argument | 78,556.7 -> 54,884.6 | 30.1% to 40.2% | 43.1% to 67.2% | 128/0 -> 0/0 | 11,264 -> 0 |
| 64 command calls / 4 arguments | 97,468.5 -> 55,933.0 | 42.1% to 44.9% | 72.9% to 81.3% | 128/64 -> 0/0 | 31,744 -> 0 |
| 64 command calls / 8 arguments | 116,749.5 -> 59,987.5 | 48.6% to 50.9% | 94.7% to 103.8% | 128/128 -> 0/0 | 72,704 -> 0 |
| 64 command calls / 64 arguments | 288,337.3 -> 94,512.5 | 67.1% to 67.5% | 204.2% to 208.1% | 128/320 -> 0/0 | 646,144 -> 0 |
| 64 command calls / 257 arguments | 849,278.1 -> 217,949.2 | 73.5% to 74.3% | 277.1% to 289.8% | 128/448 -> 0/0 | 2,612,224 -> 0 |
| 64 loader registrations | 64,665.7 -> 46,975.8 | 27.2% to 29.3% | 37.3% to 41.4% | 64/64 -> 0/0 | 1,536 -> 0 |
| 64 loader retargets | 78,030.1 -> 63,309.5 | 18.9% to 20.7% | 23.2% to 26.0% | 64/64 -> 0/0 | 1,536 -> 0 |
| 64 missing-loader lookups | 59,343.7 -> 43,940.1 | 25.2% to 29.5% | 33.8% to 41.9% | 64/64 -> 0/0 | 1,536 -> 0 |
| Dense JSON array / 64 items | 12,791.0 -> 9,334.8 | 19.6% to 27.0% | 28.4% to 43.5% | 2/6 -> 2/0 | 2,088 -> 1,080 |
| Dense JSON array / 512 items | 66,405.7 -> 59,103.8 | 5.3% to 11.0% | 6.3% to 12.4% | 2/9 -> 2/0 | 16,424 -> 8,248 |
| JSON object / 64 fields | 28,791.4 -> 22,479.9 | 19.6% to 21.9% | 25.1% to 30.6% | 8/0 -> 2/0 | 3,104 -> 1,592 |
| JSON object / 512 fields | 212,668.0 -> 167,758.0 | 17.9% to 24.2% | 23.0% to 32.3% | 11/0 -> 2/0 | 24,608 -> 12,344 |
| JSON object / 512 fields, half null | 181,629.4 -> 161,070.1 | 8.4% to 13.3% | 9.3% to 16.1% | 10/0 -> 2/0 | 12,320 -> 6,200 |
| JSON / 64 nested records | 252,833.9 -> 164,725.3 | 34.2% to 35.5% | 52.4% to 55.8% | 386/134 -> 258/0 | 27,176 -> 17,464 |

All new warmed command and loader batches have zero allocation, reallocation,
free and byte churn in every run. Command argument ownership remains in the
created closure; the runtime call no longer constructs a carrier buffer.

64 command calls with one argument: allocations/reallocations/frees change from 128/0/128 to 0/0/0; requested/freed bytes change from 11,264/11,264 to 0/0.

64 loader retargets: allocations/reallocations/frees change from 64/64/64 to 0/0/0; requested/freed bytes change from 1,536/1,536 to 0/0.

A 4,096-item dense JSON array: allocations/reallocations/frees change from 2/12/1 to 2/0/1; requested/freed bytes change from 131,112/360,432 to 65,592/294,912.

A 512-field JSON object: allocations/reallocations/frees change from 11/0/523 to 2/0/514; requested/freed bytes change from 24,608/118,154 to 12,344/105,890.

64 nested JSON records: allocations/reallocations/frees change from 386/134/577 to 258/0/449; requested/freed bytes change from 27,176/51,888 to 17,464/43,200.

Allocator counts agree across all four runs for every workload and implementation.
Sparse JSON fixtures retain identical allocator work in both implementations.

## Controls and limits

Negative percentages mean higher cost or lower throughput. Empty, small,
sparse and partially null inputs are measured alongside larger dense inputs.

| Workload | Thread cycles/op, old -> new | CPU reduction | Throughput gain | Allocations/reallocations, old -> new | Requested bytes, old -> new |
| --- | ---: | ---: | ---: | ---: | ---: |
| Cold command construction / 0 arguments | 4,904.4 -> 5,232.0 | -6.8% to -3.8% | -10.0% to -5.4% | 8/0 -> 8/0 | 384 -> 392 |
| Cold command construction / 1 argument | 5,525.2 -> 5,449.8 | -5.7% to 4.4% | -5.8% to 7.7% | 9/0 -> 8/0 | 584 -> 432 |
| Cold command construction / 8 arguments | 6,696.5 -> 6,079.1 | 5.6% to 11.1% | 9.3% to 19.7% | 9/0 -> 8/0 | 1,024 -> 712 |
| Cold command construction / 64 arguments | 12,310.9 -> 11,213.4 | 8.9% to 19.2% | 11.5% to 27.1% | 9/2 -> 8/2 | 9,504 -> 6,952 |
| Empty JSON array | 1,809.1 -> 1,790.3 | -0.6% to 1.0% | -1.3% to 4.0% | 1/0 -> 1/0 | 56 -> 56 |
| Dense JSON array / 1 item | 2,330.5 -> 2,374.2 | -1.9% to 7.5% | 2.7% to 15.7% | 2/0 -> 2/0 | 72 -> 72 |
| Dense JSON array / 8 items | 4,911.4 -> 3,280.5 | 20.1% to 33.2% | 53.1% to 90.4% | 2/3 -> 2/0 | 296 -> 184 |
| Dense JSON array / 4,096 items | 626,017.4 -> 504,992.3 | -13.7% to 19.3% | -12.2% to 24.3% | 2/12 -> 2/0 | 131,112 -> 65,592 |
| Sparse JSON array / 64 positions | 24,509.4 -> 24,117.6 | -3.6% to 2.8% | -3.6% to 2.6% | 16/19 -> 16/19 | 7,424 -> 7,424 |
| Sparse JSON array / 512 positions | 132,121.8 -> 129,088.2 | -0.2% to 2.3% | -0.6% to 2.2% | 37/43 -> 37/43 | 85,688 -> 85,688 |
| JSON object / 8 fields | 5,906.8 -> 4,510.1 | 18.8% to 24.0% | 31.1% to 52.0% | 5/0 -> 2/0 | 416 -> 248 |

Controls with higher CPU cost in every run: Cold command construction / 0 arguments.
Controls with mixed CPU changes: Cold command construction / 1 argument, Empty JSON array, Dense JSON array / 1 item, Dense JSON array / 4,096 items, Sparse JSON array / 64 positions, Sparse JSON array / 512 positions.
The largest dense array has mixed CPU timings despite eliminating all twelve
growth reallocations and halving requested bytes; no uniform CPU improvement
is claimed for that size.
The capacity scan adds work before JSON conversion, including an early-exit
scan on sparse arrays. Cold builder timings include closure construction,
whose cost can dominate the removed argument copy. All controls and timing
ranges remain in the CSV.

Command construction reuses a VecDeque buffer that can retain its consumed
name slot, whereas the parent stored a Vec of only the arguments. Its container
metadata is also slightly larger. Name-only commands release that buffer.
Reduced churn during construction or invocation does not imply lower retained
memory for every command closure. No peak RSS or whole-game loading/frame-rate
improvement is claimed. Requested/freed bytes measure allocator work.

Cold command builders and JSON conversion use fresh Lua fixtures. Their output
Lua objects and Rust captures can remain alive until the fixture is destroyed
outside the measured window. Input JSON fixtures are constructed/cloned outside
measurement but consumed and freed inside conversion. Lua GC is stopped while
measuring to separate collection pauses from the operations. Object comparisons
check contents and raw lengths; Lua hash-table traversal order is unspecified.

## Method and behavior checks

Five baseline functions are frozen from `c7747938f`: command-helper installation,
loader registration, loader retargeting, pointer-key formatting and recursive
JSON conversion. They call their frozen helpers rather than the new ones.
The source audit checks all five against the parent, modulo formatting, and
verifies Cargo manifest, lock and locked metadata against the exact patch bump.

The existing scoped System allocator harness records allocation, reallocation,
free, requested-byte and freed-byte counts separately from timing. Seven
samples produce median wall time and Windows `QueryThreadCycleTime` calling-
thread CPU cycles. Runs alternate old-first/new-first; all four runs execute
serially after builds, tests and Clippy finish.

Command calls and loader operations use warmed fixtures with loop timing;
each operation is a 64-call batch. Command arities are 0/1/4/8/64/257. Warm
command methods are Lua functions that return nil. Cold builders use 0/1/8/64
arguments and per-operation timing with fresh Lua state setup and destruction
outside measurement. JSON fixtures cover empty through 4,096-element arrays,
sparse arrays, objects, partly null objects and nested object/array records.
Those also use fresh-fixture per-operation timing; its clock overhead is
included equally in both variants. Each JSON fixture warms its string intern
and reference pools before conversion. Throughput is calls/second, builders/
second or top-level JSON items/second; empty JSON uses one nominal item.

Eight new behavior tests pass in debug and release. They cover:

- Exact argument counts through 257 parameters; nil slots, booleans, numeric
  bits, Unicode/NUL and invalid string bytes, table/function/thread/light-userdata
  identity; method replacement after construction and ignored return values.
- Reentrant calls with another actor, partial writes before errors, lookup
  metamethod errors and missing/invalid command names.
- Exact loader pointer keys, environment aliases and replacement, absent
  registries and functions, metamethod key/value observations and error text.
- Zero warm allocation/reallocation/free/byte churn for command transfer at
  every tested arity and for registration, retargeting and missing-loader lookup.
- JSON scalar/numeric types, nested tables, null removal, Unicode/NUL keys,
  independent output ownership and all 2,047 null patterns in lengths 0-10.
- Dense JSON arrays through 512 elements within a zero-reallocation budget.

Both full library suites report **684 passed, 4 failed, 66 ignored**. The four
failures match the parent's assertions and source locations:

- `compile_song_lua_extracts_actorproxy_targets`
- `compile_song_lua_resolves_local_and_hidden_screen_proxy_targets`
- `compile_song_lua_runs_cmd_queuecommand_builders`
- `compile_song_lua_supports_notefield_column_api`

Clippy passes with existing warnings and no new fixture warnings. Formatting
and `git diff --check` pass. Environment: Windows x86_64, Intel Xeon E5-2696 v4
at 2.20 GHz, rustc 1.98.1 (LLVM 22.1.8), workspace release optimization level 3
with fat LTO. Commands:

```text
cargo test --locked -p deadsync-song-lua --lib
cargo test --locked -p deadsync-song-lua --release --lib
cargo clippy --locked -p deadsync-song-lua --lib --tests
cargo metadata --locked --no-deps --format-version 1
cargo test --locked -p deadsync-song-lua --release --lib lua_integration_bench -- --ignored --nocapture --test-threads=1
```

The benchmark runs four times; `DEADSYNC_PERF_REVERSE=1` reverses order for
runs 2 and 4. The CSV contains only the final implementation's release runs.

Workspace version: **0.5.1631 -> 0.5.1632**, exactly one patch increment.
Cargo.toml and Cargo.lock are included. `deadsync-song.json.gz`,
`rust-performance.md`, `optimize.sh` and `optimize.ps1` are outside the commit.
