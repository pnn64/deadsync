# Package routines' native stacks

`require`, `module`, `package.seeall` and the preload/Lua/C-path searchers now
present their library C stacks to return hooks. The slots include retained
arguments, `_LOADED`/loader/preload tables, nil and false lookups, accumulated
search messages, imported values, sentinels, module copies, translated names,
paths and successful Lua-file loader functions. Writes to the returned suffix
change the pending results.

`require` now maintains an actual contiguous C window while it awaits callbacks.
Its captured name and numeric loader cursor live outside that window. Cache
metamethods and loader/module calls can inspect and mutate the retained slots.
Consuming a non-nil module result pops it before the final cache lookup, releasing
its collector root. Nil results remain below the subsequent lookup; the default
true-result path retains its sentinel and temporary boolean copies.

`seeall` keeps its original metatable and the pending globals value on its stack
while a metatable's `__newindex` runs, even if that callback replaces the target's
metatable. Module initialization pops the `_NAME` lookup before options run.
Argument moves clear their original slots and relocate native error-function
indexes. Replacing an already consumed module option can therefore release its
closure and captured userdata before the next option collects.

Preload lookup, module field callbacks and search-path field callbacks expose
their real temporary registers. The translated name, root prefix and path slots
remain separately writable. Captured search names and path cursors continue to
use the original values, matching the C routine's local variables.

The [source-built before comparison](2026-10-27-rust-lua-package-stacks-before.json)
retains 126 mismatches among 126 package observations. The
[expanded before comparison](2026-10-27-rust-lua-package-stacks-extended-before.json)
uses a retained runner with the same digest as that source-built baseline. Its
later fixture adds live callbacks and mutations; the old Rust runner aborts, and
that failed/incomplete observation remains recorded. Its fixture hashes do not
claim a rebuild of the old runner from modified Rust sources.

The [current source-built C comparison](2026-10-27-rust-lua-package-stacks-boundary-current.json)
builds the same module against official Lua 5.1.5 and the Rust boundary. Each
scenario runs without writes, with a first-slot write and with a last-slot write.
It checks 135 package observations for ordinary functions and again for copied
function bodies, plus live cache/loader/module/preload/search callbacks and three
collector lifetime regressions. Earlier base/string, table/debug/I/O/OS, hook,
context, parser-root, coroutine and C API probes remain in the comparison. The
[evidence manifest](2026-10-27-rust-lua-package-stacks-evidence.json) binds canonical
reports to their digests and source hashes.

All 270 package observations match. Both package modes match all 28,869 stdout
bytes, including the live traces and lifetime markers. Their 17,718 dispatch/drop
guards balance under `panic=abort`, and all 274 states are destroyed. All 678
copied-body observations across the three stack fixtures also match. The manifest
verifies all eighteen Rust runs' dispatch and state counters.

Seven added portable differential groups exercise package return windows,
result aliases, live import mutations, consumed result collection, field callback
temporaries, captured search names, popped `_NAME` values, replaced option
captures, and metatable replacement during `seeall`. The
[platform validation](2026-10-27-rust-lua-package-stacks-validation.json) records
Windows, Linux x64 and i686 SSE2 tests, strict Clippy, formatting and the empty
normal/build dependency tree with unchanged source hashes.
There are 749 passing tests on Windows and 750 on each Linux target.

The [i686 SSE2 stock suite](2026-10-27-rust-lua-package-stacks-i686-suite.json)
runs unchanged `main.lua` and `all.lua` through both actual CLIs, with ELF/chunk
ABI checks. Stock mode omits T/testC and native suite libraries. The
[Windows probes](2026-10-27-rust-lua-package-stacks-windows-suite.json) retain the
shared `files.lua:59` open-file rename failure in reference, direct Rust and both
cache modes.
Both stock entry points pass in both i686 CLIs; Windows retains 17/18 independent
passes in all four modes, and its strict runner still exits unsuccessfully.

This fixes native package stack behavior without enabling production native
loading. `package.loadlib` and existing native-library paths still report the
recorded unsupported-loader result. The fourteen native-loading argument-audit
failures, five missing lifecycle/allocator C declarations, complete inactive-state
execution, broader error-time/native file/resource/OOM semantics, parser-internal
mutation and the retained lowered-call-count yield mismatch remain open. Full
standalone Lua 5.1 parity remains the goal; performance tuning stays deferred.
