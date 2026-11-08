# Table, debug, I/O and OS native stacks

Native return hooks now see the library's actual retained arguments, consumed
values, temporaries and result suffix across the table, debug, I/O and OS cases
in this comparison. Hook writes alias pending results. Moving the complete window
releases old scratch and discarded argument roots, with native error-function
indexes following the move.

The table paths account for insert consuming its value, remove's empty result,
sort's two-slot argument normalization, and traversal's retained key/value slots
on early return. Debug paths preserve function-option strings, duplicated local
values, optional thread prefixes, upvalue consumption, hook controls and completed
traceback windows. `os.time(table, ...)` retains its table beside the result after
discarding extra arguments.

I/O paths preserve file metatables and close environments/functions, implicit
default-file arguments, failed-open userdata, and iterator EOF/close temporaries.
File opening now allocates a genuine closed userdata before attempting the OS
open. A failed-open userdata remains observable to its return hook and is
collectible when its slot is replaced. No placeholder object is synthesized just
for a hook.

The comparison also exposed behavior beyond return stacks:

- `debug.setupvalue(f, n, value, extra)` consumes the actual topmost `extra`, as
  the C API does, rather than always using the third argument.
- `io.lines(nil, extra)` checks the still-nil first slot and raises a file argument
  error. It no longer silently creates an iterator from the default input.
- Implicit `io.lines()` and `io.close()` arguments report a closed default file
  through the ordinary file check, matching the reference's error text.

The original [source-built before comparison](2026-10-27-rust-lua-library-stacks-before.json)
retains 186 mismatches among its 204 observations. The
[expanded before comparison](2026-10-27-rust-lua-library-stacks-extended-before.json)
retains 267 mismatches among 291 observations. The expanded comparison uses a
retained prebuilt runner whose digest matches that original source-built report;
its current fixture hashes do not claim a rebuild from the modified Rust sources.

The [current source-built C comparison](2026-10-27-rust-lua-library-stacks-boundary-current.json)
uses the same module against official Lua 5.1.5 and the Rust boundary. Ninety-seven
scenarios each run with no write, a first-slot write and a last-slot write. These
include cross-thread debug access, cleared/installed hooks on other threads,
null/non-null light userdata metatables, successful and failed opens, file reads
and writes, default files, explicit collection, and iterator EOF/closing. The
fixture compares object categories and stable identities rather than process
pointer addresses; it uses fixed dates and file contents.

All 291 observations and all 42,531 stdout bytes match. All 14,146 Rust
dispatch/drop guards balance under `panic=abort`, and all 293 states are
destroyed, including 292 collected before shutdown. Earlier boundary, parser-root,
coroutine, call-level, debug, hook, context, base/string stack and protection-scope
comparisons also pass. The [evidence manifest](2026-10-27-rust-lua-library-stacks-evidence.json)
binds canonical reports to their digests/current source hashes and records the
expanded baseline's runner provenance.

Four portable differential groups exercise the corresponding Lua debug-hook
windows, extra setupvalue arguments, nil line arguments, closed defaults, and
collection of failed-open userdata after its slot is replaced.

The [platform validation](2026-10-27-rust-lua-library-stacks-validation.json)
records 734 passing tests on Windows and 735 on both Linux x64 and i686 SSE2,
strict Clippy on all three, formatting, and an empty normal/build dependency tree.
Source hashes remain unchanged throughout those runs. The unchanged
[i686 SSE2 stock suite](2026-10-27-rust-lua-library-stacks-i686-suite.json) passes
`main.lua` and `all.lua` in both actual CLIs with verified ELF32/chunk ABIs. Stock
mode still omits T/testC and native suite libraries. The
[Windows independent probes](2026-10-27-rust-lua-library-stacks-windows-suite.json)
retain 17/18 passes in reference, direct Rust, cold-cache and cache-hit modes;
the shared `files.lua:59` open-file rename failure remains a failure.

This advances builtin/C API semantics without completing full Lua 5.1 parity.
Six core C declarations, production native loading, remaining library and
error-time C windows, broader native file/resource/OOM behavior, parser-internal
mutation semantics and the retained lowered-call-count yield mismatch remain
open. Performance tuning remains deferred.
