# Lua 5.1 C-owned default states

After engine bootstrap, `luaL_newstate` creates an independent Rust VM owned by
the C state group. Its main state and children retain that VM and their C image
until `lua_close`. C API operations can run between native entries: an idle
owned group borrows its VM for one operation, while nested callbacks retain
their existing context. The default creation path uses Rust's allocator.
Custom allocator APIs are not implemented or simulated.

`NativeCAbiLoader::program` resolves a retained `int(void)` C program and
initializes its C image's engine factory. `NativeCProgram::run` invokes that
program without a Rust-facing VM or synthetic Lua/C activation. The program
stays on its creating thread. Its unsafe constructor requires the declared C
signature, valid C API use, live callback code and no jump across the Rust caller.

The factory retains its own C image by a live address, avoiding an executable
path assumption. Every owned VM has a separate state map, globals, registry and
heap. Its native loader uses that same image. Existing borrowed Rust states keep
their earlier lifetime contract and still require a live entry.

`lua_close` accepts a main or child state and consumes the main group's sole
owner. VM destruction resets the main stack and runs remaining finalizers,
continuing after errors. Active same-group callbacks and concurrent/reentrant
root borrows are rejected before freeing or mutably borrowing VM storage.
General reentry between independently executing VMs remains an open gate.

Unprotected root errors leave their Lua error value and persistent error status.
Rust unwinds before C invokes the panic callback. A C `setjmp`/`longjmp` recovery
inside the embedding program therefore skips no Rust frames. Protected calls can
still run afterwards without clearing the earlier `lua_status`. Default and
returning panic callbacks terminate through C with status 1 and flush streams.

The public-header fixture creates two independent states, retains them after the
first C program returns, and uses them again in the second. It covers empty
initial globals, stack and registry roots, an independent state's values across
collection, parser readers/writers, bytecode reload, raw builtin bodies,
protected calls, recursion limits, hooks and coroutine yield/resume. A callback
uses the other idle VM and another callback closes it. Closing the main through
its child runs three finalizers in reverse order, including an error in the first
one. A finalizer collects a weak value held only by the main stack before close.
Panic recovery checks status, error values and escaped upvalues.

The [preserved pre-reset development build](2026-10-29-rust-lua-native-owned-close-before.json)
retains that weak value in both C image configurations, differing on one row in
each transcript. Close now closes main-thread cells, clears its stack roots and
resets its C-call bookkeeping before finalizers. The baseline binds archived
sources and binaries; only the two runtime files implementing that reset differ.

All 89 observations match release and assertion-enabled Lua 5.1.5 on
[Windows debug](2026-10-29-rust-lua-native-owned-debug.json),
[Windows release](2026-10-29-rust-lua-native-owned-release.json) and
[Linux i686 SSE2 release](2026-10-29-rust-lua-native-owned-i686.json). Each profile
tests both the public SDK with a separately built C program and an explicit image
containing its own shim: 267 observations across four configurations, with 534
Rust transcript rows. The two panic termination probes also match both
references through both C images on all three profiles, covering 12 expected
Rust process terminations. Normal runs balance every operation, five C states,
four groups and their library lifetimes.

The [i686 verifier](2026-10-29-rust-lua-native-owned-i686-verification.json)
checks six ELF32 binaries, SSE2 reference settings, enabled assertions, source
and artifact hashes, and the native-enabled stock suite.
[Full validation](2026-10-29-rust-lua-native-owned-validation.json) passes 831
Windows tests and 832 tests on each Linux target, strict Clippy, formatting and
all-target checks without default features. Normal/build dependencies remain
empty. A normal integration test checks both image configurations against the
golden transcript. The separate
[portable run](2026-10-29-rust-lua-native-owned-portable.json) passes 35 tests.

The [library audit](2026-10-29-rust-lua-native-owned-stdlib.json) matches 5,507
cases. The [native audit](2026-10-29-rust-lua-native-owned-boundary-current.json)
retains balanced operation/state/library lifetimes and expected termination
probes. Both native-enabled i686 CLIs pass `main.lua` and `all.lua` in the
[stock suite](2026-10-29-rust-lua-native-owned-i686-suite.json), with T/testC
absent. [Windows](2026-10-29-rust-lua-native-owned-windows-suite.json) retains
17/18 independent passes in all four modes, including the common `files.lua:59`
failure. The [evidence manifest](2026-10-29-rust-lua-native-owned-evidence.json)
binds the final sources, reports, artifacts and logs and embeds the proof tools.

Core declaration coverage is 77/80. `lua_newstate`, `lua_getallocf` and
`lua_setallocf` remain absent. Loading the C shim alone does not supply a Rust
engine; a standalone C host artifact remains open. Custom allocator ownership,
OOM behavior, exact heap/GC accounting, arbitrary independent-VM reentry and
broader continuation/resource behavior remain unproven. Full Lua 5.1 parity is
not established.
