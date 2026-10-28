# Native library resolution and package integration

The VM can now load native modules through an explicit host ABI adapter.
`Vm::set_native_library_loader` installs a `NativeLibraryLoader`; opened
`NativeLibrary` objects resolve `NativeSymbol` callbacks and private binding data.
Opening and symbol lookup run within the package routine's activation, with no
extra Lua frame or call/return hook. Resolved functions inherit the actual
calling closure's environment, including copied builtin bodies.

`package.loadlib` and C/C-root searchers share a resumable loading path. It creates
pointer-sized `_LOADLIB` userdata in `LOADLIB: path` registry entries before
opening the library. Failed opens leave null handles and retry on later calls;
failed symbol lookup keeps the opened handle and returns the `init` status.
C-root symbol misses contribute the reference's missing-module diagnostic.
Hyphen prefixes and dotted initializer names follow Lua 5.1's naming rules.

Registry getters/setters and `_LOADLIB` metatable lookup preserve their live
C stack slots while callbacks run. Library return hooks see the retained handle
and writable result suffix. The finalizer's metatable lookup also supports registry
metamethods. Error unwinding and instruction aborts discard pending loader jobs;
reentrant callbacks and child-coroutine loads share the same VM safely.

The userdata owns its library resource. Its finalizer closes the native handle
and zeros the pointer payload. Repeated closes are harmless; a later load can
reopen the same cached userdata. Removing registry entries permits collection,
and shutdown runs finalizers before releasing the adapter. Disabling future
opens keeps already opened handles available for symbol resolution. Private
function bindings can retain the owner object, while explicit close still closes
the library. Calling functions from a closed library remains unsupported.

The Windows development boundary now supplies a resolver backed by the actual
platform loader. It uses the reference's Windows message formatting and keeps
DLL owners through closures, hooks, reader/writer callbacks and thread-state
destruction. The C-only error-jump shim still returns every Rust dispatcher
before jumping. No C Lua implementation executes the Rust cases.

The [retained before comparison](2026-10-27-rust-lua-library-loading-before.json)
uses the source-built runner from the preceding package-stack milestone; its
digest can be checked against that report. The reference completes both new
module-loading modes, while the old Rust runner aborts with the unsupported
loader/invalid copied-body result behavior. This is a prebuilt baseline, not a
claim that modified source hashes built that runner.

The [current source-built comparison](2026-10-27-rust-lua-library-loading-boundary-current.json)
compiles one C module against official Lua 5.1.5 and the Rust boundary. It loads
the module through `package.loadlib` and C-root `require`, invokes its initializer
and functions, checks import caching, nil/false/error initializer results,
environment inheritance into newly created C closures, NUL prefixes, missing and
invalid DLLs, missing symbols, registry callbacks, hook writes, close/reopen and
collection. Both ordinary and copied builtin bodies run the same fixture. Earlier
native boundary and stack fixtures remain in the strict comparison. The
[evidence manifest](2026-10-27-rust-lua-library-loading-evidence.json) binds reports
to current source hashes and digests.

Both real-library modes match all 1,767 stdout bytes. Each opens and closes five
platform references and destroys all eight C states. Across both modes, all 915
Rust dispatch/drop guards balance under `panic=abort`. All twenty Rust runs in the
current boundary comparison have balanced dispatch/state counters, and all 678
copied-body stack observations match.

The [first expanded comparison](2026-10-27-rust-lua-library-loading-first.json)
retains a strict package-search mismatch outside the new DLL fixture. Its
`package.path` callback replaces the only stack root for the translated search
name and collects while Lua 5.1.5 `findfile` still holds a raw pointer into that
string. The reference produced `absent-.missing` in that run. The unchanged input
matched in three later [paired replays](2026-10-27-rust-lua-library-loading-capture-audit.json)
and in the current source-built comparison. The first failure and raw outputs
remain recorded; this does not prove stable parity for that input.

Six portable groups use a deterministic ABI provider to check caching and null
handle retries, symbol status, userdata payloads, C/C-root naming, function
environments, loader-frame identity, result aliases, resource cleanup, disabling
future opens, nested/child-thread loading and instruction-abort recovery. The
[platform validation](2026-10-27-rust-lua-library-loading-validation.json) records
Windows, Linux x64 and i686 SSE2 tests, strict Clippy, formatting and the
normal/build dependency tree with unchanged sources.
It passes 755 tests on Windows and 756 on each Linux target. The
[first platform validation](2026-10-27-rust-lua-library-loading-validation-first.json)
retains two GC test failures from fixed table-count bounds. Those tests now
require exact reclamation back to the measured builtin-table baseline, including
the standard `_LOADLIB` metatable; they pass without weakening the reclamation
requirement.

The [i686 SSE2 stock suite](2026-10-27-rust-lua-library-loading-i686-suite.json)
runs unchanged `main.lua` and `all.lua` through both actual CLIs, with verified
ELF32/chunk ABIs. It still omits T/testC and native suite libraries. The
[Windows probes](2026-10-27-rust-lua-library-loading-windows-suite.json) retain the
shared `files.lua:59` open-file rename failure in all four modes.
Both i686 stock entry points pass in both CLIs. Windows retains 17/18 independent
passes in each mode, and its strict runner still fails.

This establishes package integration with an installed ABI provider. The default
VM and production CLI still have no C ABI adapter; their unsupported-loader
behavior and the fourteen native-loading argument-audit failures remain open.
The development C shim is not yet a shipped standard `lua51` library, and the
Linux native adapter remains unfinished. Five lifecycle/allocator C declarations,
arbitrary foreign handle/payload mutation, complete inactive-state execution,
broader error-time/native resource/OOM semantics, parser-internal mutation and the
retained lowered-call-count yield mismatch remain open. The unstable captured-name
case also remains unproven. Full standalone Lua 5.1
parity remains the goal; performance work stays deferred.
