# Protected loading through another thread's stack

`NativeStack::load_reader_thread` now compiles through a selected thread's own
native stack. The runtime adapter uses it for `lua_load`, including the public
buffer/string helpers that delegate to that API. Parsing does not resume the
target's pending entry function or suspended computation. Complete inactive-state
execution and full Lua 5.1 parity remain open.

A temporary, invisible window lends the selected stack to the existing native
loader. Reader callbacks see its retained status, calling environment and globals.
Compiled functions inherit the target's globals. The original caller stays
rooted during target execution, including collection and nested reader calls.
Protected parser cleanup and inherited error handling use the existing loader;
fatal instruction limits still escape and remain sticky after restoring the
original context. C reader errors return through Rust before the C-only jump.

The loader now preserves an absolute entry top when its visible base changes.
After an ordinary nested C API call, Lua 5.1 restores the current CallInfo base.
That can expose arguments preceding the temporary suffix installed by a prior
C yield. Ordinary parsing without that nested call keeps the narrower base.
Parser errors also restore the CallInfo base. A selected window preserves these
distinctions rather than discarding the earlier arguments.

Reader callbacks can also enter Lua through a C API invocation that yields.
The invocation scope retains pending frames and returns to the C reader without
requiring a coroutine resumer. Its explicit C-call counter increment survives
until the call returns or protected error cleanup restores the saved count.
The target retains its new yield status if a later reader call fails. Scheduler
cleanup must therefore restore both the native status and `coroutine.status`.

The [preceding release CLI](2026-10-28-rust-lua-thread-loads-before.json) fails
the first selected-thread load. Its digest matches the preceding milestone's
source-built argument-audit binary. The same public-header fixture completes
47 observations in the reference. The
[current boundary comparison](2026-10-28-rust-lua-thread-loads-boundary-current.json)
and [direct release comparison](2026-10-28-rust-lua-thread-loads-current.json)
match those observations and their exact output:

- Fragmented text, syntax failures and named binary chunks on fresh, completed,
  yielded, failed and partial-yield targets.
- Arbitrary reader error identity, target globals, omitted and embedded-NUL
  names, parser diagnostics and stack/status placement.
- Later resumption of fresh and yielded computations, collection during readers,
  loading through a waiting parent and nested readers on another target.
- Fresh-target yield rejection and three permitted initial reader yields whose
  later reader invocation fails, preserving the resulting status and error.

The loader probe matches all 2,474 stdout bytes. Its Rust process balances
2,998 dispatch returns, destroys all 48 C states and closes its native library
reference under `panic=abort`.

The [initial exact comparison](2026-10-28-rust-lua-thread-loads-focused-before.json)
retains two partial-yield stack-base mismatches. The
[first reader-yield audit](2026-10-28-rust-lua-thread-loads-reader-yields-before.json)
retains three status/reader-count differences. Subsequent
[first](2026-10-28-rust-lua-thread-loads-reader-yields-first.json) and
[counter](2026-10-28-rust-lua-thread-loads-reader-yields-count-before.json)
reports preserve intermediate aborts: the new window missed a C-call increment
and attempted to restore a loader top relative to a yielded base when the target
had no call frame. The counter and base restoration fixes close those cases.
The [final release audit](2026-10-28-rust-lua-thread-loads-reader-yields-current.json)
matches all four inputs with normal process exits.

Seven public-header integration groups exercise the default adapter. Two new
portable groups also check validation before reader entry, full-stack rejection,
same-thread loading, shared instruction limits through a nested resumed child,
sticky aborts, context restoration and subsequent VM reuse. The
[final platform validation](2026-10-28-rust-lua-thread-loads-validation.json)
passes 768 tests on Windows x64 and 769 on each Linux target, strict Clippy,
formatting, all-targets builds with default features disabled and the empty
normal/build dependency tree.

The [first platform run](2026-10-28-rust-lua-thread-loads-validation-first.json)
retains the reader-yield `coroutine.status` failure and resulting test-mutex
poisoning. Its original logs remain preserved. A subsequent
[platform run](2026-10-28-rust-lua-thread-loads-validation-registration-before.json)
passes after the status fix. Two boundary setup failures are retained separately:
the new C fixture initially used three public-header macros absent from the
development's minimal header, and the
[next boundary run](2026-10-28-rust-lua-thread-loads-boundary-current-registration-before.json)
found the example had not registered the new helper. The fixture now calls the
underlying public functions and the example registers that helper. Neither setup
failure is counted as a matching runtime observation.

All [5,507 argument cases](2026-10-28-rust-lua-thread-loads-stdlib.json) match.
Unchanged `main.lua` and `all.lua` pass in both
[native-enabled i686 SSE2 CLIs](2026-10-28-rust-lua-thread-loads-i686-suite.json);
T/testC remains absent. The
[Windows independent probes](2026-10-28-rust-lua-thread-loads-windows-suite.json)
retain 17/18 passes in all four modes, with the shared `files.lua:59` rename
failure and failing strict exit. The earlier 72-call and 51-API-yield corpora
still match. The [evidence manifest](2026-10-28-rust-lua-thread-loads-evidence.json)
verifies current source, report, artifact, binary and log hashes and balances
729,604 dispatch returns, 6,027 destroyed C states and 34 closed library handles
across all 24 Rust boundary runs.

Inactive `luaL_loadfile`, arbitrary reader continuations, direct inactive
metamethod/unprotected calls, the five missing lifecycle/allocator declarations,
complete panic/error routing, allocator/OOM windows, private parser mutation,
foreign payload/FILE pointers and broader debug/GC behavior remain open. The
earlier lowered-C-call-count protected-yield mismatch and unrooted search-name
reference mismatch remain recorded. Core declaration coverage stays 75/80.
Full standalone parity is not proven; performance work remains deferred.
