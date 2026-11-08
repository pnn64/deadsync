# Protected native calls and error-function status

`NativeStack::protected_call` now provides Lua 5.1 protected execution with an
optional error-function index. Its `NativeCallStatus` distinguishes success
(0), runtime failure (2) and error-handler failure (5). Successful calls replace
their function/arguments with the requested results; failure always leaves one
error value. Stack prefixes survive, missing results become nil, and runtime
failures leave the native callback usable. Instruction/resource limits still
abort the outer Rust entry, even if the callback ignores the returned error.

A hidden continuation roots the selected error function before entering the
child. It reuses the existing VM handler path, keeping failing frames and open
locals available until handling finishes. Status occupies a private register
slot and is returned separately from Lua values. A handler can return the text
`error in error handling` successfully while the protected call reports runtime
failure (2); an actual handler failure reports status 5. Status is never inferred
from diagnostic bytes. Error functions retain stack-slot references, so changes
through debug inspection or parameter mutation affect later handling attempts.
References follow fixed-parameter and native argument-window moves; vararg entry
clears the original fixed-parameter slots. Pseudo-indexes are rejected, as the
Lua 5.1 manual requires for this API. The continuation adds no visible Lua
wrapper frame or extra native-call depth.

The development C bridge adds `lua_pcall` and `lua_cpcall`. The latter creates a
zero-upvalue C closure, calls it with one light-userdata argument, discards its
results and retains its error when needed. C-only error jumps remain inside
the shim's active invocation; every Rust dispatch returns and releases its
locals before C jumps. Production C API and native-module loading remain open.

The same C module source is compiled against unmodified official Lua 5.1.5 and
the Rust development bridge on Windows/MSVC. It checks positive and negative
error-function indexes, result counts, arbitrary error identity, handlers that
inspect/mutate open Lua locals, collection, recursive handler failures, reader
error-function inheritance, intervening protection, yield errors, binary C
errors and `lua_cpcall` stack effects. Both engines reach 220 handler attempts
before returning status 5 and the same diagnostic. The reference CLI and module
share one official Lua DLL; no C Lua implementation executes the Rust cases.

| Check | Result |
| --- | --- |
| Rust tests | 614 pass: 73 unit, 537 integration, 4 documentation |
| New native-protected groups | 9 pass |
| Formatting and strict Clippy | Pass |
| Normal/build dependencies | Only deadlib-lua |
| Shared-source C module | Raw stdout and status match official Lua |
| C dispatch cleanup | 32,377 operations and returned Drop guards; panic=abort |
| C state lifetime | 72 created, 71 collected before shutdown, 72 destroyed |
| Cache identity | 16/16 |
| Process checks | 140/140 |
| Standalone CLI | 183/221 raw; 221/221 after one exact leading banner removal |
| Library argument matrix | 5,493/5,507; fourteen native-loader failures retained |
| Official files, C/Rust/cache store/cache hit | 17/18 each; same Windows rename failure |

The nine new Rust groups cover result counts and callable values; fault-local
inspection and mutation; status independent of error bytes; nested calls,
reader failures and coroutines; recursive handler recovery; fatal instruction
limits; invalid stack/count inputs; handler-overflow recovery; and mutable error-function slot references. Existing
native table, userdata, unprotected call, closure, state and error-boundary
checks remain enabled. The C fixture now exposes 37 fixed callbacks and 38 Lua
API declarations.
The pinned official header declares 80 core functions; this shim declares 37
of those plus `luaL_checknumber`. This inventory measures declarations, rather
than semantic completeness or production availability. Validation records the
43 missing core names.

Raw [boundary](2026-10-27-rust-lua-native-protected-boundary.json),
[cache identity](2026-10-27-rust-lua-native-protected-identity.json),
[process](2026-10-27-rust-lua-native-protected-process.json),
[CLI](2026-10-27-rust-lua-native-protected-cli.json),
[library](2026-10-27-rust-lua-native-protected-library.json) and
[official-suite](2026-10-27-rust-lua-native-protected-suite.json) observations are
retained. [Validation metadata](2026-10-27-rust-lua-native-protected-validation.json)
pins sources, artifacts, workspace inputs and logs. A fresh Cargo target built
the final revision. Official file hashes, assertions, bootstrap, instruction
limit and timeout remain unchanged; cache modes record eighteen stores and
eighteen hits.

The strict CLI, library and official-file audits retain their nonzero exits.
CLI differences are exact version banners; fourteen library cases still fail
because native loading is unsupported. Official `files.lua:59` also fails in
the C reference on Windows rename of an open input. Full `all.lua`/`main.lua`
passage, the remaining C and auxiliary APIs, inactive/cross-thread C access,
general allocation-failure recovery, broader native GC/debug/resource behavior
and production native-module loading remain open. Full standalone Lua 5.1
parity is unproven. Production changes use safe Rust and add no dependencies.
No performance tuning or benchmarks accompany this milestone.
