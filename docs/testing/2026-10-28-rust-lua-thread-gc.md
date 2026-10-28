# Collector operations on a selected thread

The C adapter now accepts `lua_gc` on another thread. The portable interface
adds `NativeStack::gc_thread`. Heap controls remain shared across the VM, while
finalizers observe the selected thread, environment and retained call frames.
Successful tag calls restore a suspended C target's actual CallInfo base;
operations without a tag call preserve its yielded suffix. Full Lua 5.1 parity
remains the goal.

The [preceding release](2026-10-28-rust-lua-thread-gc-before.json) rejects the
first selected-thread control with `inactive operation requires execution
support`. Its saved CLI and SDK match the preceding concatenation-yield
checkpoint. The [current public-header comparison](2026-10-28-rust-lua-thread-gc-current.json),
[main native boundary](2026-10-28-rust-lua-thread-gc-boundary-current.json) and
[checked reference](2026-10-28-rust-lua-thread-gc-checked.json) match 52
observations. Five idle target states cover stop/restart, full collection,
memory-count queries, pause and step-multiplier controls, invalid options and
collection through a large explicit step. Two additional cases collect on a
waiting parent. Finalizers check thread identity and the caller visible through
`debug.getinfo`. A suspended C target keeps its old base without finalizers and
restores the earlier CallInfo base when a finalizer completes.

Ten memory-count observations validate integer and range properties, including
the 0..1023 byte remainder. Their output records the satisfied contract rather
than exact heap counts. Equal allocation charges and byte-for-byte count parity
are not established. Explicit full collections and large steps give controlled
finalizer points; they do not close the earlier automatic-GC timing mismatch.
The separate [raw-count audit](2026-10-28-rust-lua-thread-gc-counts-current.json)
records the actual numbers without substitution. All ten count observations
differ. Both programs use the same relative executable name, equal-length run
directory names and empty Lua search-path variables. Its strict checker exits 1.

The [panic corpus](2026-10-28-rust-lua-thread-gc-panic-current.json) matches
100 observations against both release and [checked Lua 5.1.5](2026-10-28-rust-lua-thread-gc-panic-checked.json).
Across the five idle states, full collection and stepping preserve table,
string, number, boolean and nil error identity. A finalizer can replace the
panic callback after collection starts. Rust dispatch returns before the
latest C callback runs; that callback recovers through a C-local jump. Reset
closes exported upvalues, removes fault frames, preserves the target identity
and leaves the caller usable. Protected error routing into a waiting target
remains unfinished.

The portable tests verify collection on a completely full 8,000-slot target
without a spare public result slot, same-thread control fallback, caller and
target-prefix preservation, and a fatal instruction quota during a selected
finalizer. Ignoring the quota error in the host does not suppress it. The VM
can execute again after the failed outer call.

This work also removes the eight Rust aborts recorded in the preceding broad
concatenation-resume audit. A newer yielded call had consumed the thread's one
resume-argument record; execution then reached an older retained native yield
and panicked on the absent record. Invalid top/base combinations could also
underflow an unsigned count. Both native yield paths now validate resume state
and report a Lua runtime error. A portable regression exercises eight combinations
of retained Lua/C yields and yielded-value counts, then verifies VM reuse.

The [current broad audit](2026-10-28-rust-lua-resume-arguments-current.json)
finishes all twenty Rust processes without an abort, but remains a strict
6/20 comparison. Fourteen cases still fail checked-reference invariants, and
eight crash the release reference. Other release-only results contain opaque
function addresses. These failures and unequal results are retained. The new
Lua errors do not establish parity for these continuations. The
[current reader audit](2026-10-28-rust-lua-thread-gc-reader-resume-current.json)
also remains strict, matching 19/24 cases with five reference crashes.

The [validation report](2026-10-28-rust-lua-thread-gc-validation.json) passes
789 Rust tests on Windows x64 and 790 on Linux x64 and i686 SSE2, strict Clippy,
formatting, builds without default features and the empty normal/build dependency
tree. The [argument audit](2026-10-28-rust-lua-thread-gc-stdlib.json) matches all
5,507 library argument cases. Native-enabled i686 `main.lua` and `all.lua` pass
with T/testC absent. Windows retains 17/18 independent passes in all four modes
and the shared `files.lua:59` rename failure. Earlier 54 execution, ten
concatenation-panic, forty concatenation-yield and 156 allocation observations
are rechecked against the final release binary. The
[evidence manifest](2026-10-28-rust-lua-thread-gc-evidence.json) binds source,
binary, report, artifact and log digests.
Thirty-five normal Rust boundary processes balance 821,082 dispatch returns,
destroy 6,860 C states and close 45 library references. Shared groups are asserted
released. Five expected exit-1 processes remain outside these cleanup totals.

The [unsettled GC audit](2026-10-28-rust-lua-thread-gc-unsettled-current.json)
retains both earlier file-panic timing mismatches. The prior unrooted package-name
mismatch and checked reader-continuation failures remain recorded. Core
declaration coverage remains 76/80: `lua_close`, `lua_getallocf`, `lua_newstate`
and `lua_setallocf` are missing. Full continuations, waiting-target errors,
collector timing and charges, auxiliary safe points, lifecycle/allocator APIs
and complete resource/OOM behavior remain unfinished. This checkpoint does not
establish full standalone Lua 5.1 parity.
