# Protected native reader and buffer loading

`NativeStack::load_reader` and `load_buffer` compile source or native Lua 5.1
chunks within an active host callback. The development C boundary adds
`lua_load`, `luaL_loadbuffer` and `luaL_loadstring`. These entry points do not
enable production native modules or establish complete native loader parity.

The loader uses the existing resumable parser, binary decoder and verifier.
A hidden protected continuation preserves the native caller and its live
argument window. Readers can invoke Lua, load recursively, resume coroutines,
collect, and change caller slots. Success pushes the compiled function; failure
restores the entry top and pushes its error. Reader runtime errors inherit an
enclosing error function and preserve arbitrary error identity. Syntax errors
bypass that handler. Failed handlers return status 5. Fatal instruction limits
escape to Rust and leave the VM reusable at its next entry.

The initial reader lookahead preserves Lua 5.1's nonsticky empty-probe behavior.
Names use C-string prefixes, absent names select `?`, and loadstring uses the
source prefix before NUL. Binary closures initialize imported upvalue cells to
nil. The completed closure takes the current thread environment. Parser depth
includes active native calls and nested parser work; the C comparison caught
and corrected an off-by-one syntax budget.

The shared Windows/MSVC C module runs unchanged against official Lua 5.1.5
and the Rust development boundary. Its loader cases cover 710 two-block splits,
one-byte reads, text/binary errors, C-string and chunk-name handling, reader
reentry, coroutine environments, inherited/failed handlers, and nested parser
depth. Both engines consume identical official binary chunks. The complete
shared fixture produces 43,929 identical stdout bytes. Rust dispatch/drop
guards balance at 79,680; all 76 native states are destroyed, including 75
collected before VM destruction. The Rust executable uses panic=abort; C error
jumps occur only after the Rust dispatcher has returned.

A separate probe deliberately records a remaining mismatch. During a text
reader's second invocation, official Lua exposes its private constant table and
prototype on the C stack. Its `lua_gettop` trace is 1,3; this engine exposes only
the caller stack and produces 1,1. Both probe executions must complete; their
unequal raw outputs are retained independently of the matching main fixture.
**Native loader parity remains unproven.** The safe Rust API documents its caller
window, and the development C bridge does not yet emulate those parser roots.

Eight new Rust integration groups cover fragmented input, caller mutation and
collection, inherited errors, recursive loads and empty probes, quota recovery,
imported binary captures, sticky failures and handler statuses, and the host
syntax-depth budget. The complete regression run has 654 tests per platform:
77 unit, 573 integration and 4 documentation tests on Windows x64, Linux x64 and
Linux i686 SSE2. Rustfmt, strict Clippy and the zero normal/build dependency tree
are checked. The imported-name audit compares eighteen chunks in reference,
direct Rust, cache-store and fresh-process cache-hit modes on every platform.
Unchanged stock `main.lua` and `all.lua` pass in both engines on Linux i686 SSE2;
internal T/testC and native suite modules remain absent.

Raw [C-boundary](2026-10-27-rust-lua-native-load-boundary.json),
[Windows](2026-10-27-rust-lua-native-load-windows.json),
[Linux](2026-10-27-rust-lua-native-load-linux.json),
[i686](2026-10-27-rust-lua-native-load-i686.json) and
[stock-suite](2026-10-27-rust-lua-native-load-suite.json) reports retain commands
and observations. [Validation metadata](2026-10-27-rust-lua-native-load-validation.json)
binds the source snapshot, workspace inputs, ABI headers, logs and private
executables. Final targets start absent and all final executables build from
source.

Parser scratch slots, `luaL_loadfile`, allocator/OOM statuses, other C APIs,
production native library ownership/loading, collector resource behavior and
full standalone Lua 5.1 parity remain open. Later dialects and integrations are
unfinished. Optimization and benchmarks remain frozen; the legacy engine stays
in production.
