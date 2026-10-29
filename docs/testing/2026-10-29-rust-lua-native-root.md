# Lua 5.1 persistent native root stack

`Vm::with_native_stack` borrows the idle main thread's complete stack without
adding a visible Lua/C function activation. Values remain collector roots across
entries. Parser, table, protected-call, collector and coroutine operations use
the existing native stack API. Other VM execution methods can replace that stack.

`NativeCAbiLoader::driver` resolves a retained `int (lua_State *)` entry point.
`NativeCDriver::run` invokes it through the existing C-only jump boundary and
returns its integer unchanged, retaining the complete stack. It does not
interpret that integer as a Lua result count. The constructor is unsafe because
the caller must guarantee the entry signature and matching C API image.

The driver retains its shared library. Closures it pushes retain their own
library owner and remain callable after the drivers and loader are dropped.
The same live VM retains its main `lua_State *` identity between entries.
Unprotected Lua errors unwind to the Rust boundary and leave only their error
value, if present. Fatal instruction limits remain sticky even if Rust ignores
a protected-call error. A later entry can recover after the limit is removed.

Base-stack operations have no implicit Rust root-call charge. Selected windows
now charge protected calls explicitly and start their parser at the actual
C-call count. The reference executes 199 recursive C callback bodies before
rejecting the next entry, and rejects the 199-parenthesis parser case. Raw builtin
bodies also work without a current closure or visible owner frame.

The public-header C fixture runs two phases directly on the root stack. The
reference executable calls them directly on `luaL_newstate`, without wrapping
them in `lua_call`. The Rust executable uses `Vm::new_bare` and the new driver.
The phases cover:

- Empty initial stack, absent root debug frame, main-thread identity and direct
  `luaL_openlibs` initialization.
- Retained tables, registry entries, captured C closures and state pointers.
- Reader callbacks, bytecode writer callbacks, reload, result arity, syntax
  failures, protected errors and parser/C recursion limits.
- A raw `math.abs` body, line hooks, userdata finalizers and full collection.
- A coroutine yielding in the first phase and completing in the second.
- Calls after all drivers and the loader are dropped, plus balanced native
  operation, state, group and library lifetimes at VM destruction.

All 54 observations match unmodified release and assertion-enabled Lua 5.1.5 on
[Windows debug](2026-10-29-rust-lua-native-root-debug.json),
[Windows release](2026-10-29-rust-lua-native-root-release.json) and
[Linux i686 SSE2 release](2026-10-29-rust-lua-native-root-i686.json): 162
three-engine observations. The checked builds enable assertions in all reference
translation units. The [i686 verifier](2026-10-29-rust-lua-native-root-i686-verification.json)
validates four ELF32 binaries, reference settings, source and artifact hashes,
and the native-enabled stock suite.

Eight portable root-stack tests and one C integration test run in normal Cargo
validation. [Full validation](2026-10-29-rust-lua-native-root-validation.json)
passes 830 Windows tests and 831 tests on each Linux target, strict Clippy,
formatting and all-target checks without default features. Normal/build
dependencies remain empty. The separate
[portable run](2026-10-29-rust-lua-native-root-portable.json) passes 35 tests.
The [library audit](2026-10-29-rust-lua-native-root-stdlib.json) matches all 5,507
cases. The [native audit](2026-10-29-rust-lua-native-root-boundary-current.json)
retains balanced operation/state/library lifetimes and its expected termination
probes.

Both native-enabled i686 CLIs pass unchanged `main.lua` and `all.lua` in the
[stock suite](2026-10-29-rust-lua-native-root-i686-suite.json); T/testC remain
absent. [Windows](2026-10-29-rust-lua-native-root-windows-suite.json) retains
17/18 independent passes in all four modes, with the common `files.lua:59`
failure. The [evidence manifest](2026-10-29-rust-lua-native-root-evidence.json)
binds the final sources, reports, logs and artifacts and embeds the proof tools.

This milestone provides a Rust-owned root entry. It does not implement C-owned
state creation/destruction, allocator routing, or calls outside an active
driver/native entry. Core declarations remain 76/80: `lua_newstate`,
`lua_close`, `lua_getallocf` and `lua_setallocf` are still missing. Standalone C
panic/lifecycle semantics, exact heap/GC accounting, general continuation
interleavings and OOM/resource behavior remain open. Full Lua 5.1 parity is not
established.
