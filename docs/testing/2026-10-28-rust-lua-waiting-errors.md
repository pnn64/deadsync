# Protected errors through a waiting thread

An error raised through another waiting thread now follows that thread's
protection chain. It bypasses handlers on the calling child. Abandoned children
keep the normal status, C/Lua frames and open upvalues visible in Lua 5.1. C
jumps remain inside C boundaries and occur after Rust dispatch has returned.
Full Lua 5.1 parity remains the goal.

The [initial comparison](2026-10-28-rust-lua-waiting-errors-initial-before.json)
shows the preceding release catching the error on the child and continuing
after its local `pcall`. The reference instead returns the original error to
the waiting parent and leaves the child normal. The saved CLI and SDK match the
preceding selected-thread GC checkpoint. The [expanded baseline](2026-10-28-rust-lua-waiting-errors-before.json)
retains the preceding release's failure against the final corpus.

Errors now carry an internal destination while returning through intervening
C/Rust invocations. A scope on another thread cannot catch that error, and a
Rust callback cannot suppress it by ignoring a nested call's error. The matching
driver transfers to the destination before searching its protected frames.
Abandoned threads keep their frames and cells while their resume links are
detached. Public execution errors discard the internal destination before
returning to the embedding application.

The [public-header release comparison](2026-10-28-rust-lua-waiting-errors-current.json),
[main native boundary](2026-10-28-rust-lua-waiting-errors-boundary-current.json) and
[assertion-enabled reference](2026-10-28-rust-lua-waiting-errors-checked.json)
match all 736 observations. They cover Lua and C resumers, child `pcall` scopes,
main-thread and coroutine parents, protected API calls on five target states,
full/stepped GC errors, and errors raised by a handler toward its own waiting
parent. Table, binary string, signed-zero number, boolean, nil, function,
userdata and thread errors preserve identity. Handlers can collect while fault
frames are visible. Weak references verify that abandoned threads are reclaimed
after both child and parent roots are released; exported upvalues retain their
values and remain mutable.

C-call accounting also matters after a nonlocal return. A normal thread whose
invocation was abandoned has no live C callback, so its saved count must be
read without charging its last visible C frame again. Native resumer frames
that have returned before an error handler runs also stop adding a C-call charge.
The [initial count probe](2026-10-28-rust-lua-waiting-errors-level-first.json)
records the first off-by-one results; the
[intermediate comparison](2026-10-28-rust-lua-waiting-errors-level-focused-resume-before.json)
retains sixteen native-resumer mismatches before that second correction.
The [final count audit](2026-10-28-rust-lua-waiting-errors-level-current.json)
matches the entire matrix and verifies 800 copies, zero-count writes, selected
API activations and restorations. It uses the preceding boundary module's
reference `nCcalls` observation and the corresponding Rust query; module and
binary digests are recorded.

Two portable unit regressions check that a protected Rust callback on the wrong
thread cannot swallow the transfer, and that a public execution error has no
internal destination. The C integration corpus exercises the observable state
and collection behavior through unmodified public headers.

The first expanded fixtures had shared assumptions that the reference rejected.
The snapshot after a nested `pcall` has four frames rather than five. A
protected API error belongs at the final restored stack slot, which can follow
a suspended C prefix. A dead coroutine parent also retains a child argument in
its fault frames, so the fixture releases both roots before asserting collection.
Original fixtures, diagnostics and reports remain preserved. A temporary
native-resumer accounting edit addressed an owner frame before restoring its
thread; the resulting bounds failure is also preserved. The final corpus and
count audit complete normally.

The [validation report](2026-10-28-rust-lua-waiting-errors-validation.json) passes
792 Rust tests on Windows x64 and 793 on Linux x64 and i686 SSE2, strict Clippy,
formatting, builds without default features and the empty normal/build dependency
tree. The [argument audit](2026-10-28-rust-lua-waiting-errors-stdlib.json) matches
all 5,507 library argument cases. Both native-enabled i686 stock entry points
pass with T/testC absent. Windows retains 17/18 independent passes in all four
modes and the shared `files.lua:59` rename failure. Earlier execution,
concatenation, allocation and selected-GC corpora are rechecked with the final
release binary. The [evidence manifest](2026-10-28-rust-lua-waiting-errors-evidence.json)
binds source, report, binary, module and log digests.

The 36 successful native-boundary processes balance 850,268 dispatches and
returns, 8,205 state creations and destructions, and 46 library opens and closes.
The five expected terminating processes exit with status 1 and are recorded
separately from those cleanup totals.

Unprotected waiting targets and all interleaved reader/body/API continuations
remain unfinished. The broader concatenation-resume audit remains a strict
6/20 comparison, despite all Rust processes completing without the earlier
aborts. The strict reader audit still matches 19/24 cases. Checked-reference
invariant failures, reference crashes, the unrooted package-name mismatch,
unequal raw heap counts and the unsettled GC timing failures remain recorded.
Core declaration coverage remains 76/80, with `lua_close`, `lua_getallocf`,
`lua_newstate` and `lua_setallocf` missing. Full continuation, collector,
lifecycle/allocator and resource/OOM parity still need work. This checkpoint
does not establish full standalone Lua 5.1 parity.
