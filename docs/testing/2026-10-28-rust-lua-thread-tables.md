# Selected-thread table and comparison APIs

`lua_gettable`, `lua_getfield`, `lua_settable`, `lua_setfield`, `lua_equal` and
`lua_lessthan` now execute through another thread's stack. Previously their
declarations existed, but the native adapter rejected non-raw access and
comparisons on other states. Six corresponding `NativeStack` methods expose
the same operations to Rust hosts.

The selected stack becomes the execution context while its metamethods run.
Normal completion preserves its status and restores the caller. Runtime errors
retain the fault frames and follow the selected state's protection chain or
panic callback. A handler on a running child cannot catch its waiting parent's
error. Abandoned children keep their normal status and open cells until
collection closes them.

Two additional fixes were required. A returning tag method restores the original
CallInfo base of a yielded C activation; primitive accesses and table-valued
metamethod chains do not. The action driver now distinguishes those outcomes.
A selected stack window also needs the tag method's ordinary C-call charge,
because it has no real active C callback to supply that charge. Without it,
sixty recursion controls allowed one extra call.

The [Windows release comparison](2026-10-28-rust-lua-thread-tables-current.json),
[Windows debug comparison](2026-10-28-rust-lua-thread-tables-focused.json) and
[i686 SSE2 comparison](2026-10-28-rust-lua-thread-tables-i686-current.json)
match all 3,468 observations against release and assertion-enabled Lua 5.1.5:

- 240 normal operations cover five idle/suspended/failed thread contexts, all
  six APIs, table/userdata handlers, table-valued chains, nil/false/negative-zero
  results, collection in callbacks and surviving mutable upvalues.
- 240 panic cases cover eight error-value types, reset stacks, thread identity,
  status and closed upvalues.
- 60 recursive Lua/C controls check the C-call limit through every operation
  and target context.
- 48 successful waiting-parent cases use main/coroutine parents, Lua/C resumers
  and nested protection. Parent locals and open cells survive the API call.
- 2,880 waiting-error cases cover Lua `pcall`/`xpcall`, coroutine resume and
  protected C API entry on all five target states, with and without handlers.
  They check handler context, error identity, abandoned frame shapes, weak
  reachability and both parent and child upvalues.

The [Windows baseline](2026-10-28-rust-lua-thread-tables-before.json) and
[i686 baseline](2026-10-28-rust-lua-thread-tables-i686-before.json) use preserved
executables from commit `3cc311e9c`. Both references complete the corpus while
Rust rejects its first selected lookup. The
[first fixture record](2026-10-28-rust-lua-thread-tables-fixture-first.json)
retains an erroneous assumption that string errors were location-free and a
checked-run module-resolution mistake. Explicit error level zero and absolute
module paths correct those fixture issues. Nil and false branches also use
explicit conditionals so Lua's `and`/`or` idiom cannot discard those values.
The [call-count failure](2026-10-28-rust-lua-thread-tables-counts-first.json)
retains all sixty unequal recursion rows before the charge fix.

The [initial debug CLI run](2026-10-28-rust-lua-thread-tables-stack-first.json)
also overflowed its Windows entry stack during valid recursive C calls. A
separately preserved executable and stack-reserve probes reproduce the failure;
one, four and eight MiB reservations failed, while sixteen MiB completed the
isolated control. The production CLI now executes Lua on a thread with a
32 MiB stack on all platforms. The native stress test uses the same reservation.
Embedding hosts still choose their own thread stacks; these checks do not prove
general resource/OOM parity.

The [portable regression](2026-10-28-rust-lua-thread-tables-portable.json) runs
the same 240 normal observations through the Rust APIs with default features
disabled. [Full validation](2026-10-28-rust-lua-thread-tables-validation.json)
passes 803 tests on Windows x64 and 804 on Linux x64 and i686 SSE2, strict
Clippy, formatting, builds without default features and an empty normal/build
dependency tree. The [library audit](2026-10-28-rust-lua-thread-tables-stdlib.json)
matches all 5,507 argument cases. Both native-enabled i686 stock CLIs pass
unchanged `main.lua` and `all.lua`; T/testC remains absent. Windows retains
17/18 independent passes in all four modes and the shared `files.lua:59`
rename failure.

The [combined native audit](2026-10-28-rust-lua-thread-tables-boundary-current.json)
also rechecks the earlier native corpora. Forty successful Rust processes balance
1,184,833 operations, 18,130 state lifetimes and fifty library lifetimes.
Twenty-five expected terminating processes remain separate. The
[evidence manifest](2026-10-28-rust-lua-thread-tables-evidence.json) verifies current
sources, reports, logs, binaries, modules and the checked-reference chain. The
[i686 verification](2026-10-28-rust-lua-thread-tables-i686-verification.json)
checks Linux artifact hashes, reference sources and assertion-enabled builds.

Full Lua 5.1 parity remains open. This corpus does not establish permitted
metamethod-yield continuations or arbitrary reader/body/API interleavings.
Exact heap counts, collector work/allocation charges and timing, the retained
unrooted package-name mismatch, broader diagnostics and resource/OOM behavior
remain incomplete. Earlier strict failures remain retained. Core declaration
coverage stays 76/80: `lua_close`, `lua_getallocf`, `lua_newstate` and
`lua_setallocf` are still missing.
