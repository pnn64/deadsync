# Native file loading with shared streams

`NativeStack::load_file` and development `luaL_loadfile` compile named files or
the actual standard input stream. They use the engine's existing streams,
source/binary verifier and executable-header prelude. The mutable global
`loadfile` function and default `io.input` do not redirect the native API.
Named streams close after success, syntax errors, collector failures and fatal
instruction limits. Standard input remains open and retains shared lookahead.

The temporary `@filename` or `=stdin` value stays in the native caller's window
during parsing. Filename construction checks collection before opening the
file, outside parser protection; file-error formatting also checks collection
after closure. The parser inherits enclosing error handlers for collector
runtime errors, while syntax failures bypass those handlers. Open, reopen and
read failures use Lua 5.1's auxiliary file status **6**. Read errors override the
parser result. Compiler functions take the current coroutine globals and native
binary capture cells begin nil.

The extracted `FileReader::prepare` keeps the existing Lua loader's header and
binary-reopen path shared with the native API. Named binary files reopen as raw
bytes and scan for the signature after executable headers. Text headers supply
a synthetic newline. Active native parser jobs trace borrowed file handles;
the surrounding native scope retains responsibility for closing owned streams.
This preserves an outer load through nested collection, finalizers, child
coroutines and recursive native file loads.

The Windows/MSVC shared C fixture adds sixteen named-file cases: empty/text
chunks, long text/header inputs, CRLF, executable headers, UTF-8 BOM rejection,
text-mode EOF, native binary input and truncated chunks. It checks source names,
NUL path prefixes, missing/empty paths, syntax/file handler isolation, coroutine
environments and closure before file removal. A shared stdin check first reads
a number, loads the remainder through `luaL_loadfile(NULL)`, verifies EOF and
repeats an empty load while a different `io.input` remains selected. The exact
input bytes are retained in the report. The empty-path comparison exposed a
Windows discrepancy: MSVC fopen returns EINVAL, so the shared opener now matches
that diagnostic rather than returning ENOENT.

Eight Rust groups run on Windows, with a ninth Linux/glibc group for directory
read errors. They execute text and binary results, inspect source names and
coroutine environments, compare empty-path diagnostics directly with official
Lua, inspect the actual caller during collection, load recursively in a child
coroutine, and verify file closure after runtime and quota failures. The Linux
case verifies that a read error takes precedence even after a protected
finalizer error has invoked its enclosing handler. A separate pre-open check
verifies that collector errors escape unprotected before a filename slot or
open handle exists. Tests use explicit collector controls; automatic collector
timing and work units remain different from PUC Lua.

Fresh builds pass 662 tests on Windows x64 and 663 on each of Linux x64 and
Linux i686 SSE2 (77 unit and 4 documentation tests, plus 581/582 integration
tests). Rustfmt, strict Clippy and the zero normal/build dependency tree pass.
The expanded C fixture emits 44,694 identical stdout bytes; 79,841 Rust
operation/drop guards balance and all 77 native states are destroyed, including
76 collected before VM shutdown. The executable uses panic=abort and C-only
error jumps. Imported-name audits match eighteen chunks in reference, direct
Rust, cache-store and fresh-process cache-hit modes on each platform. Unchanged
stock `main.lua` and `all.lua` pass in both CLIs on Linux i686 SSE2. Internal
T/testC and suite native modules remain absent.

Raw [C-boundary](2026-10-27-rust-lua-native-file-boundary.json),
[Windows](2026-10-27-rust-lua-native-file-windows.json),
[Linux](2026-10-27-rust-lua-native-file-linux.json),
[i686](2026-10-27-rust-lua-native-file-i686.json) and
[stock-suite](2026-10-27-rust-lua-native-file-suite.json) reports retain commands
and observations. [Validation metadata](2026-10-27-rust-lua-native-file-validation.json)
binds exact source snapshots, workspace inputs, logs, ABI headers and private
executables. Every final target starts absent and builds from source.

The separate C-reader scratch probe still differs: PUC exposes parser roots
through `lua_gettop` (1,3), while this engine exposes the caller window (1,1).
Both executions complete and their unequal outputs remain in the report.
Allocator/OOM statuses, remaining C APIs, production native loading, complete
collector/resource behavior, full standalone Lua 5.1 parity, later dialects and
integrations remain unfinished. Optimization stays frozen and the legacy
engine remains the production runtime.
