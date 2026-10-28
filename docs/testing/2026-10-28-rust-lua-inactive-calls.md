# Protected calls through other native thread stacks

`NativeStack::protected_call_thread` and the runtime C adapter now execute
`lua_pcall` and ordinary `lua_cpcall` bodies through a selected thread's own
stack. This is separate from coroutine resume: it does not start the thread's
pending entry function or resume its suspended computation. Full inactive-state
execution remains open, including a strict protected-yield mismatch described
below.

The driver exchanges the VM's active vectors with the selected thread and uses
the existing protected-call scheduler. It adds no visible wrapper activation.
The original caller remains a collector root while another thread executes, and
its native window is refreshed after callbacks that modify it. Error handlers
run on the target while failing frames and open cells remain available. Result
and error placement share the existing active-thread implementation.

The target's retained status is observable even during a C callback entered by
this API. A yielded or failed thread can therefore report status 1 or 2 while
the protected callback executes. Normal completion restores the saved CallInfo
base, which can precede the temporary result suffix exposed by `lua_yield`.
Calling through a thread that yielded only its last argument exposes earlier
arguments again after that base restoration. These behaviors follow the pinned
[Lua 5.1.5 API](https://www.lua.org/source/5.1/lapi.c.html) and
[call/protection paths](https://www.lua.org/source/5.1/ldo.c.html).

The [preserved preceding CLI](2026-10-28-rust-lua-inactive-calls-before.json)
fails at the first protected call on another thread. Its digest matches the
preceding milestone's source-built argument-audit binary. The same module built
against official public Lua headers completes 65 observations in the reference.

The [current boundary comparison](2026-10-28-rust-lua-inactive-calls-boundary-current.json)
adds five direct reference C-call counter observations and two counter-write
cases, for 72 matching call observations and all 2,991 stdout bytes. Its Rust
process balances 1,956 dispatches, destroys all 74 C states and closes its one
library reference under `panic=abort`. The shared fixture
covers fresh, completed, yielded, partial-yield and failed threads; fixed/open
results and nil padding; arbitrary error identity; successful, failing and
nonfunction error handlers; C callbacks and C-call data pointers; handler-local
mutation and cell closure; collection; recursive calls through the original
waiting caller; and ordinary hook event records. Fresh and suspended entry
computations are subsequently resumed and checked. The public-header module
also runs through the default adapter on Windows x64, Linux x64 and i686 SSE2.

The [first boundary run](2026-10-28-rust-lua-inactive-calls-boundary-first.json)
retains a fixture error: the new counter callback omitted the thread argument
required by `boundary_level`, causing a reference null dereference and a Rust
assertion failure. The current fixture passes `coroutine.running()` explicitly.
No runtime fix or matching claim is inferred from that invalid probe.

The [subsequent counter comparison](2026-10-28-rust-lua-inactive-calls-boundary-current-level-before.json)
retains one real mismatch: calls through a partial-yield C closure counted an
extra C invocation in Rust (4 versus 3). Activation now calibrates to the saved
target count plus the API call, removing that temporary adjustment on return.
Successful explicit `lua_setlevel` writes survive, including unsigned counter
wrap; protected errors restore the entry count, as the reference does.

The separate [first yield audit](2026-10-28-rust-lua-inactive-calls-yield-first.json)
found a Rust abort when a protected callback yielded through a completed target
with no coroutine resumer. A bare target now enters through a structural C API
boundary, so this case returns through the host instead of reaching the
resume-only transition. A portable regression checks host usability without
asserting the mismatching yield status as correct.

The [current strict yield audit](2026-10-28-rust-lua-inactive-calls-yield-current.json)
still fails. Fresh-target yield rejection matches. In the three completed,
yielded and failed target cases, the release reference permits the callback to
yield and returns protected status 0 with thread status 1. Rust retains its
structural boundary and returns protected status 2. These raw outcomes are
preserved and are not normalized into a match. Returning normally from Rust
fixes the abort; it does not establish Lua 5.1 parity for those calls.

Three portable Rust groups check validation before mutation, shared instruction
limits through nested resumed children, context restoration, sticky aborts and
VM reuse, plus the completed-target crash regression. The
[platform validation](2026-10-28-rust-lua-inactive-calls-validation.json),
[library argument audit](2026-10-28-rust-lua-inactive-calls-stdlib.json),
[native-enabled i686 suite](2026-10-28-rust-lua-inactive-calls-i686-suite.json),
[Windows cached probes](2026-10-28-rust-lua-inactive-calls-windows-suite.json) and
[evidence manifest](2026-10-28-rust-lua-inactive-calls-evidence.json) retain exact
inputs, source hashes and process output.

The final validation passes 764 tests on Windows and 765 on each Linux target,
strict Clippy, formatting, all-targets builds with default features disabled and
the empty normal/build dependency tree. All 5,507 library argument cases still
match. Unchanged `main.lua` and `all.lua` pass in both i686 SSE2 CLIs with native
libraries enabled; T/testC remains absent. Windows retains 17/18 independent
passes in reference, Rust and both cache modes, with the shared `files.lua:59`
rename failure and a failing strict runner.

All twenty-two Rust processes in the main boundary corpus balance 725,356
dispatch returns, destroy 5,927 C states and close 32 native loader references.
These passing corpus results do not include the separate failing yield audit.

The five missing lifecycle/allocator declarations, unprotected other-thread
calls, direct inactive metamethod/loading operations, complete panic/error
routing, native resource/OOM windows, parser-internal mutation, foreign payload
and FILE pointer behavior, broader debug/hook/scratch-stack behavior and the
earlier lowered-call-count yield mismatch remain open. The unrooted search-name
reference mismatch also remains recorded. Full standalone Lua 5.1 parity is the
goal; selected passing calls do not close it. Performance work stays deferred.
