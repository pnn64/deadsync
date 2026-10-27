# Reader and iterator C-call limits

Lua load(reader) now uses the shared Lua 5.1 C-call check. The previous separate
test rejected every count at or above 199, which disagreed with unsigned wrapping
and Lua's error reserve. At count 65535 the reference enters a reader at zero;
at count 200 it can enter a reader at 201 before a syntax-level check fails.
The old path could reject the load before invoking the reader at all.

Generic-for calls now check changes made by lua_setlevel even when their saved
structural depth is small. A call at count 199 fails before invoking its Lua or
native iterator. Protected recovery restores the entry count. Both fixes use
the existing counter model; no runtime layout or dependency changes are needed.

The C fixture compares 24 reader scenarios across ordinary, wrapped and inherited
high counts: successful text, empty input, an initial empty probe, syntax errors,
reader errors and invalid reader values. It also checks two iterator entry
failures. Explicit chunk names preserve diagnostic bytes. Two added portable
groups exercise the same reader and iterator paths, including parser recovery.

The retained lowered-count yield case was investigated separately. Its release
comparison still differs and the strict audit still exits 1. A second reference
build keeps every official Lua 5.1.5 source file unchanged and force-includes a
lua_assert handler that prints the failing expression and exits 86. This avoids
interactive CRT assertion dialogs. That build rejects the lowered-count yield
case at ldo.c:435, where the reference requires
`L->nCcalls == L->baseCcalls` before lua_resume completes. The corresponding
control without the counter mutation completes normally, and all 26 reader/
iterator cases complete with assertions enabled and match the ordinary reference.
The assertion is also visible in the [official resume source](https://www.lua.org/source/5.1/ldo.c.html).

This evidence diagnoses an internal invariant violation; it does not turn the
release difference into a match or establish complete coroutine/C API parity.
The existing strict audit and its raw observations remain in place.

Fresh isolated runs pass 704 Rust tests on Windows x64 and 705 each on Linux x64
and i686 SSE2, with formatting and strict Clippy. The normal/build dependency
tree remains empty. Imported-name chunks match reference, direct execution and
both cache modes on all platforms. Unchanged stock main.lua and all.lua pass
in both i686 SSE2 CLIs. The separate release lowered-count yield audit remains
a recorded failure; the new checked-reference diagnostic passes its controls.

Raw [boundary](2026-10-27-rust-lua-native-reader-boundary.json),
[checked reference](2026-10-27-rust-lua-native-reader-checked.json),
[release yield audit](2026-10-27-rust-lua-native-reader-yield.json),
[Windows](2026-10-27-rust-lua-native-reader-windows.json),
[Linux](2026-10-27-rust-lua-native-reader-linux.json),
[i686](2026-10-27-rust-lua-native-reader-i686.json),
[stock suite](2026-10-27-rust-lua-native-reader-suite.json) and
[validation](2026-10-27-rust-lua-native-reader-validation.json) reports bind the
source snapshots, commands, compiler logs, ABI headers and source-built artifacts.
The assertion header/handler are recorded explicitly. The normal reference is
unchanged. Final Cargo targets start absent and committed workspace inputs
exclude unrelated uncommitted dependency edits.

Complete standalone Lua 5.1 parity, the remaining C APIs, allocator/OOM behavior,
production native modules, broader GC/resource and debug behavior, later dialects
and gameplay integration remain unfinished. T/testC and native suite modules
remain absent. Optimization, profiling and benchmarks stay deferred.
