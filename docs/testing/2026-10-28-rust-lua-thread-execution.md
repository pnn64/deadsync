# Creating threads, formatting and concatenating on a selected stack

The native adapter now executes `lua_newthread`, formatted-message construction
and `lua_concat` on another thread without resuming its original entry. The
portable interface adds `NativeStack::new_thread_on` and `concatenate_thread`.
The shared execution window also handles selected-thread loading and explicit
collector checks. Full Lua 5.1 parity remains the goal.

The [preceding release](2026-10-28-rust-lua-thread-execution-before.json) fails
the initial construction probe with `inactive operation requires execution
support`. Its saved CLI and SDK digests match the preceding native allocation
checkpoint. The initial 35-observation reference run completes. Later fixtures
extend this corpus with metamethods, waiting targets and nested execution.

The [native boundary comparison](2026-10-28-rust-lua-thread-execution-boundary-current.json)
and [public-header release comparison](2026-10-28-rust-lua-thread-execution-current.json)
match 54 execution observations. Fresh, completed, yielded, failed and suspended
C targets cover child identity, inherited environments and hook configuration,
formatted values, zero/one/multiple concatenation operands and Lua metamethods.
Additional cases execute on a waiting parent and nest operations on two targets.
Child threads start empty and do not execute during creation.

One source detail changes observable results. Lua 5.1's `luaV_concat` keeps its
loop offsets relative to `L->base`, but a completed tag call restores the current
CallInfo base. On a suspended C target this can move the base before the yielded
suffix. Later passes then read different physical slots. The implementation
performs the same primitive batches and individual tag calls, retaining the
physical operand window until the API exits. Two ordinary metamethod cases
therefore return `"a"` or the original first table on this target. Identity is
asserted with `rawequal`; process-specific pointer text is not compared.
The [checked reference](2026-10-28-rust-lua-thread-execution-checked.json) completes
all 54 observations with the same bytes.

The [panic comparison](2026-10-28-rust-lua-thread-execution-panic-current.json)
matches ten observations across the same five idle target states. Plain type
errors and errors raised by a concatenation metamethod reach the target's panic
callback after Rust dispatch has returned. The reset preserves original error
and thread identity, removes fault frames, closes exported upvalues and leaves
the caller usable. The callback recovers through a C-local jump. A separate
[checked-reference run](2026-10-28-rust-lua-thread-execution-panic-checked.json)
also completes with identical bytes.

The expanded [allocation corpus](2026-10-28-rust-lua-thread-execution-allocation-current.json)
matches 156 observations: 26 in the active C frame and 130 on the five selected
target states. The thirty added cases cover formatted strings, concatenation
with multiple operands and thread construction. They observe finalizers before
the allocation, target identity and stack effects, original errors and panic
recovery. The [checked reference](2026-10-28-rust-lua-thread-execution-allocation-checked.json)
also matches. Zero/one-operand concatenation and other nonallocating controls
continue to defer collection. Collector checks in loading and allocation now
share the same base-restoration helper.

Portable regressions check caller restoration, target-prefix preservation,
invalid operand counts, same-thread construction, environment inheritance and
a fatal instruction quota during a selected concatenation. The quota remains
fatal even if the host callback ignores its returned error; the VM is usable
again after the outer execution ends.

The [validation report](2026-10-28-rust-lua-thread-execution-validation.json),
[argument audit](2026-10-28-rust-lua-thread-execution-stdlib.json),
[native-enabled i686 suite](2026-10-28-rust-lua-thread-execution-i686-suite.json)
and [Windows independent probes](2026-10-28-rust-lua-thread-execution-windows-suite.json)
record the broader gates. The [evidence manifest](2026-10-28-rust-lua-thread-execution-evidence.json)
binds source, binaries, reports, logs and public-header modules. Final validation
passes 782 Rust tests on Windows x64 and 783 on Linux x64 and i686 SSE2,
strict Clippy, formatting, builds without default features and the empty
normal/build dependency tree. All 5,507 library argument cases match. Both
native-enabled i686 CLIs pass unchanged `main.lua` and `all.lua`, with T/testC
absent. Windows retains 17/18 independent passes in all four modes and the shared
`files.lua:59` rename failure. Thirty-two normal Rust boundary processes balance
816,067 dispatch returns, destroy 6,681 C states and close 42 library references.
All shared groups are asserted released. Five expected exit-1 boundary processes
remain separate from these cleanup totals.

The [first checked allocation run](2026-10-28-rust-lua-thread-execution-allocation-checked-fixture-before.json)
caught a fixture capacity error: it prepared string operands on a failed target
before reserving its API stack. Capacity is now checked before those pushes.
The [first full boundary run](2026-10-28-rust-lua-thread-execution-boundary-current-fixture-before.json)
also exposed an unstable hook assertion. Lua 5.1 stores debug hooks under raw
pointer keys; a newly allocated child could reuse a previously collected hooked
target's address. The execution fixture now retains hooked targets and children
so the hook-inheritance check has a stable precondition. Original fixtures,
reports and logs are preserved. That first boundary run also reproduced the
previously recorded unrooted package-search-name mismatch. It remains a strict
failure; allocator-dependent reference bytes are not normalized into equality.

Yielding concatenation metamethods remain unfinished. The
[release audit](2026-10-28-rust-lua-thread-execution-yields-current.json) matches
the fresh-target rejection but differs in four other states. Lua 5.1.5 returns
from the API with status `LUA_YIELD` and a visible top of -1; Rust rejects the
yield through the target's panic callback. The
[assertion-enabled reference](2026-10-28-rust-lua-thread-execution-yields-checked.json)
also completes those four cases without an assertion. These are retained strict
failures; successful normal and panic corpora do not establish their parity.

The [unsettled GC audit](2026-10-28-rust-lua-thread-execution-unsettled-current.json)
retains the earlier two file-panic timing mismatches with the final release
binary and the preceding public module. Required allocation checks alone do
not establish equal collector phases or allocation charges. The earlier strict
reader-resume mismatches, checked-continuation invariant failures and Windows
rename failure also remain open. Core declaration coverage stays 76/80, with
`lua_close`, `lua_getallocf`, `lua_newstate` and `lua_setallocf` missing. Errors
into waiting targets, complete continuation and resource/OOM behavior, other
auxiliary safe points and broader GC timing remain unfinished. This checkpoint
does not establish full standalone Lua 5.1 parity.
