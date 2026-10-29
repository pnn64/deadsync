# Fallible Lua object storage and allocator rejection

Object arena insertion now reserves values, generations, free slots and mark
buffers before publishing an identity. Failure leaves live objects, generations
and marks intact. Earlier reservations can retain additional capacity; that real
storage is included in allocation accounting. Every slot has free-list space,
so removing, sweeping and reusing arena slots need no allocation.

`Vm::try_new_userdata` and `Vm::try_new_table` expose fallible constructors.
Userdata creation commits its order and payload accounting after insertion
succeeds. Both constructors reserve collector queue space before inserting a
new object during marking. Host roots use fallible arena insertion as well.
Native userdata/table construction preflights its destination stack on the main
or a selected thread. Stack growth now distinguishes allocation failure from the
8,000-slot limit; `check_stack` continues to return false for either failure.
The Lua `NewTable` instruction uses the fallible constructor. Memory failures
carry the existing fixed Lua diagnostic and status 4 through protected calls.
Existing infallible Rust convenience constructors remain available.

The test-only allocator delegates accepted requests to `System`, returns NULL
for actual alloc/realloc requests starting at a chosen failure point, and always
allows deallocation. Rejection persists until the probed operation returns.
The allocator does not panic or jump. Each test measures a successful control,
rejects each measured allocation in turn, checks the error and unmodified
logical state, then retries with allocation restored. Coverage includes payloads,
all four userdata arena buffers, table arena growth, host roots, native table
buffers, main/selected stack growth and selected-thread constructors. A native
callback returns memory status 4 and bypasses an invalid error handler after a
real allocation failure. Collector tests cover a full work queue during marking.
A separate test rejects all allocations while removing, reusing and sweeping
129 arena objects, and observes no allocation attempts.

The [optimized fault runs](2026-10-29-rust-lua-fallible-storage-faults.json) pass
47 rejection points on each of Windows x64, Linux x64 and Linux i686: 141 actual
failure cases. The same allocation tests also run in the ordinary debug suites.
This test allocator is absent from production builds and adds no dependency.

Native table creation now also matches PUC's `table overflow` runtime error when
a hash hint exceeds 2^26 records. The guard runs before Rust's hash-capacity
calculation, including on i686 where multiplication of `INT_MAX` would overflow.
Public-header C probes cover the first excessive size, a larger power of two,
`INT_MAX`, handler invocation, parser readers, coroutine failure, selected-child
panic and a successful subsequent table write.

All 21 table-limit observations match release and assertion-enabled Lua 5.1.5 on
[Windows debug](2026-10-29-rust-lua-fallible-storage-debug.json),
[Windows release](2026-10-29-rust-lua-fallible-storage-release.json) and
[Linux i686 SSE2 release](2026-10-29-rust-lua-fallible-storage-i686.json).
Together with the existing 97 C-host and 46 allocation observations, these runs
compare 492 observations across the three profiles and retain 18 expected Rust
panic exits. Public forwarding entries and unchanged `lua.c` execution still
pass. All six i686 stock C-CLI suite runs pass with official native modules and
without T/testC. The
[i686 verifier](2026-10-29-rust-lua-fallible-storage-i686-verification.json)
checks sources, artifacts, ELF32 binaries, module flags and assertions.

[Full validation](2026-10-29-rust-lua-fallible-storage-validation.json) passes
845 Windows tests and 846 on each Linux target, strict Clippy, formatting and
all-target checks without default features. The
[library audit](2026-10-29-rust-lua-fallible-storage-stdlib.json) matches all 5,507
cases. The [native audit](2026-10-29-rust-lua-fallible-storage-boundary-current.json)
retains 42 normal and 25 expected termination probes with balanced lifetimes.
The [evidence manifest](2026-10-29-rust-lua-fallible-storage-evidence.json)
verifies current source, artifacts, fault logs and reports, and retains its proof
tools.

This establishes recoverable reservation failure in the named paths. It does
not make the whole interpreter recover from persistent memory exhaustion.
Compiler/string storage, table mutation, closures and other collector/control
buffers still contain infallible allocations. Production storage still uses
Rust's allocator. Exact heap/GC accounting, broader resource/continuation parity
and custom allocator ownership remain open. Core API coverage stays 77/80;
`lua_newstate`, `lua_getallocf` and `lua_setallocf` remain absent. Full Lua 5.1
parity is not complete.
