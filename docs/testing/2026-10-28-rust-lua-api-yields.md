# Yields through another thread's protected C API call

The three protected-yield mismatches retained by the preceding
[thread-call milestone](2026-10-28-rust-lua-inactive-calls.md) now match the
standalone Lua 5.1.5 reference. A callback entered by `lua_pcall` through a
completed, yielded or failed target can yield with protected status 0 and target
status 1. A fresh target still rejects the yield. This closes these observed
cases, while full Lua 5.1 parity remains the goal.

An API invocation now has its own protection scope, separate from coroutine
resume. Its status travels outside the Lua stack, avoiding a hidden status slot
that would move pending registers or open cells. The existing error handler and
protected-call scheduler still handle normal returns and errors. The numerical
C-call check remains in force; the structural boundary admits the selected API
call's yield while retaining barriers around deeper synchronous calls.

At a permitted yield, the driver exposes the yielded range and preserves the
pending frames. An invisible continuation finishes that call on later resumption.
Its error handler expires when the original API invocation returns. The caller
is restored and remains a collector root throughout target execution. Normal
completion retains the preceding target-status and CallInfo-base behavior.

The [preserved original audit](2026-10-28-rust-lua-api-yields-before.json)
records three mismatches and matching fresh-target rejection. The
[final release audit](2026-10-28-rust-lua-api-yields-current.json) matches all
four cases, including exact stdout, stderr and process status. The
[preserved preceding release CLI](2026-10-28-rust-lua-api-yields-extended-before.json)
also fails the first permitted-yield assertion in the expanded fixture. Its
digest matches the preceding milestone's source-built argument-audit binary.

The new shared public-header fixture completes 51 observations in the
[current boundary comparison](2026-10-28-rust-lua-api-yields-boundary-current.json):

- Lua and native C yielding callbacks on fresh, completed, yielded and failed
  targets, with open results and requested counts 0, 1, 3 and 5.
- Yielding `lua_cpcall` bodies on all four target classes.
- Completed-target resumption with exact result counts, nil padding and pending
  locals retained across two collections.
- A shared open cell changed through `debug.setupvalue` while suspended, with
  the old object collected and the replacement visible after resumption.
- A resumed error retaining arbitrary value identity after the original API
  handler has expired.

All 1,631 stdout bytes match. The Rust process balances 1,240 dispatch returns,
destroys all 52 C states and closes its library reference under `panic=abort`.
The earlier 72-call fixture still matches all 2,991 bytes. Six public-header
test groups now exercise the default native adapter on Windows x64, Linux x64
and Linux i686 SSE2. The safe Rust regression also checks that a completed target
can yield through the API and subsequently resume while the caller stays usable.

The [platform validation](2026-10-28-rust-lua-api-yields-validation.json)
passes 765 tests on Windows and 766 on each Linux target, strict Clippy, formatting,
all-targets builds with default features disabled and the empty normal/build
dependency tree. The [argument audit](2026-10-28-rust-lua-api-yields-stdlib.json)
matches all 5,507 cases. Unchanged `main.lua` and `all.lua` pass in both
[i686 SSE2 CLIs](2026-10-28-rust-lua-api-yields-i686-suite.json) with stock native
libraries enabled; T/testC remains absent. The
[Windows independent probes](2026-10-28-rust-lua-api-yields-windows-suite.json)
retain 17/18 passes in each of four modes and their failing strict exit, with the
shared `files.lua:59` rename failure.

The [first validation](2026-10-28-rust-lua-api-yields-validation-pre-removal.json)
failed one Windows OS fixture: its final cleanup found the original file absent
after six pathological removal attempts. The cause of that disappearance is
not established. Both the [normalized-path audit](2026-10-28-rust-lua-api-yields-remove-audit.json)
and [raw-path audit](2026-10-28-rust-lua-api-yields-remove-raw-audit.json) preserved
the file through all six cases in both engines. The fixture now creates an
identical file for each attempt and compares the complete error tuple, file
existence and contents afterward, with explicit placeholders for absent values.
All six inputs remain. It passes all 18 OS checks and the complete platform
validation. No removal runtime change is inferred from this result. The first
validation report and original log hashes remain preserved.

The first final-source boundary runner failed while the argument audit was also
building the official CLI in their shared build directory. Its runner failure
is preserved, but the other build replaced the shared compiler log; the precise
compiler failure is unavailable. The isolated boundary build and comparison
passed after the argument audit finished. No runtime source was changed for
that rerun.

The [evidence manifest](2026-10-28-rust-lua-api-yields-evidence.json) verifies
current source, report, binary, artifact and log hashes. All 23 Rust boundary
processes balance 726,606 dispatch returns, destroy 5,979 C states and close 33
native loader references. The first failed Windows validation is retained
separately from the passing final validation.

This corpus does not establish resumption through arbitrary underlying suspended
C chains or broader hook-yield behavior. The five missing lifecycle/allocator
declarations, unprotected other-thread calls, direct inactive metamethod/loading
operations, complete panic/error routing, native resource/OOM windows,
parser-internal mutation, foreign payload/FILE pointers and broader debug/GC
behavior remain open. The earlier lowered-C-call-count protected-yield mismatch
and the unrooted search-name reference mismatch remain recorded. C declaration
coverage stays 75/80. Full standalone parity is not proven, and performance work
remains deferred.
