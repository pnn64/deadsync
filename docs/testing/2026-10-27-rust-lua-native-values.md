# Native scalar conversions, comparison and concatenation

`NativeStack` now reads numbers, pointer-width integers and booleans without
mutating their source slots, pushes integers as Lua binary64 values, compares
indexed values with equality/less-than, and concatenates the last stack values.
Comparisons and concatenation share the existing VM metamethod and child-call
scheduler. Failures retain faulting Lua frames for outer error handlers; quotas
and native yield restrictions remain in force.

The development C bridge adds thirteen functions: `lua_isnumber`,
`lua_isstring`, `lua_isuserdata`, `lua_iscfunction`, `lua_tonumber`,
`lua_tointeger`, `lua_toboolean`, `lua_pushinteger`, `lua_pushstring`,
`lua_typename`, `lua_equal`, `lua_lessthan` and `lua_concat`. A pointer-width
integer field in the private fixture protocol preserves all input/output bits
before conversion to or from Lua numbers. The engine adds no dependency or
production C shim.

The same C module source is compiled against unmodified official Lua 5.1.5 and
the Rust development bridge on Windows/MSVC x64. Scalar records compare finite
number bits, signed zero, integer bytes, numeric-string acceptance, type
preservation, truth conversion and native-function classification. Cases
include embedded NUL bytes, invalid tails, hex input, overflow, infinities,
NaNs, pointer-width limits, userdata, null light userdata and C captures.
NaN payload bytes are excluded: the fixture emits a canonical label for that
field while checking classification and integer conversion. Ten C integer
samples include values beyond binary64's exact integer range.

Native concatenation uses the original operand slots. Coercions and partial
results are visible, but Lua 5.1 keeps the original C stack top and operand
roots until the entire operation succeeds. The fixture checks that a reduced
right operand survives collection during a later callback and is released
after completion. Zero operands push an empty string; one operand stays
unchanged, including numbers and objects. Metamethod results can be numbers or
objects. Concatenation order, binary strings, prefixes, errors and native yield
barriers are exercised.

Equality and ordering use shared metamethod rules and normalize results to
booleans. Absent indexes compare false; unlike numeric conversion, ordering
does not coerce numeric strings. Comparison operands remain in the visible C
window. Private comparison snapshots are omitted, and fixed metamethod
arguments stop owning duplicate roots once Lua receives them. This lets a
callback release objects by clearing its parameters and the original C slots.
Vararg extras remain rooted in their vararg area. Common table/userdata
handlers, callable handlers, fault-local mutation, collection and recursion
are checked. Comparison and concatenation both reach 196 recursive calls in
matching standalone contexts, as in the reference.

| Check | Result |
| --- | --- |
| Rust tests | 623 pass: 73 unit, 546 integration, 4 documentation |
| New native-value groups | 9 pass |
| Formatting and strict Clippy | Pass |
| Normal/build dependencies | Only deadlib-lua |
| Shared-source C module | Raw stdout and status match official Lua |
| C dispatch cleanup | 35,514 operations and returned Drop guards; panic=abort |
| C state lifetime | 73 created, 72 collected before shutdown, 73 destroyed |
| Cache identity | 16/16 |
| Process checks | 140/140 |
| Standalone CLI | 183/221 raw; 221/221 after one exact leading banner removal |
| Library argument matrix | 5,493/5,507; fourteen native-loader failures retained |
| Official files, C/Rust/cache store/cache hit | 17/18 each; same Windows rename failure |

The nine new Rust groups cover conversion and integer pushes; common-handler
comparison; cleared comparison roots; zero/one/binary/numeric concatenation;
original-slot coercions and stack visibility; fault frames and open locals;
operand-root lifetime; coroutine scheduling/yield rejection; and bad-count
checks with fatal quota recovery. Existing native API cases remain enabled.
The C fixture exposes 44 fixed callbacks and 51 Lua API declarations: 50 of
the pinned official header's 80 core functions, plus `luaL_checknumber`.
Declarations are an inventory, rather than proof of complete semantics or
production availability. Validation lists the 30 missing core functions.

Raw [boundary](2026-10-27-rust-lua-native-values-boundary.json),
[cache identity](2026-10-27-rust-lua-native-values-identity.json),
[process](2026-10-27-rust-lua-native-values-process.json),
[CLI](2026-10-27-rust-lua-native-values-cli.json),
[library](2026-10-27-rust-lua-native-values-library.json) and
[official-suite](2026-10-27-rust-lua-native-values-suite.json) observations are
retained. [Validation metadata](2026-10-27-rust-lua-native-values-validation.json)
pins sources, artifacts, workspace inputs and logs. A fresh Cargo target built
the final revision. Official file hashes, assertions, bootstrap, instruction
limit and timeout remain unchanged; cache modes record eighteen stores and
eighteen hits.

The strict CLI, library and official-file audits retain nonzero exits. CLI
differences are exact version banners; fourteen library cases still fail
because production native loading is unsupported. Official `files.lua:59`
also fails in the C reference on Windows rename of an open input. Full
`all.lua`/`main.lua` passage, remaining C/auxiliary APIs, inactive/cross-thread
C access, general allocation-failure recovery, broader native GC/debug/scratch
behavior and production native-module loading remain open. Full standalone
Lua 5.1 parity is unproven. Production changes use safe Rust. No performance
tuning or benchmarks accompany this milestone.
