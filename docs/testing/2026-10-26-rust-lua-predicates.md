# Statement predicates and observable control flow

`if`, `while` and `repeat` now lower conditions through the compiler's pending
true/false edges. Local tests read the live register at their callback boundary;
comparisons use verified conditional branches without a boolean materializer.
Logical predicates preserve short-circuit calls and negations. The parser keeps
an absent else distinct from an explicit empty else, whose native jump remains
observable. A terminal nil descriptor becomes false before its final test,
matching the reference's instruction events even at function entry.

Compiler locals track capture. Uncaptured block exits omit Close; captured
repeat scopes emit the reference's separate success and back-edge closes, and
break closes live captured scopes. Shared unconditional targets resolve in a
linear graph pass with cycle detection. Value-chain test sharing is restricted
to value-producing logical branches, so an elseif test retains its own event.
These are compiler changes; no runtime field, opcode or dependency is added.
New compiler paths use safe Rust. Existing isolated platform shims remain.

| Check | Result |
| --- | --- |
| Rust unit / integration / doc tests | 69 / 468 / 4 = 541 |
| Formatting / strict Clippy | Pass |
| Normal/build dependencies | Zero |
| CLI raw stdout/stderr/status | 176/214 |
| CLI with one exact leading version banner removed | 214/214 |
| New predicate CLI cases | Five parent failures; five current exact matches |
| Process probes | 140/140 |
| Official C / Rust / cache store / fresh cache hit | 17/18 in each mode |
| Official cache outcomes | 18 Stored; 18 Hit |
| Cache build identity | 16/16 |

Five new hook test groups cover local/comparison/logical predicates, all truth
classes, nil-at-entry, if/elseif, absent/empty else, while/repeat, callback writes
to parameters, collection and both captured-scope exits. Native export tests
compare selected predicate hook traces and loop/capture results in both engines.
Persistent-cache process coverage adds a fifth valid entry (four text, one
native), each stored and hit in independent children and reexecuted after GC.

The first fresh process run passed 139/140: the Rust calendar_format_2 child
timed out at ten seconds while C and the CLI returned the expected CRT abort.
The [initial report](2026-10-26-rust-lua-predicates-process-timeout.json) is retained. The unchanged
repeat passed 140/140. This rerun does not change timeouts or comparisons.

The byte-identical preserved e4b9b2ee5 parent CLI is pinned by its committed
[ordinary-build audit](2026-10-26-rust-lua-boolean-hooks-audit.json).
The supplementary [parent audit](2026-10-26-rust-lua-predicates-parent-audit.json) fails exactly
predicate_if_hook_events, predicate_loop_hook_events,
predicate_captured_repeat_hooks, predicate_live_local_hooks and
predicate_terminal_nil_hooks. The [current audit](2026-10-26-rust-lua-predicates-audit.json)
matches raw reference output for all five. Its exit 1 is retained for 38 known
version-banner differences. The supplementary comparison removes at most one
exact known leading banner; it does not normalize arbitrary output.

All eighteen official sources and assertions retain the previous hashes.
Only files.lua:59 fails, at the Windows open-input rename, in all four modes
including C. The suite exits 1. Bootstrap _U=true, 100,000,000 instructions and
60-second file deadlines remain. Archive SHA-256 is
49e4ca6561f82ea605908c5041ab5fad66ed9930fa0686675bd51b02767f18ad.
These independent entries do not establish all.lua/main.lua or full Lua 5.1;
earlier Windows module/shell failures remain open.

The retained [known-gap fixture](lua51-predicate-known-gaps.lua) and
[observations](2026-10-26-rust-lua-predicates-known-gaps.json) expose two additional open details.
Its source loop has 13 instruction events in both engines; the exported loop
has 19 in Rust versus 13 in C because operand loads/copies remain. Unnamed
source-frame slots queried from a hook also differ from the C
stack view. These failing observations are outside the passing selected CLI
corpus. Complete debug/hook parity is not claimed.

All fourteen full source maps, including the initial process timeout report,
match the final Lua sources and recorded build
inputs. Every retained Cargo profile builds in a new, initially absent target
directory, so copied source timestamps cannot admit older profile artifacts.
The [snapshot manifest](../performance/2026-10-26-rust-lua-predicates-workspace-inputs/manifest.json)
retains the isolated checkout, target path and Cargo manifest/lock hashes.
Source bytes for the Lua crate, legacy oracle bridge and vendored C are copied
exactly; workspace input overrides in performance JSONs resolve the frozen
manifest/lock snapshots. Process and suite runners now honor CARGO_TARGET_DIR.
The ordinary CLI build runs last, after all other builders; focused timing
reads the pinned executables without rebuilding. Official CLI archive SHA is
2640fc56a795f29d28ef15e13c34a47e223960b0240e8cb0a82d9b0738695333.
The contract remains x64 Windows, MSVC /O2 /MT, binary64, 64-bit Lua integers,
32-bit native ints and C locale.

Two broad runs retain 81 runtime and 22 frontend fixtures, alongside two cache,
callback and focused parent/current/C runs. 74 of the preceding 103
layouts change; both current runs agree. Vm remains 4,440 B, Value 16 B and
Frame 64 B. [Measurements](../performance/2026-10-26-rust-lua-predicates.md) retain selected wins,
slow paths, allocation counts, heap estimates, CPU counters and limits.
Raw [process](2026-10-26-rust-lua-predicates-process.json), [suite](2026-10-26-rust-lua-predicates-suite.json) and
[identity](2026-10-26-rust-lua-predicates-identity.json) results remain available.
Full standalone Lua 5.1, later dialects, machine-code compilation and gameplay
integration remain unfinished. The full goal stays active.

~~~sh
cargo test -p deadlib-lua
cargo fmt -p deadlib-lua --check
cargo clippy -p deadlib-lua --all-targets --no-deps -- -D warnings
python scripts/check_rust_lua_process.py --output docs/testing/2026-10-26-rust-lua-predicates-process.json
python scripts/check_rust_lua_suite.py --cached --output docs/testing/2026-10-26-rust-lua-predicates-suite.json
python scripts/check_rust_lua_cache_identity.py --output docs/testing/2026-10-26-rust-lua-predicates-identity.json
python scripts/check_rust_lua_cli.py --output docs/testing/2026-10-26-rust-lua-predicates-audit.json
~~~
