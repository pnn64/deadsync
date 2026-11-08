# Loading that continues after a reader yield

Selected-thread loading now preserves the parser's final physical stack top and
pending reader frames when parsing succeeds after a yield. The preceding
[loader milestone](2026-10-28-rust-lua-thread-loads.md) covered a permitted first
yield followed by a later reader failure. Successful continuation exposed
additional stack, lifetime and current-function differences. Full Lua 5.1 parity
remains the goal, and arbitrary reader continuation parity is still unproven.

The [preserved preceding release](2026-10-28-rust-lua-reader-continuations-before.json)
matches 48 of 96 fragmented empty/text/syntax/binary cases. The other cases lose
yielded values, restore the wrong visible top or return a parser-root/stack error.
Its digest matches the preceding milestone's source-built argument-audit CLI.
The [shared corpus baseline](2026-10-28-rust-lua-reader-continuations-corpus-before.json)
also fails at the first completed-target continuation while the reference
finishes all 114 snapshot/resumption observations.

Parser-root pushes and pops now use physical positions independently of the
visible native base. The capacity limit remains relative to the current window.
Successful parsing appends its function at the final parser top. A reader yield
can move the visible base past that function, leaving an empty window despite
successful parsing. Resetting every successful load to its entry top discarded
these observable distinctions.

The driver also retains pending Lua/C frames. Invisible parser and window
continuations expire their protection and finish transparently on later
resumption. A subsequent C API call restores the latest current activation's
base, rather than the activation that preceded loading. Environment and capture
lookup follow that current function. Parser failures still restore the saved
entry top and CallInfo base.

The [current boundary comparison](2026-10-28-rust-lua-reader-continuations-boundary-current.json)
and [direct release comparison](2026-10-28-rust-lua-reader-continuations-current.json)
match 122 observations and all 6,152 stdout bytes:

- 96 snapshots cover four target states, four input forms, three block sizes and
  a yield requested on the first or second reader invocation. They check parser
  status, both stack sizes, retained native status, result type and coroutine
  status. Binary reader counts follow the actual ABI-specific wire length.
- 18 completed-target resumption cases check exact result counts, values,
  collection and final status. Two captured-reader cases expose the reference's
  constants table and prototype after resumption. The fixture validates the
  table's exact entries and metatable, the prototype type and survival across
  collection instead of printing addresses.
- Eight environment cases distinguish target globals from the yielded C
  function's environment, including transitions after the first reader call.

The initial fixture incorrectly assumed every successful resume returned only
its success flag. Both engines returned the same additional parser anchors in
the captured-reader cases. The
[raw captured-reader comparison](2026-10-28-rust-lua-reader-continuations-captured-resume-first.json)
preserves their types and table contents; the shared assertions now reflect
those observations.

Eight public-header Rust integration groups run through the default adapter.
A new portable regression also exercises a syntax failure after a permitted
reader yield. The parser can unwind every reader frame while retaining native
status 1. Rust now rejects both Lua and native resume attempts when no suspended
frame remains, rather than underflowing its stack index. The VM remains usable.
That defensive behavior is not claimed to match the reference crashes below.

The [strict release resume audit](2026-10-28-rust-lua-reader-continuations-resume-current.json)
matches 19 of 24 separate cases and still exits 1. Five syntax-error cases crash
the reference when subsequently resumed. Every Rust process exits normally,
reports the rejected resume and completes a VM-usability control. The
[first audit](2026-10-28-rust-lua-reader-continuations-resume-first.json) retains
the earlier Rust underflows. No crash is normalized into a match.

An unchanged-source, assertion-enabled reference gives additional
[resume diagnostics](2026-10-28-rust-lua-reader-continuations-resume-checked.json).
It rejects those five cases at the missing current-function invariant in
`ldo.c:395`, and two other resume shapes at the stack-top/open-call invariant in
`lvm.c:405`. The shared continuation corpus also reaches that latter assertion.
Release equality therefore does not prove that these continuations satisfy the
reference's internal invariants. These diagnostic failures remain separate from
the passing release corpus.

The C fixture required three corrections. Its first audit generator produced a
[malformed-number syntax error](2026-10-28-rust-lua-reader-continuations-probe-syntax-first.json).
The [next raw audit](2026-10-28-rust-lua-reader-continuations-raw-before.json)
attempted to copy index -1 even when the visible window was empty; the fixture
now returns nil in that case. The initial
[environment audit](2026-10-28-rust-lua-reader-continuations-environments-first.json)
also read `LUA_ENVIRONINDEX` without a current function and omitted explicit
reader stack reservation on failed targets. The
[first checked build](2026-10-28-rust-lua-reader-continuations-environments-checked.json)
diagnoses both issues, retaining its original source copies and four rejected
inputs. The fixture now reserves its temporary space and uses the globals
pseudo-index when `lua_getstack` finds no current function. All eight
[corrected environment comparisons](2026-10-28-rust-lua-reader-continuations-environments-current.json)
match, and all eight pass the
[checked reference](2026-10-28-rust-lua-reader-continuations-environments-checked-current.json).
The old invalid-input crashes are retained as fixture failures.

The [final validation](2026-10-28-rust-lua-reader-continuations-validation.json)
passes 770 tests on Windows x64 and 771 on Linux x64 and i686 SSE2, strict Clippy,
formatting, all-targets builds with default features disabled and the empty
normal/build dependency tree. All [5,507 argument cases](2026-10-28-rust-lua-reader-continuations-stdlib.json)
still match. Unchanged `main.lua` and `all.lua` pass in both
[native-enabled i686 CLIs](2026-10-28-rust-lua-reader-continuations-i686-suite.json);
T/testC remains absent. The
[Windows independent probes](2026-10-28-rust-lua-reader-continuations-windows-suite.json)
retain 17/18 passes in all four modes, the shared `files.lua:59` rename failure
and the failing strict exit.

The [evidence manifest](2026-10-28-rust-lua-reader-continuations-evidence.json)
verifies current source, report, log, artifact and binary hashes. All 25 Rust
boundary processes balance 746,539 dispatch returns, destroy 6,150 C states and
close 35 native loader references. Earlier call, API-yield and loader corpora
still match. The isolated strict resume audit and checked-continuation failures
are explicitly outside those passing corpus totals.

Core declaration coverage remains 75/80. The lifecycle/allocator APIs,
unprotected other-thread calls, direct inactive file/metamethod operations,
complete panic/error/resource/OOM behavior, private parser mutation, foreign
payload/FILE pointers and broader debug/GC behavior remain open. The earlier
lowered-C-call-count yield mismatch and unrooted search-name reference mismatch
remain recorded. Full standalone parity is not proven; performance work remains
deferred.
