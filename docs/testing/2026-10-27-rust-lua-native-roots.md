# Native reader parser roots

The original native-reader stack observation now matches official Lua 5.1.5:
the first reader sees its one caller slot, and the second sees three slots.
Text parsing creates a constants table and an internal prototype for every
active function. Binary decoding installs one prototype after the header and
one for each unfinished child. An initially empty lookahead requests another
block before installing text roots.

These are collector objects with generation-checked identities, rather than
padding slots. Private prototypes have native type tag 9 and no lua_topointer
address. Text tables receive lexical strings (including reserved words),
implicit self/arg/for locals and the logical constant pool's zero-based indices;
the table itself keys the nil constant. Parser continuations install field-name
constants after fetching the following token, and retain a closing function's
roots through that lookahead. The decoder retains source/debug strings,
constants and child prototypes as it reads them. Native prototype metadata
follows the loaded closures: an escaping child keeps its own descendants alive,
without keeping its parent or sibling prototype alive. Temporary constant
tables can be collected after parsing ends.

The collector traces prototype edges, protects new allocations, accounts for
storage and sweeps unreachable prototypes. Loaded programs hold a non-owning
prototype-index map; live closures trace their own entry in that map. Native
loader errors restore the entry window and parser scope, including an overflow
while installing roots. The safe Rust engine adds no unsafe code or normal/build
dependencies. Production continues to use deadlib-lua-legacy.

Detailed table snapshots also exposed a general table-key mismatch: a first
inserted -0 key became +0 when enumerated. Hash/equality still equate both signs,
while stored keys now preserve the inserted sign. A differential Lua test
checks updates, lookups, next continuation keys and collection for both signs.

The Windows/MSVC fixture runs the same C module and reader-roots.lua with both
engines. It compares 1,745 scenarios over 21 sources: text, nested functions,
syntax failures, binary prototype trees and
stripped native debug data. Fixed-size fragments, every two-block split and
empty initial probes produce 7,772 reader observations. Each records top, native
types, raw identity self-equality, prototype NULL pointer conversions and sorted
constant-table contents. Collection runs inside every callback. A separate
weak-reference check retains the loaded entry, then one child, then neither,
and checks which prototypes and constants tables survive. The resulting 470,384
stdout bytes match exactly; the Rust run balances 268,564 dispatch/drop guards
and releases both C thread handles, one before shutdown. Internal prototype
values are inspected with type/raw identity and collector operations; the test
does not call public metamethod APIs on private non-value tags.

Five new portable groups cover this native root lifecycle and signed-zero
iteration: text/nested loading, binary fragmentation through collection,
function order through literal branches and siblings, stack overflow recovery,
and ordinary table-key behavior. The existing fragmented reader test now checks
the exposed table/prototype slots. Fresh verification passes 685 tests on
Windows x64 and 686 on each of Linux x64 and Linux i686 SSE2: 78 unit, four
documentation and 603/604 integration tests. Rustfmt, strict Clippy and the zero
normal/build dependency tree pass.

The retained main C fixture still matches 61,019 stdout bytes and balances
141,654 operation/drop guards. Its 183 thread handles are released, including
182 collected before shutdown. Eighteen imported-name chunks match reference,
direct Rust, cache-store and fresh-process cache-hit modes on each platform.
Unchanged stock main.lua and all.lua pass in both CLIs on Linux i686 SSE2.
Internal T/testC and suite native modules remain absent.

Raw [C-boundary](2026-10-27-rust-lua-native-roots-boundary.json),
[Windows](2026-10-27-rust-lua-native-roots-windows.json),
[Linux](2026-10-27-rust-lua-native-roots-linux.json),
[i686](2026-10-27-rust-lua-native-roots-i686.json) and
[stock-suite](2026-10-27-rust-lua-native-roots-suite.json) reports retain commands
and observations. [Validation metadata](2026-10-27-rust-lua-native-roots-validation.json)
binds the source snapshot, workspace inputs, logs, ABI headers and private
executables. Final build targets start absent and build from source.

This closes the recorded stack-count mismatch and establishes the listed
read-only observations and lifetimes. Arbitrary mutation of private parser
state is unfinished. Inactive C stack operations, coroutine C execution APIs,
state lifecycle, allocator/OOM behavior, other C APIs and complete standalone
Lua 5.1 parity remain open. The boundary still declares 61 of 80 core APIs;
declarations do not prove complete semantics. Later dialects and integrations
remain open. Optimization and benchmarks stay deferred.
