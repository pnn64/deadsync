# Native coroutine resume and yield

NativeStack::resume_thread now resumes a thread through the shared scheduler.
It keeps yielded values, successful results and errors on that thread's native
stack, preserving the caller's window and values below a fresh entry function.
Lua coroutine.resume continues to transfer results to its caller and add its
success boolean. The two entry points can resume each other's suspended threads.

NativeStack::yield_results ends the host callback by returning through Rust.
Resumption supplies the callback's return values without invoking it again.
The development C adapter adds lua_resume and lua_yield; a C callback uses
return lua_yield(L, n). Yielding does not suspend a Rust stack or jump across
Rust. Main-thread, protected-call and metamethod yield restrictions remain in
force. Production consumers continue to use deadlib-lua-legacy.

The compatibility work also corrects failed stack windows: error('bad', 0)
must discard its level argument before lua_resume appends the error copy.
An invalid entry retains its raw diagnostic and the error copy. Suspended
debug locals use the thread's current public stack, including staged values;
the temporary value pushed by debug.setlocal can name the slot above that
stack without leaving an extra value behind. Return hooks wait for resumption,
see the original C window plus new arguments and their hook table, and can
modify the actual returned slots. Both native yield and coroutine.yield are
checked against the same reference fixture.

The unchanged C module is compiled separately against source-built official
Lua 5.1.5 and the development bridge. The new resume fixture covers values and
nil order, object identity, prefixes, successful reuse, native and callable
entries, rejected active/failed resumes, parent-window mutation, five fault
windows, invalid entries, suspended locals, return-hook aliases, nested resumes,
yield restrictions, collection and 1,000 resume/yield cycles. Its 1,518 output
bytes match exactly; all 42,829 Rust dispatch guards drop before C can jump.
Its 1,023 states are destroyed, including 1,022 collected during execution.

Five additional portable integration groups check native/Lua resume mixing,
error status and restrictions, normal Rust destruction at yield without callback
reentry, argument validation before mutation, and instruction-limit propagation
with escaped child-cell recovery. Fresh isolated builds pass 697 Rust tests on
Windows x64 and 698 on Linux x64 and i686 SSE2, plus strict Clippy and formatting.
The engine still has no normal/build dependencies. Eighteen imported-name chunks
match direct, reference and both cache modes on all three platforms. Unchanged
stock main.lua and all.lua pass in both CLIs on i686 SSE2.

Raw [C-boundary](2026-10-27-rust-lua-native-resume-boundary.json),
[Windows](2026-10-27-rust-lua-native-resume-windows.json),
[Linux](2026-10-27-rust-lua-native-resume-linux.json),
[i686](2026-10-27-rust-lua-native-resume-i686.json) and
[stock-suite](2026-10-27-rust-lua-native-resume-suite.json) reports retain commands
and observations. [Validation metadata](2026-10-27-rust-lua-native-resume-validation.json)
binds sources, workspace inputs, compiler logs, ABI headers and private artifacts.
Final target directories start absent. Existing boundary, parser-root and
thread-transfer probes remain independent checks with their original markers.

The build snapshot uses workspace version 0.5.2474; the committed workspace
advanced to 0.5.2475 during validation. A separate clean checkout confirms that
only three unrelated package versions changed in the lockfile. Every Lua
dependency record and selected feature tree is identical on all three targets.
Validation pins the committed input blobs and both sets of manifests; ongoing
uncommitted dependency upgrades are excluded from this Lua milestone.

The development boundary declares 65 of 80 core APIs; declarations and these
selected traces do not establish full semantics. lua_setlevel, native hook/debug
APIs, state lifecycle, allocator/OOM behavior, other inactive operations that
execute Lua, broader diagnostic/parser scratch behavior and complete standalone
Lua 5.1 parity remain unfinished. T/testC and suite native modules remain absent.
Production package.loadlib remains unsupported; the retained
[argument audit](2026-10-26-rust-lua-stdlib-arguments.md) records fourteen
native-loader differences.
Later dialects and integration remain open. Optimization, profiling and
benchmarking remain deferred until the full standalone Lua 5.1 parity gate passes.
