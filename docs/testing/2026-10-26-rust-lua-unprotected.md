# Unprotected native calls retain the Lua fault stack

The development C bridge previously implemented `lua_call` through the Rust
host API's protected call. A failed child call closed its open upvalues and
removed its frames before an outer `xpcall` handler could inspect them.

`NativeStack::call_unprotected` now preserves those frames, registers, locals,
open upvalues and active hook state while returning the error through Rust and
the C boundary. The outer Lua handler runs before normal protected recovery
removes the fault stack. The C shim still jumps only after Rust dispatch has
returned and released its locals; no Rust frame is skipped by a C jump.

An unprotected failure remains pending until the callback exits. Later stack
mutations, result selection and writes through `HostCall::push` reject it,
and returning `Ok` from Rust cannot swallow it. Resource failures still clean
up active child coroutines before propagating. `NativeStack::call` and ordinary
Rust host calls retain their existing protected recovery behavior.

Protected loaders retain the enclosing error function across an unprotected
host call. This matters when a `load` reader fails: the surrounding `xpcall`
handler transforms the error even though `load` then returns nil and the
transformed value. An intervening protected call stops inheritance. The handler
search can cross the Rust driver's unwind floor without unwinding a live Rust
callback; loader recovery only removes frames above that floor.

The retained [reproduction](2026-10-26-rust-lua-unprotected-before.lua) passes
against unchanged official Lua 5.1.5 and fails with the previous milestone's
Rust runner, `810714adf`. Its [raw report](2026-10-26-rust-lua-unprotected-before.json)
pins that prebuilt runner and both sets of native artifacts. The reproduction
contains the initial fault-local and reader-handler cases; the final fixture
adds further cases.

Seven new Rust test groups cover fault-local inspection and mutation, open
upvalues across collection, errors crossing multiple native callbacks, ignored
errors, protected versus inherited reader handlers, fixed/open result counts,
callable objects, resource abort recovery and dead-coroutine inspection. The
shared-source C fixture additionally covers hook and finalizer failures and
compares child arithmetic, native argument and non-callable error bytes.

| Check | Result |
| --- | --- |
| Rust tests | 579 pass: 73 unit, 502 integration, 4 documentation |
| New unprotected-call groups | 7 pass |
| Formatting and strict Clippy | Pass |
| Normal/build dependencies | Only deadlib-lua |
| Shared-source C module | Program stdout/status match official Lua |
| C dispatch cleanup | 2,109 operations, 2,109 returned Drop guards; panic=abort |
| C state lifetime | 68 created, 67 collected before shutdown, 68 destroyed |
| Cache identity checks | 16/16 |
| Process checks | 140/140 |
| Standalone CLI | 183/221 raw; 221/221 after one exact leading banner removal |
| Library argument matrix | 5,493/5,507; fourteen loadlib differences retained |
| Official files, C/Rust/cache store/cache hit | 17/18 in every mode; same Windows rename failure |

Raw [boundary](2026-10-26-rust-lua-unprotected-boundary.json),
[cache identity](2026-10-26-rust-lua-unprotected-identity.json),
[process](2026-10-26-rust-lua-unprotected-process.json),
[CLI](2026-10-26-rust-lua-unprotected-cli.json),
[library](2026-10-26-rust-lua-unprotected-library.json) and
[official-suite](2026-10-26-rust-lua-unprotected-suite.json) observations remain
available. The strict CLI audit exits 1 for 38 version banners, and the library
audit exits 1 for fourteen unsupported native-loader results. These differences
remain failures. All eighteen official file hashes, assertions, bootstrap and
limits are unchanged. Only files.lua:59 fails in each mode, on Windows rename
of an open input. Cache modes record eighteen stores and eighteen hits.

[Validation metadata](2026-10-26-rust-lua-unprotected-validation.json) pins the
final source maps, artifacts, workspace inputs, check logs and baseline
reproduction. The final Rust source revision uses a fresh Cargo target.
Separate retained reference executables preserve the library-audit and
shared-DLL identities. The reference CLI and C module use one official Lua DLL.

The production engine changes use safe Rust and add no dependency or C shim.
The C bridge remains a development fixture limited to its active thread.
Inactive/cross-thread C API access, pseudo-index and complete auxiliary APIs,
C closure/userdata/thread operations, production library caching/unload and
actual `package.loadlib`/`require` integration remain unfinished. Broader debug
and scratch-stack behavior, all.lua/main.lua and full standalone Lua 5.1 remain
open gates. Later dialects and gameplay integration stay deferred.
No performance benchmarks ran.
