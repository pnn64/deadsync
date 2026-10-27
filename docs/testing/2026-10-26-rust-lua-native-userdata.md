# Native userdata, environments and string conversion

`NativeStack` now allocates full userdata and lends its payload bytes, reads and
sets function/userdata/thread environments, measures raw object lengths and
converts numeric values to strings in their original stack slot or capture.
The development C bridge adds `lua_newuserdata`, `lua_objlen`, `lua_tolstring`,
`lua_getfenv` and `lua_setfenv`; `lua_touserdata` now accepts full userdata.
It remains a development fixture, with 32 fixed callbacks and 36 Lua API
declarations. Production native-module loading is still unfinished.

Userdata payloads are zero-initialized, aligned for pointers and doubles, and
stored in a stable allocation. The alignment reserve keeps zero-length payload
addresses inside distinct allocations. Allocation overflow or impossible
payload sizes return an error before changing the native stack. Payloads keep
their requested logical length; heap estimates include the alignment reserve.
This does not make every VM allocation failure recoverable.

Interned strings now include a trailing NUL outside their logical bytes. The
string wrapper exposes logical bytes to existing Lua operations, while
`to_c_bytes` lends the complete allocation including its terminator. Embedded
NUL bytes, string identity and logical lengths are preserved. Numbers convert
in the original slot or C upvalue with the existing write barriers. Other
types return no string. Storage remains stable while the object is rooted.

Environment setters consume their table even when the target type is
unsupported. Function, userdata and thread updates use the existing collector
barriers. New native userdata inherits its executing closure's environment.
Pending unprotected failures prevent later allocation, mutable payload access,
environment changes and string conversion from continuing the callback.

The same C module source is compiled against unmodified official Lua 5.1.5 and
the Rust development bridge on Windows/MSVC. It initializes and mutates a real
C struct, checks alignment and retained pointers through allocation/collection,
and exercises direct C finalizers, resurrection, environment identity, empty
userdata, binary strings, numeric conversions and mutable C upvalues. The
reference CLI and module share one official Lua DLL. No C Lua implementation
executes the Rust cases. C-only setjmp/longjmp still occurs after Rust dispatch
returns and releases its locals and borrows; release uses panic=abort.

| Check | Result |
| --- | --- |
| Rust tests | 605 pass: 73 unit, 528 integration, 4 documentation |
| New native-object groups | 8 pass |
| Formatting and strict Clippy | Pass |
| Normal/build dependencies | Only deadlib-lua |
| Shared-source C module | Raw stdout and status match official Lua |
| C dispatch cleanup | 32,080 operations and returned Drop guards; panic=abort |
| C state lifetime | 70 created, 69 collected before shutdown, 70 destroyed |
| Cache identity | 16/16 |
| Process checks | 140/140 |
| Standalone CLI | 183/221 raw; 221/221 after one exact leading banner removal |
| Library argument matrix | 5,493/5,507; fourteen loadlib differences retained |
| Official files, C/Rust/cache store/cache hit | 17/18 each; same Windows rename failure |

The eight Rust groups cover payload sizes 0 through 65, alignment and mutation
across nested collection, moving the VM, last-root release, impossible
allocation and full-stack rejection, binary terminators, numeric conversion,
raw lengths, environment cycles and stack effects, and pending-error guards.
The existing tests and C table/call/error/closure cases remain enabled.

Raw [boundary](2026-10-26-rust-lua-native-userdata-boundary.json),
[cache identity](2026-10-26-rust-lua-native-userdata-identity.json),
[process](2026-10-26-rust-lua-native-userdata-process.json),
[CLI](2026-10-26-rust-lua-native-userdata-cli.json),
[library](2026-10-26-rust-lua-native-userdata-library.json) and
[official-suite](2026-10-26-rust-lua-native-userdata-suite.json) observations are
retained. [Validation metadata](2026-10-26-rust-lua-native-userdata-validation.json)
pins sources, artifacts, workspace inputs and logs. A fresh Cargo target built
the final source revision. The eighteen official file hashes, assertions,
bootstrap, instruction limit and timeout remain unchanged; cache modes record
eighteen stores and eighteen hits.

The strict CLI, library and official-file audits retain their nonzero exits.
The CLI differences are exact version banners; the library differences are
fourteen unsupported native-loader cases. Official `files.lua:59` also fails
in the C reference on Windows rename of an open input. Full `all.lua`/`main.lua`
passage, complete C and auxiliary APIs, inactive/cross-thread C access, broader
native GC/debug/resource behavior, production library lifetime management and
actual `package.loadlib`/`require` integration remain open. Full standalone Lua
5.1 parity is unproven. Later dialects and gameplay integration stay deferred.
Production engine changes use safe Rust and add no dependencies. No performance
tuning or benchmarks accompany this milestone.
