# Selected concatenation yields and signed stack offsets

Selected-thread `lua_concat` now retains pending Lua frames when a metamethod
yields at a permitted C-call count. The C API continues its remaining stack
reductions before returning. Native stacks expose the resulting signed offset
through `top_offset`; `top` continues to count accessible values. Full Lua 5.1
parity remains the goal.

The [preceding release](2026-10-28-rust-lua-concat-yields-before.json) rejects
completed-target concatenation yields through its panic callback. Its preserved
CLI and SDK match the preceding selected-thread execution checkpoint. The
original [five-state release probe](2026-10-28-rust-lua-concat-yields-original-current.json)
now matches all five cases. Fresh targets still reject a yield; completed,
yielded, failed and suspended C targets retain status `LUA_YIELD` and report a
visible top of -1 when the metamethod yields one value.

Lua 5.1's `callTMres` consumes one physical slot after `luaD_call` returns,
even if the thread yielded. `lua_concat` then consumes the remaining operands.
The implementation now keeps the pending call continuations, uses the last
physical yielded slot as the tag result and applies both reductions. Zero to
three yielded values produce offsets -2 through 1 in the two-operand cases.
Primitive pushes advance this physical top, including pushes below the base;
setting an absolute top restores an ordinary visible window.

The [public-header comparison](2026-10-28-rust-lua-concat-yields-current.json),
[main native boundary comparison](2026-10-28-rust-lua-concat-yields-boundary-current.json)
and [assertion-enabled reference](2026-10-28-rust-lua-concat-yields-checked.json)
match all forty observations. Twenty exercise five target states and four yield
counts. Sixteen inspect three primitive pushes and absolute stack restoration
after the permitted yields. Four resume completed targets with two or three
yielded values. They check resume arguments, returned values, repeated yields,
surviving upvalues, mutation through `debug.setupvalue` and collection between
resumptions. A portable Rust regression checks owner restoration and pushes
through a negative selected-thread offset.

The repeated-yield test initially exposed a stale yield boundary. The C API's
invocation has returned, but retained frames still carried its barrier count.
Resumption now removes that escaped boundary while retaining nested boundaries
above it. The [initial comparison](2026-10-28-rust-lua-concat-yields-focused-repeat-before.json)
records the second-yield rejection before this fix.

The full [validation](2026-10-28-rust-lua-concat-yields-validation.json) passes
784 Rust tests on Windows x64 and 785 on Linux x64 and i686 SSE2, strict Clippy,
formatting, builds without default features and the empty normal/build dependency
tree. The [argument audit](2026-10-28-rust-lua-concat-yields-stdlib.json) matches
all 5,507 library argument cases. Both native-enabled i686 CLIs pass unchanged
`main.lua` and `all.lua`, with T/testC absent. Windows retains the shared
`files.lua:59` rename failure and 17/18 independent passes in all four modes.
The preceding 54 execution, ten panic and 156 allocation observations are
rechecked with the final binary and checked reference. The
[evidence manifest](2026-10-28-rust-lua-concat-yields-evidence.json) binds current
sources, reports, binaries, public modules and logs.
Thirty-three normal Rust boundary processes balance 817,009 dispatch returns,
destroy 6,706 C states and close 43 library references. Shared groups are asserted
released. Five expected exit-1 processes remain outside these cleanup totals.

The first main-boundary build failed because its minimal ABI header lacks the
public header's `lua_isboolean` convenience macro. The fixture now uses the
equivalent `lua_type` comparison. The original fixture, failed link log and
preceding validation reports are preserved; the complete source-bound checks
were repeated after the correction.

Broader concatenation resumptions remain unfinished. The
[initial twenty-case audit](2026-10-28-rust-lua-concat-yields-resume-first.json)
and [final release audit](2026-10-28-rust-lua-concat-yields-resume-current.json)
retain every case, including reference failures. Fourteen resumptions fail
assertions in the checked reference; eight crash the release reference. Some
release-only results contain process-specific function addresses. These results
are not normalized into passing comparisons. Rust also aborts in some of these
resumption cases. The forty passing observations do not prove
these continuations correct or establish complete error/resource behavior.

The [unsettled GC comparison](2026-10-28-rust-lua-concat-yields-unsettled-current.json)
also retains the two earlier file-panic timing mismatches with the final release
binary and preceding public module. The earlier unrooted package-search-name
mismatch remains recorded. The [current strict reader audit](2026-10-28-rust-lua-concat-yields-reader-resume-current.json)
still matches 19/24 cases, with the five reference crashes retained. Core declaration
coverage stays 76/80: `lua_close`, `lua_getallocf`, `lua_newstate` and
`lua_setallocf` remain missing. Waiting-target error routing, full continuations,
GC timing, auxiliary safe points and complete resource/OOM behavior still need
work. This checkpoint does not establish full standalone Lua 5.1 parity.
