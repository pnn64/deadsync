# Lua 5.1 console validation

The CLI accepts -i after commands/scripts and enters the console automatically
when stdin is a terminal and no script, -e or -v suppresses it. Console entries
share globals and the parked standalone main frame. =expression returns and
prints values; quoted EOF syntax diagnostics request continuation lines. Incomplete
input at EOF is discarded. The default reference's 511-byte fgets fragments,
embedded NUL truncation and Windows CRLF/control-EOF handling are retained.

Input uses the same original-stdin stream and buffering as Lua I/O, independently
of reassigned io.stdin and io.input. Prompt lookup honors _PROMPT/_PROMPT2,
numeric conversion and global __index callbacks. Prompts use shared logical stdout
and flush before reading; replacement Lua output defaults do not redirect them.
Source/continuation/result buffers are reused; string/default prompts are borrowed.

Returned values stay rooted through print lookup callbacks, including a full
collection. Printing uses a handler-free protected entry without an extra visible
Lua/C pcall frame or post-error full collection. Chunk failures still use the
standalone message handler and collection. Print errors are reported and the next
entry continues; unprotected prompt/print lookups and escaping finalizer errors
abort the console without a program-name prefix. get_global provides host lookup
with __index; global remains raw. standalone_active reports whether an outer
context survived a failure.

Usage diagnostics now use argv[0], include -i and preserve the reference's output
shape. Require lookup honors global callbacks. Implicit piped-stdin failures are
reported while preserving the reference's successful process status; an explicit
dash script or an error that unwinds pmain still fails.

All **415 tests** pass: 43 unit, 371 integration and one documentation example.
The added host group covers global lookup callbacks, old result ownership across
collection, embedded/standalone result slots and recovery after lookup errors.
Formatting, strict all-target crate Clippy, whitespace checks and the normal/build
dependency tree pass. No production dependency, unsafe code or C linkage is added.

The [release CLI audit](2026-10-26-rust-lua-console-audit.json) retains **72/110 raw matches**. All
**38 raw differences are the intentional version banner**, including -i, which
implies -v. A separate comparison removes at most one exact known leading banner
from each stderr; **110/110 then match** exit status, remaining stderr, stdout and
file bytes. Raw bytes and both banner strings remain in the report. The strict
audit command still exits 1; this is not a full-byte compatibility pass.

Forty-four added probes cover console expressions/results, globals, continued and
unfinished chunks, syntax/runtime/nil/object errors, custom handlers, dynamic
prompts, callable/replaced/failing print, live result/closure roots during lookup
GC, reader protection, shared input after initialization, scripts/arguments,
fragment limits, binary messages, hooks, escaping finalizers, usage and implicit
versus explicit stdin status. Each case has a fresh working directory and captures
all resulting files. The unmodified official Lua 5.1.5 oracle uses the pinned
archive SHA-256 2640fc56a795f29d28ef15e13c34a47e223960b0240e8cb0a82d9b0738695333,
MSVC /O2 /MT and default console configuration without readline. See the official
[standalone source](https://www.lua.org/source/5.1/lua.c.html).

All [140 process probes](2026-10-26-rust-lua-console-process.json) still pass. The
[official isolated files](2026-10-26-rust-lua-console-suite.json) remain checksum-identical: **17/18
pass in both engines** with _U=true, 100-million-instruction limits and 60-second
deadlines. Both retain files.lua:59's Windows open-input rename failure. That
command exits 1 and does not establish all.lua, internal testC or full suite parity.

[Live Windows terminal observations](2026-10-26-rust-lua-console-terminal.json) exercise the final
release executable and C oracle without -i: automatic entry, =6*7 -> 42, a
continued block -> 17, io.read consuming the next terminal line and Ctrl-Z/Enter
completion with status 0. These are manually inspected ConPTY observations,
not automated exact terminal-mode/echo comparisons; the raw control sequences and
echo differences are retained. No SIGINT equivalence is claimed.

```sh
cargo test -p deadlib-lua
cargo fmt -p deadlib-lua --check
cargo clippy -p deadlib-lua --all-targets --no-deps -- -D warnings
python scripts/check_rust_lua_cli.py --output docs/testing/2026-10-26-rust-lua-console-audit.json
python scripts/check_rust_lua_process.py --output docs/testing/2026-10-26-rust-lua-console-process.json
python scripts/check_rust_lua_suite.py --output docs/testing/2026-10-26-rust-lua-console-suite.json
```

All recorded final source hashes match the tree. [Measurements](../performance/2026-10-26-rust-lua-console.md)
include two complete-process and two engine runs. Terminal signals/modes, broader
debug/C-scratch-stack and collector timing, resource bounds, native modules, later
dialects, caches, Rust callbacks, JIT and song/noteskin integrations remain open.
The full standalone/engine objective is unfinished.
