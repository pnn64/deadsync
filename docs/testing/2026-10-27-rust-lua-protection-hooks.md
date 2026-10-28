# Protection scopes during return hooks

Errors from a completed `pcall`, `xpcall` or loader's return hook now escape that
operation's finished protection scope. The visible function frame remains
available for debug inspection. Active parent scopes still catch errors from
their called function's call/return hooks.

Previously, a successful inner `pcall` could catch its own return-hook error and
return `false, token` as ordinary results. Its outer `pcall` then reported
`true, false, token`; official Lua reports `false, token`. A completed `xpcall`
could also use an expired error handler. A loader called inside the hook could
inherit that handler instead of the active outer handler.

Error-boundary selection now excludes a frame whose return continuation is
pending. Parser error-handler inheritance uses the same scope check. This
preserves the faulting frames and ordinary protection of child calls while
preventing an already completed operation from handling new hook errors.

The [before C comparison](2026-10-27-rust-lua-protection-hooks-before.json)
retains 18 mismatches among 39 scenarios. It uses the prior source-built release
binary, whose digest matches the
[preceding source-built report](2026-10-27-rust-lua-return-stacks-boundary-current.json).
The [before provenance](2026-10-27-rust-lua-protection-hooks-inputs/before-provenance.json)
records that match, an exact snapshot of the old native executor, and unchanged
other production Rust sources. The before runner is explicitly marked prebuilt;
its new fixture-source hashes do not claim that it was rebuilt from those inputs.

The [current source-built comparison](2026-10-27-rust-lua-protection-hooks-boundary-current.json)
matches all 39 scenarios and all 5,184 stdout bytes against official Lua 5.1.5.
The same C module checks successful and failed protected calls, reader/syntax/file
errors, missing files and `dofile`, with direct native hook errors and nested Lua
callbacks. It checks outer `pcall`/`xpcall`, error-object identity, handler counts,
nested protection, inherited parser handlers and collection inside delivery.
All 1,879 dispatch/drop guards balance under `panic=abort`; all 41 states are
destroyed, including 40 collected before shutdown. Earlier boundary, parser-root,
coroutine, call-level, debug, hook, context and return-stack comparisons also pass.

Three new portable groups cover Lua hook errors after protected operations finish,
call/return hook errors while a parent scope is still active, and external native
hook errors after `pcall`, `xpcall` and `load` finish.

The [platform validation](2026-10-27-rust-lua-protection-hooks-validation.json)
records 730 passing tests on Windows and 731 each on Linux x64 and i686 SSE2,
strict Clippy on all three, formatting and an empty normal/build dependency tree.
Sources stay unchanged throughout those runs. The unchanged
[i686 SSE2 stock suite](2026-10-27-rust-lua-protection-hooks-i686-suite.json) passes
`main.lua` and `all.lua` in both actual CLIs, with verified ELF32/chunk ABIs.
Stock suite mode still omits T/testC and native suite libraries. The
[Windows independent probes](2026-10-27-rust-lua-protection-hooks-windows-suite.json)
retain 17/18 passes in reference, direct Rust, cold-cache and cache-hit modes.
The common `files.lua:59` open-file rename failure remains a failure. The
[evidence manifest](2026-10-27-rust-lua-protection-hooks-evidence.json) binds the
canonical reports to their digests and current source hashes.

This fixes protection semantics without adding C declarations. Full Lua 5.1
parity remains unproven: six core C APIs, production native loading, broader
library/error-time stack layouts, resource/OOM behavior, parser-internal mutation
semantics and the retained lowered-call-count yield mismatch remain open.
