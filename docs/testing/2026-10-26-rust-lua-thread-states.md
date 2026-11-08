# Persistent host state owned by Lua threads

`HostThreadState` gives each Lua thread independent, typed host resources.
`HostCall::thread_id` identifies the active thread, including the main one.
Initialization fills one slot per concrete type; duplicate initialization fails
without replacing the existing resource. New coroutines do not inherit state.

State may retain Lua values through an immutable roots slice. Initialization
validates every root before publishing state and applies the incremental GC
barrier even when the thread has already been marked. Tracing follows these
edges from live threads. An unreachable thread/state self-cycle remains
collectible. State allocation bytes and the resource vector enter heap
accounting. Host destructors must not reenter Lua or panic.

Resources stay attached to their thread arena entry while execution stacks
switch between threads. Suspension preserves them, and collection drops them.
Main-thread resources remain alive through Lua shutdown finalizers and drop
afterward. Five integration groups exercise identity, separate type slots,
suspended roots and self-cycles, shutdown order and rejected roots. A unit test
checks root installation after the thread has already been marked.

The development C boundary now allocates a persistent `lua_State` per Lua
thread. Each invocation temporarily installs its own dispatch context and C
error boundary, restoring the previous context after ordinary return or error.
The same pointer survives nested calls and a move of the Rust VM. Independent
coroutines receive distinct pointers, retain them through yield/resume and
release them when collected. A C callback caches its main-thread pointer and
uses it on later active invocations. DLL owners remain alive until their C
states have been freed.

| Check | Result |
| --- | --- |
| Rust tests | 572 pass: 73 unit, 495 integration, 4 documentation |
| Formatting and strict Clippy | Pass |
| Normal/build dependencies | Only deadlib-lua |
| Shared-source C module | Program stdout/status match official Lua |
| C dispatch cleanup | 2,071 operations, 2,071 returned Drop guards; panic=abort |
| C state lifetime | 68 created, 67 collected before shutdown, 68 destroyed |
| Cache identity checks | 16/16 |
| Process checks | 140/140 |
| Standalone CLI | 183/221 raw; 221/221 after one exact leading banner removal |
| Library argument matrix | 5,493/5,507; fourteen loadlib differences retained |
| Official files, C/Rust/cache store/cache hit | 17/18 in every mode; same Windows rename failure |

The reference executable and module share the same official Lua DLL, built
from unchanged, checksum-pinned Lua 5.1.5 sources. The same C module source
also runs through the Rust development boundary. Rust dispatch returns and
drops its locals before the C shim performs an error jump. No Rust frame is
skipped by that jump; the release build retains `panic=abort`.

Raw [boundary](2026-10-26-rust-lua-thread-states-boundary.json),
[cache identity](2026-10-26-rust-lua-thread-states-identity.json),
[process](2026-10-26-rust-lua-thread-states-process.json),
[CLI](2026-10-26-rust-lua-thread-states-cli.json),
[library](2026-10-26-rust-lua-thread-states-library.json) and
[official-suite](2026-10-26-rust-lua-thread-states-suite.json) reports retain
their observations. The strict CLI audit exits 1 for 38 version banners, and
the library audit exits 1 for fourteen unsupported native-loader results.
Those differences remain failures. All eighteen official file hashes,
assertions, bootstrap and limits are unchanged. Only files.lua:59 fails in
each mode, on Windows rename of an open input. Cache modes record eighteen
stores and eighteen hits.

[Validation metadata](2026-10-26-rust-lua-thread-states-validation.json) pins
source maps, native artifacts, isolated workspace inputs and check logs. A
fresh Cargo target built the final Rust source revision. Separate retained
reference executables preserve the library-audit and shared-DLL identities.

The engine changes use safe Rust and add no production dependency or C shim.
The C bridge remains a development fixture. Its active-thread check rejects
inactive/cross-thread API access, which still requires implementation.
Unprotected `lua_call` fault stacks, pseudo-index and full auxiliary APIs,
C closure/userdata/thread operations, production library caching/unload and
actual `package.loadlib`/`require` integration remain unfinished. Broader debug
and scratch-stack behavior, all.lua/main.lua and full standalone Lua 5.1
remain open gates. Later dialects and gameplay integration stay deferred.
No performance benchmarks ran.
