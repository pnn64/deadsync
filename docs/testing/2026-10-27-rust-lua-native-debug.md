# Native activation records and local inspection

`NativeStack` now supplies Lua 5.1 stack records, function information and local
reads/writes. The development C boundary exposes `lua_getstack`, `lua_getinfo`,
`lua_getlocal` and `lua_setlocal`. Records contain the visible CallInfo ordinal;
internal scheduler continuations do not appear as extra inspected functions.

Information requests preserve unrequested fields and event/record indexes.
Invalid options still apply valid fields and stack effects. Function inspection
pops its target; duplicate options push the function once, then active lines
once, regardless of option order. NUL ends the options. Tail records initialize
all fields even for invalid options. The complete 60-byte source buffer is
compared, including zero padding for '=' sources and retained trailing bytes
for other source forms.

Local reads push onto the inspected thread. Setters find the slot while their
input is still present, write before popping, and pop even for an absent local.
Names and sources share interned string storage. Program tracing retains newly
exposed names while their program is reachable; unreachable program metadata
and its strings can be reclaimed. Neither native handle roots its owner.

The C fixture compares active calls, Lua parents, normal/waiting parents,
fresh/suspended/failed/finished coroutines, native yields and lost tail calls.
It includes Lua and native function inspection, a dumped/reloaded function,
source truncation/padding, invalid and duplicate options, NUL termination,
local mutation and 1,000 source-pointer/collection cycles. Four portable Rust
groups also cover invalid and collected handles, the empty-thread base record,
top-slot aliasing, unconditional setter pops, 8,000-slot overflow, interned
pointer identity and program reclamation.

The selected isolated runs pass 708 Rust tests on Windows x64 and 709 each on
Linux x64 and i686 SSE2, with formatting and strict Clippy. Normal/build
dependencies remain empty. Eighteen imported-name chunks match direct and both
cache modes on each platform. Unchanged stock `main.lua` and `all.lua` pass in
both i686 SSE2 CLIs. The debug C fixture matches all 15,690 stdout bytes; its
12,209 dispatch guards balance, and all six native states are destroyed.

The source-built [C boundary report](2026-10-27-rust-lua-native-debug-boundary.json)
and [validation](2026-10-27-rust-lua-native-debug-validation.json) bind sources,
commands, compiler logs, API declarations, artifacts and observations. Additional
reports retain [Windows](2026-10-27-rust-lua-native-debug-windows.json),
[Linux x64](2026-10-27-rust-lua-native-debug-linux.json),
[Linux i686 SSE2](2026-10-27-rust-lua-native-debug-i686.json),
[unchanged stock suite](2026-10-27-rust-lua-native-debug-suite.json),
[strict release yield mismatch](2026-10-27-rust-lua-native-debug-yield.json) and
[checked-reference controls](2026-10-27-rust-lua-native-debug-checked.json).

The [first strict boundary run](2026-10-27-rust-lua-native-debug-boundary-first.json)
failed: the existing parser-root probe's reference process exited with Windows
heap-corruption status `0xc0000374`, after 328,383 stdout bytes. Rust completed
that probe, and the new debug probe matched on both engines. Replaying the
same reference binary with the same bindings, and with the three new bindings
removed, completed and matched Rust in both [diagnostic controls](2026-10-27-rust-lua-native-debug-roots-replay.json).
The cause of the nonreproduced reference failure remains open. Its failed report,
artifacts and raw output are retained. The strict probe's success criteria are
unchanged; a later successful run does not erase this failure or establish parity.

This closes four missing development C declarations (70/80). Declaration counts
and selected matching observations do not prove complete debug/C API semantics
or standalone parity. Native hooks, allocator/OOM and state lifecycle APIs,
production native modules and broader language/library/GC/resource coverage
remain open. The strict lowered-count yield comparison still fails; the checked
reference diagnoses its counter invariant. Performance tuning, profiling and
benchmarks remain deferred until full standalone Lua 5.1 parity is demonstrated.
