# Native closure upvalues

`NativeStack::get_upvalue` and `set_upvalue` inspect and mutate Lua and native
captures with Lua 5.1 stack effects. The development C boundary exposes them as
`lua_getupvalue` and `lua_setupvalue`. A successful read pushes the value; a
successful write consumes its replacement. A missing capture or nonfunction
returns no name and leaves the top unchanged. Lua capture names use the retained
native debug-name count; native captures have an empty name.

Native closures now keep their captures in one traced array of Lua values.
Pattern iterators read and update their source, pattern and cursor there;
coroutine wrappers read their thread there; file iterators read their file and
close flag there. `pairs` and `ipairs` return their actual captured iterator.
`newproxy` captures an ordinary weak table whose own metatable is itself and
whose `__mode` is `kv`, matching official Lua. The previous private set of valid
metatables is removed. C upvalue writes therefore affect both subsequent
execution and collector reachability.

The three capturing base functions are owned native closures. Their capture
cycles can be collected after the functions lose their roots, and retired
handles are rejected. Heap statistics now count these three closures. The
standalone message-handler reclamation test measures its baseline before
checking that all temporary handlers are reclaimed.

Lua upvalue names enter the VM's existing NUL-terminated string storage when
the program is bound. The loaded program traces them until the program and
its closures become unreachable. Reads and writes return borrowed pointers
into this storage, which remains stable through collection, nested callbacks
and VM moves. No temporary name buffer is returned across the C boundary.
Lua cell writes and native capture writes use the existing collector barriers.
Native binding data and DLL ownership remain separate from Lua captures.

The identical C fixture is compiled against official Lua 5.1.5 and the Rust
development boundary on Windows/MSVC x64. It checks positive, negative and
capture pseudo-indexes; absent/nonfunction targets; negative and large capture
numbers; stack deltas; shared open, closed and suspended cells; C closure
captures through slot 255; callbacks that collect and reenter native code; and
all builtin capture families. It mutates the `newproxy` validation table and
replaces iterator functions, pattern strings/cursors, files/close flags and
wrapper threads, then verifies the resulting execution.

Nine official native fixtures retain zero, one or two names with ordinary,
empty or embedded-NUL text. Their executable words and capture counts remain
unchanged. The C API checks names, borrowed-pointer lifetime, shared-cell
mutation and hidden stripped captures through both engines. Previous fixture
cases, including auxiliary buffers and C-only error jumps, remain enabled.

| Check | Result |
| --- | --- |
| Shared C fixture | Both processes exit zero; all 16,871 stdout bytes equal |
| C boundary cleanup | 58,297 dispatches and 58,297 drop guards; release `panic=abort` |
| Native state lifetime | 74 created, 73 collected before shutdown, all 74 destroyed |
| Rust tests per platform | 640 pass: 77 unit, 559 integration, 4 documentation |
| Imported-name audit per platform | All 18 chunks match in C, Rust direct, cache store and fresh-process cache hit |
| Stock Linux i686 SSE2 suite | Unchanged `main.lua` and `all.lua` pass in both engines |
| Static checks | Rustfmt and strict Clippy; normal/build dependencies contain only `deadlib-lua` |

Six new Rust groups check shared cells, builtin execution, capture cycles, name
lifetime, stack bounds and sticky unprotected errors with subsequent recovery.
Two new collector groups force propagation states: capture writes from already
traced native closures must immediately mark old white graphs, and retained
programs must trace their interned upvalue names after compiler owners are gone.
The regression suites and imported-name audits run on Windows x64, Linux x64
and Linux i686 SSE2 from fresh Cargo targets.

Raw [C-boundary](2026-10-27-rust-lua-native-upvalues-boundary.json),
[Windows](2026-10-27-rust-lua-native-upvalues-windows.json),
[Linux](2026-10-27-rust-lua-native-upvalues-linux.json),
[i686](2026-10-27-rust-lua-native-upvalues-i686.json) and
[stock-suite](2026-10-27-rust-lua-native-upvalues-suite.json) observations are
retained. [Validation metadata](2026-10-27-rust-lua-native-upvalues-validation.json)
binds source snapshots, workspace inputs, toolchain commands, logs, ABI headers
and private executable artifacts. The proof builds its final runner from source.

The development header now includes these two upvalue operations; it still
does not provide the complete C API or production module loading. Inactive and
cross-thread C stacks, lifecycle/allocator/debug/reader/writer APIs, internal
T/testC conformance tests and broader resource behavior remain open. Invalid
typed builtin captures can reach undefined behavior in the C implementation;
the execution comparisons use defined reference cases. Full standalone Lua 5.1
parity remains unproven. Later dialects, performance tuning and gameplay
integration remain deferred. The legacy engine remains the production runtime.
