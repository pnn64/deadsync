# Lua 5.1 allocation-error propagation

Fallible userdata payload allocation now produces `ErrorKind::Memory` and Lua
status `LUA_ERRMEM` (4). Protected calls bypass their error function for this
failure, including invalid handlers and allocation failure inside an error
handler. Parser readers and failed coroutines retain status 4. Unprotected root
and selected-thread panic callbacks observe that same status and error value.
Raising the ordinary string `not enough memory` still produces a runtime error
and invokes its handler.

The error carries static Rust text and a Lua string retained as a permanent VM
root. Cloning that memory error does not copy its text into a new allocation.
The userdata allocation failure happens before its creation order or arena entry
changes. A regression test collects repeatedly, checks the retained diagnostic
and unchanged userdata state, then allocates usable zeroed storage.

PUC checks userdata header overflow before calling its allocator. Requests beyond
that guard now raise `memory allocation error: block too big` with runtime status
2. The C fixture reproduces the default header layout from public-header types
and probes the last allowed size and first rejected size. Both sides match on
Windows x64 and Linux i686, including their different alignment rules.

The public-header C host has a separate allocation mode with 46 observations.
It covers protected and unprotected calls, nested protection, `lua_cpcall`, parser
readers, handler bypass, coroutine failure and repeat resume, and recovery after
the error. Its Lua assertions also exercise inherited reader handlers, an error
on a waiting resumer, collector finalizers, closed upvalues and a subsequent full
collection. Three separate probes check default, returning and selected-child
panic termination.

The [baseline](2026-10-29-rust-lua-memory-errors-baseline.json) binds the previous
milestone's shared libraries and the same C fixture. Both PUC builds pass. The
old Rust library reports runtime status 2, invokes handlers, converts an invalid
handler to status 5, and fails the fixture's Lua assertions with exit 87. Its
panic callbacks also report status 2. Those differences are retained as evidence.

All 46 observations and panic results now match release and assertion-enabled
Lua 5.1.5 on [Windows debug](2026-10-29-rust-lua-memory-errors-debug.json),
[Windows release](2026-10-29-rust-lua-memory-errors-release.json) and
[Linux i686 SSE2 release](2026-10-29-rust-lua-memory-errors-i686.json): 138
allocation observations and nine expected Rust memory-panic terminations. The
same runs retain all 97 existing C-host observations per profile, both existing
panic probes, public forwarding checks and execution through unchanged `lua.c`.
All six i686 stock C-CLI runs pass (`main.lua` and `all.lua` on three engines),
with the four official native libraries and without T/testC. The
[i686 verifier](2026-10-29-rust-lua-memory-errors-i686-verification.json) checks
source and payload hashes, ELF32 binaries, assertions and module builds.

[Full validation](2026-10-29-rust-lua-memory-errors-validation.json) passes 833
Windows tests and 834 on each Linux target, strict Clippy, formatting and
all-target checks without default features. The
[library audit](2026-10-29-rust-lua-memory-errors-stdlib.json) matches all 5,507
cases. The [native audit](2026-10-29-rust-lua-memory-errors-boundary-current.json)
passes 42 normal and 25 expected termination probes, balancing 1,229,274
dispatches, 19,219 states and 52 native libraries. The
[evidence manifest](2026-10-29-rust-lua-memory-errors-evidence.json) verifies the
current source, artifacts, logs, baseline and reports, and retains its proof tools.

This milestone covers the existing fallible userdata payload path. The fixtures
use enormous requests that cannot be satisfied; they do not induce process-wide
memory exhaustion or inject allocator failures. Arena metadata, other VM
collections and many standard-library buffers still allocate infallibly through
Rust. General recoverable OOM, custom allocator ownership, exact allocation/GC
accounting and broader resource/continuation parity remain open. Core API
coverage remains 77/80: `lua_newstate`, `lua_getallocf` and `lua_setallocf` are
absent. Full Lua 5.1 parity is not complete.
