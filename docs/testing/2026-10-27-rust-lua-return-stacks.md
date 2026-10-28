# Base/string native return stacks and Lua reader roots

Return hooks now inspect and mutate the library's logical Lua 5.1 C stack.
Previously, many builtins exposed scheduler scratch between their arguments and
results. For example, `type(17, tag)` exposed six slots instead of the reference's
three, and `rawset(table, key, value, tag)` exposed six instead of one. The retained
[before comparison](2026-10-27-rust-lua-return-stacks-before.json) reproduces these
differences.

The return path now accounts for retained, trimmed and consumed arguments,
protected metatables, environment-lookup function temporaries, proxy creation,
protected-call status/results, the saved `print` conversion function, string
operations and pattern iterators. Hook-visible result slots alias the pending
return suffix. Moving the live window releases discarded arguments and scratch;
native error-function indexes follow the move. Existing substitution and dump
windows retain their own stack layouts. Lua debug hooks also see their registry
hook-table slot after the native window.

Lua `load` now shares the native reader's incremental parser-root implementation.
Reader callbacks can inspect genuine constants tables and prototype objects;
these retain their collector edges. EOF nils remain on the parser stack, including
the constants table left by a text reader's final nil. Protected failures preserve
the reader/name/reserved slots before nil/error results. Loaded programs retain
their actual parser prototypes.

The [current source-built C comparison](2026-10-27-rust-lua-return-stacks-boundary-current.json)
uses one module against official Lua 5.1.5 and the Rust development boundary. Its
84 base/string/reader/file cases run with no write, a first-slot write and a
last-slot write: 252 observations and all 40,740 stdout bytes match. Collection
runs inside each matching hook. All 11,918 Rust dispatch/drop guards balance under
`panic=abort`; all 254 states are destroyed, including 253 collected before
shutdown. All earlier boundary, parser-root, coroutine, thread-stack, call-level,
debug, hook and context comparisons also pass. The earlier
[extended comparison](2026-10-27-rust-lua-return-stacks-extended.json) retains the
same probe with the sources before the final portable tests were added.

Three new portable differential test groups cover Lua hook slot enumeration and
writes, collection of discarded arguments/replaced results, and incremental Lua
reader tables/prototypes through EOF, errors, collection and return hooks.

The [platform validation](2026-10-27-rust-lua-return-stacks-validation.json)
records 727 passing tests on Windows and 728 on both Linux x64 and i686 SSE2,
strict Clippy on all three, formatting, and an empty normal/build dependency tree.
Sources stay unchanged throughout those runs. The unchanged
[i686 SSE2 stock suite](2026-10-27-rust-lua-return-stacks-i686-suite.json) passes
`main.lua` and `all.lua` in both actual CLIs with verified ELF32/chunk ABIs. Its
stock mode omits T/testC and native suite libraries. The
[Windows independent probes](2026-10-27-rust-lua-return-stacks-windows-suite.json)
retain 17/18 passes in reference, direct Rust, cold-cache and cache-hit modes.
Their shared `files.lua:59` open-file rename failure remains a failure. The
[evidence manifest](2026-10-27-rust-lua-return-stacks-evidence.json) binds the
canonical reports to their digests and current source hashes.

These checks advance the full Lua 5.1 goal without completing it. Core C API
coverage remains 74/80 declarations; production native loading, other library and
error-time C stack layouts, broader resource/OOM behavior, parser-internal mutation
semantics, the retained lowered-call-count yield mismatch and the full standalone
parity gate remain open. Performance tuning remains deferred.
