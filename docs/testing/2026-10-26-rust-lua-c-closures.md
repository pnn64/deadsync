# C closures with private owned bindings

The development C bridge previously stored two adapter pointers as Lua captures.
Its callbacks therefore reported two upvalues even when the corresponding C
function had none. That layout also could not represent arbitrary C closure
captures independently of the binding and its DLL lifetime.

`Vm::new_host_function_with_data` now owns private typed data separately from
the traced Lua captures. `HostCall::function_data` borrows that data by type.
Opaque data is inaccessible to Lua and does not count as an upvalue. Retained
Lua values must be in the captures; opaque data itself is not traced. Its
destructor must not reenter Lua or panic. Heap estimates include the boxed
object's size and exclude allocations it owns indirectly.

Data survives while its closure is reachable, including through capture
cycles, active calls and shutdown finalizers. Collection releases unreachable
closure cycles and their data. Rejected registration also releases the supplied
data. The existing registration path needs no private data allocation.

`NativeStack::push_closure` replaces its last captures with a new closure,
preserving their order and inheriting the active function's environment.
Native closures now support the Lua 5.1 C limit of 255 upvalues. The source
compiler's separate 60-upvalue limit is unchanged. Stack access resolves the
Lua 5.1 registry, environment, globals and upvalue pseudo-indexes. Replacement
uses the collector barriers; registry/environment/global replacements require
tables. Numeric type errors preserve negative argument indexes, including
upvalue pseudo-indexes and method adjustments.

The C fixture implements `lua_pushcclosure` through this core API. Each binding
privately owns its C function pointer and a shared DLL owner. Initial callbacks
have zero actual captures; returned counters have two, and a separate closure
uses all 255. The runner drops its external DLL owner before invoking callbacks
and verifies that the VM releases all binding and thread-state owners before
returning. It no longer reconstructs shared ownership from Lua light userdata.

The retained [reproduction](2026-10-26-rust-lua-c-closures-before.lua) passes
against unchanged official Lua 5.1.5 and fails the initial zero-upvalue assertion
with the prior Rust runner, `05951b79e`. Its [raw report](2026-10-26-rust-lua-c-closures-before.json)
records that runner as prebuilt and pins its binary to the previous milestone;
the source map describes the new fixture and audit inputs, not the old runner's
source revision. The final bridge runs the same fixture successfully.

| Check | Result |
| --- | --- |
| Rust tests | 587 pass: 73 unit, 510 integration, 4 documentation |
| New host-closure groups | 8 pass |
| Formatting and strict Clippy | Pass |
| Normal/build dependencies | Only deadlib-lua |
| Shared-source C module | Program stdout/status match official Lua |
| C dispatch cleanup | 2,442 operations, 2,442 returned Drop guards; panic=abort |
| C state lifetime | 69 created, 68 collected before shutdown, 69 destroyed |
| Cache identity checks | 16/16 |
| Process checks | 140/140 |
| Standalone CLI | 183/221 raw; 221/221 after one exact leading banner removal |
| Library argument matrix | 5,493/5,507; fourteen loadlib differences retained |
| Official files, C/Rust/cache store/cache hit | 17/18 in every mode; same Windows rename failure |

Eight Rust groups cover typed private data, capture cycles, shutdown finalizer
ordering, rejected ownership, 255 captures, pseudo-index environments, failed
stack closure creation and heap estimates. C cases additionally cover mutable
counter independence, collection, coroutine calls, inaccessible C upvalues from
Lua's debug library, environment inheritance, thread global replacement,
temporary shared registry replacement and exact negative-index diagnostic
bytes. The older unprotected call, error identity, hook, finalizer, native
stack mutation and persistent-state cases remain in the same fixture.

Raw [boundary](2026-10-26-rust-lua-c-closures-boundary.json),
[cache identity](2026-10-26-rust-lua-c-closures-identity.json),
[process](2026-10-26-rust-lua-c-closures-process.json),
[CLI](2026-10-26-rust-lua-c-closures-cli.json),
[library](2026-10-26-rust-lua-c-closures-library.json) and
[official-suite](2026-10-26-rust-lua-c-closures-suite.json) observations are
retained. Strict CLI and library audits still exit 1 for the documented banner
and native-loader differences. The official suite keeps all eighteen file
hashes, assertions, bootstrap and limits. Only files.lua:59 fails in each mode,
on Windows rename of an open input. Cache modes record eighteen stores and
eighteen hits. [Validation metadata](2026-10-26-rust-lua-c-closures-validation.json)
pins the final sources, artifacts, build inputs, logs and baseline reproduction.
A fresh Cargo target built the final Rust source revision. Separate retained
reference executables preserve the library-audit and shared-DLL identities.

Production engine changes use safe Rust and add no dependency or C shim.
The C bridge remains a development fixture for the currently executing thread.
Inactive/cross-thread C API access, complete stack/table/auxiliary APIs, C API
userdata/thread operations, production library caching/unload and actual
`package.loadlib`/`require` integration remain unfinished. Broader debug and
scratch-stack behavior, all.lua/main.lua and full standalone Lua 5.1 remain
open gates. Later dialects and gameplay integration stay deferred.
No performance benchmarks ran.
