# Calls and panic recovery through another thread

`lua_call` now runs through a selected thread's stack without installing a local
error handler. The preceding release rejected that operation. Calls retain the
selected environment and execution status on ordinary completion, preserve
fixed/open results, and leave faulting frames available to the selected state's
protection chain or panic callback. Full Lua 5.1 parity remains the goal.

The [baseline](2026-10-28-rust-lua-thread-unprotected-before.json) preserves the
preceding release's failure against the final corpus, with its saved executable
and SDK tied to the preceding waiting-error checkpoint. The
[public-header release comparison](2026-10-28-rust-lua-thread-unprotected-current.json),
[assertion-enabled reference](2026-10-28-rust-lua-thread-unprotected-checked.json)
and [native-boundary run](2026-10-28-rust-lua-thread-unprotected-boundary-current.json)
match 960 observations:

- 160 successful calls on fresh, completed, yielded, failed and partial C states,
  covering result counts, nils, signed zero, nested calls and collection.
- 160 unprotected call errors on idle states, with Lua/C failures, two panic
  callbacks and eight error-value types. Exported locals survive panic reset.
- 480 errors targeting waiting states with protected main, coroutine or C API
  scopes. Child handlers are bypassed; original/replacement values, abandoned
  child frames, open upvalues and weak collection match the reference.
- 160 panic recoveries on unprotected waiting states. A child recovers locally
  in C, inspects its reset parent, and raises toward a protected main-thread
  scope. Lua and native C resumers preserve both threads' observable state.

These paths exposed several restoration bugs. An unprotected call's old window
end could overwrite an open local with the panic error before closing that
local's upvalue. The error window now preserves the fault registers. Returning
through a waiting owner could also revive a state already reset by panic, replace
its stack or C-call count, or synchronize a native resumer frame removed by that
reset. Those wrappers now preserve the completed reset while carrying the error
out to its destination. Jumps remain inside C after Rust dispatch returns.

The [C-call audit](2026-10-28-rust-lua-thread-unprotected-level-current.json)
matches the matrix and adds 1,120 count copies, zero-count writes, selected API
activations and restorations against release and checked references. It binds
the preceding boundary module's reference `nCcalls` observation and the Rust
query by artifact/report digests. The preceding 736 waiting-error cases and
800 count controls are also rechecked with this release.

The [exit audit](2026-10-28-rust-lua-thread-unprotected-exits-current.json)
compares twenty terminating scenarios on all five states, both resumers, and
absent/returning panic callbacks. Every process exits with status 1, preserves
buffered stdout and writes an open file's pending binary bytes. The
[pre-fix comparison](2026-10-28-rust-lua-thread-unprotected-exits-before-flush.json)
records all twenty Rust mismatches: buffered Lua output and file writes were
lost when C called `exit`. The C shim now asks Rust to flush VM-owned streams
after a panic callback returns. A callback that recovers through its C-local
jump does not trigger the exit flush. The same stream-flush implementation is
shared with `os.exit`; it does not close the Lua state or invoke finalizers.

Original failed fixtures and intermediate reports remain preserved. The first
fixture assumed a negative-zero literal would survive sharing a constant pool
with positive zero; the reference instead coalesces those equal numeric keys.
The corpus obtains negative zero from a separate function's constant pool. A
partial C yield also restores two hidden prefix slots after `lua_call`, rather
than retaining the temporary yielded base. A temporary resumer restoration
attempt hit a removed frame and aborted; the final matrix completes normally.
The first full validation retained a shared fixture compile error in the
development header and the missing exit flush. Both were corrected and the
complete validation rerun with unchanged source digests.

The [validation report](2026-10-28-rust-lua-thread-unprotected-validation.json)
passes 794 Rust tests on Windows x64 and 795 on Linux x64 and i686 SSE2, strict
Clippy, formatting, builds without default features, and an empty normal/build
dependency tree. The [library argument audit](2026-10-28-rust-lua-thread-unprotected-stdlib.json)
matches all 5,507 cases. Both native-enabled i686 stock entry points pass with
T/testC absent. Windows still passes 17/18 independent files in all four modes,
with the shared `files.lua:59` rename failure. Earlier execution, concatenation,
allocation and selected-GC corpora are rechecked. The
[evidence manifest](2026-10-28-rust-lua-thread-unprotected-evidence.json) binds
current source, binary, report, module and log digests.

The 37 successful native-boundary processes balance 895,982 dispatches and
returns, 9,782 state creations and destructions, and 47 library opens and closes.
The 25 expected terminating processes are recorded separately from those cleanup
totals. Twenty of them exercise the new waiting-state exit paths.

This corpus does not complete arbitrary interleaved reader/body/API execution.
The broader resume audit remains a strict 6/20 comparison, with all Rust
processes completing, and the reader audit remains 19/24. Checked-reference
invariant failures, reference crashes, unsettled collector timing, unequal raw
heap counts and the unrooted package-name mismatch remain recorded. Core C API
declaration coverage remains 76/80: `lua_close`, `lua_getallocf`, `lua_newstate`
and `lua_setallocf` are missing. Complete lifecycle/allocator, collector,
continuation and resource/OOM parity remain unfinished.
