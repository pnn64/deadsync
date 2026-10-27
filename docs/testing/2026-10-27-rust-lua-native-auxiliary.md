# Native formatted messages and auxiliary APIs

The development C boundary now supports `lua_pushfstring`,
`lua_pushvfstring` and 22 additional auxiliary functions for argument checks,
diagnostics, userdata tags, metafields, registration and reference handles.
`luaL_checknumber` now follows the same conversion/type-error path as the new
helpers. This extends compatibility without enabling production module loading.

`NativeStack::push_where` uses existing visible-frame/debug metadata for source
prefixes. Level zero, C frames, tail placeholders, absent frames and unknown
lines produce an empty prefix. `argument_error` shares caller naming and
method/self adjustment with existing VM errors. C varargs stay in the fixture's
C shim; strings, number coercion, tables, calls and errors use the Rust VM.

The formatter accepts Lua 5.1's limited `%s`, `%d`, `%f`, `%c`, `%p` and `%%`
conversions. Unknown conversions remain literal. Embedded NUL and null-pointer
string behavior, binary characters, signed zero, C pointer formatting and
rooted string-pointer stability are compared with official Lua. Primitive
format concatenation has no scheduler or collector check: an extra collection
point would observably change finalizer timing. This is a compatibility
requirement. A finalizer probe checks that formatting leaves pending finalizers
unrun. A terminal `%` is outside the reference-tested input contract.

Auxiliary checks cover string coercion in the original argument slot,
nonmutating numeric/integer conversion, defaults, options, stack capacity and
type errors. Diagnostic cases include missing arguments, methods, NUL in a
field name and negative indexes after temporary pushes. Error handlers inspect
the live failing C frame: the adapter excludes the top error slot, matching the
reference's three visible locals in the retained regression.

Tagged userdata uses registry metatables and stable aligned payloads. Metafield
lookup is raw and distinguishes false from nil; calls accept callable objects
through the shared scheduler. Registration shares `_LOADED` and dotted globals,
inherits the registering C function's environment, and creates real traced C
captures. Cases check loaded-table reuse, module-name collisions, capture
collection and empty dotted path components. Reference handles check nil,
free-list reuse, retained objects and collection after release.

| Check | Result |
| --- | --- |
| Rust tests | 627 pass: 73 unit, 550 integration, 4 documentation |
| New auxiliary Rust groups | 4 pass |
| Formatting and strict Clippy | Pass |
| Normal/build dependencies | Only deadlib-lua |
| Shared-source C module | Raw stdout and status match official Lua 5.1.5 |
| C dispatch cleanup | 36,766 operations and returned Drop guards; panic=abort |
| C state lifetime | 73 created, 72 collected before shutdown, 73 destroyed |
| Cache identity | 16/16 |
| Process checks | 140/140 |
| Standalone CLI | 183/221 raw; 221/221 after one exact leading banner removal |
| Library argument matrix | 5,493/5,507; fourteen native-loader failures retained |
| Official files, C/Rust/cache store/cache hit | 17/18 each; same Windows rename failure |

The identical C module source is built against unmodified official Lua 5.1.5
and the Rust development boundary on Windows/MSVC x64. All previous C cases
remain enabled. Four new Rust groups check source prefixes, argument errors,
primitive formatting and full-stack/pending-error guards. The fixture exposes
56 fixed callbacks and declares 75 APIs: 52 of the pinned header's 80 core
functions and 23 of its 36 auxiliary functions. The auxiliary inventory
normalizes `luaI_openlib` to the reference's enabled `luaL_openlib` alias.
Declarations do not establish complete semantics or production availability.

Raw [boundary](2026-10-27-rust-lua-native-auxiliary-boundary.json),
[cache identity](2026-10-27-rust-lua-native-auxiliary-identity.json),
[process](2026-10-27-rust-lua-native-auxiliary-process.json),
[CLI](2026-10-27-rust-lua-native-auxiliary-cli.json),
[library](2026-10-27-rust-lua-native-auxiliary-library.json) and
[official-suite](2026-10-27-rust-lua-native-auxiliary-suite.json) observations
are retained. [Validation metadata](2026-10-27-rust-lua-native-auxiliary-validation.json)
pins sources, artifacts, workspace inputs and logs. A fresh Cargo target builds
the final source revision. Official file hashes, assertions, bootstrap,
instruction limit and timeout remain unchanged; cache modes record eighteen
stores and eighteen hits.

Strict CLI, library and official-file audits retain their nonzero exits. CLI
differences are exact version banners; fourteen library cases fail because
production native loading is unsupported. Official `files.lua:59` fails in all
four modes on Windows rename of an open input. Full `all.lua`/`main.lua`
passage, remaining C/auxiliary APIs, inactive/cross-thread C access, general
allocation-failure recovery and broader native GC/debug/resource behavior
remain open. Full standalone Lua 5.1 parity is unproven. Later dialects and
integrations remain deferred. No performance tuning or benchmarks accompany
this milestone.
