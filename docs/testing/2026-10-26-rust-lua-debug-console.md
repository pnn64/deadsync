# Lua 5.1 debug command console validation

`debug.debug` executes commands in the current thread while retaining the
suspended caller stack. Commands can inspect and mutate its locals, preserve
shared globals/object identity and enter nested debug sessions. The loop uses
the existing native continuation driver with a protected boundary that suppresses
outer xpcall/standalone handlers, including errors inside reader-based loads.
Runtime and syntax failures report to stderr and continue. No extra visible
pcall frame or recursive Rust execution driver is introduced.

Input shares original stdin and Lua I/O buffering, independently of io.input or
reassigned io.stdin. Prompts and diagnostics use physical stderr, independently
of reassigned io.stderr. Input consumes at most 249 bytes through newline, keeps
C-string truncation at NUL, and exits only on exact `cont\n` or EOF. Returned
command values are discarded; `=expression` is parsed as ordinary source.
Compiled commands use the thread environment and `(debug command)` source name.
Successful or failed commands clear their native argument roots and stack slots;
an immediate cont/EOF retains the original arguments for return hooks. Hook
inspection includes the native hook-table temporary.

All **449 tests** pass: 43 unit, 405 integration and one documentation example.
Eight new process test groups cover caller mutation, recovery and inherited
reader handlers, nested sessions, shared input, fragment/NUL/continuation rules,
argument root cleanup, hook locals, environments and coroutine yield boundaries.
The existing all-builtins collector test verifies that the new native handle can
be pinned across marking/sweeping. Strict all-target Clippy, formatting,
whitespace checks and the normal/build dependency tree pass. No production
dependency, unsafe code or C linkage is added.

The [release CLI audit](2026-10-26-rust-lua-debug-console-audit.json) adds eighteen independent cases,
including parser-depth limits, returned values and embedded-NUL errors. All
eighteen match complete exit status, stdout, stderr and file bytes. The full
audit has **142/180 raw matches** and **180/180 after at most one exact known
leading version banner is removed**. All thirty-eight raw differences remain
intentional banner differences; raw data is retained and the command exits 1.

All [140 process probes](2026-10-26-rust-lua-debug-console-process.json) pass. The unchanged eighteen
[official files](2026-10-26-rust-lua-debug-console-suite.json) retain their previous source checksums:
**17/18 pass in both engines** with _U=true, 100-million-instruction limits and
60-second deadlines. Both retain files.lua:59's Windows open-input rename
failure. That command exits 1; this is not all.lua/internal testC or a complete
standalone compatibility pass.

The unmodified official Lua 5.1.5 oracle is checksum-pinned to archive SHA-256
2640fc56a795f29d28ef15e13c34a47e223960b0240e8cb0a82d9b0738695333 and
built with MSVC /O2 /MT, binary64, 64-bit Lua integers and 32-bit native ints.
The behavior was checked against the local pinned [debug library source](https://www.lua.org/source/5.1/ldblib.c.html).
Non-string/non-number error values reach fputs(NULL) in that source; on this
Windows oracle a table error terminates the process. Rust reports `(null)` and
continues. This undefined C path is outside the byte-exact contract and is not
counted as a matching audit case. Terminal/signal/device and broader C scratch
behavior remain open.

```sh
cargo test -p deadlib-lua
cargo fmt -p deadlib-lua --check
cargo clippy -p deadlib-lua --all-targets --no-deps -- -D warnings
python scripts/check_rust_lua_cli.py --output docs/testing/2026-10-26-rust-lua-debug-console-audit.json
python scripts/check_rust_lua_process.py --output docs/testing/2026-10-26-rust-lua-debug-console-process.json
python scripts/check_rust_lua_suite.py --output docs/testing/2026-10-26-rust-lua-debug-console-suite.json
```

Two [pairs of measurement runs](../performance/2026-10-26-rust-lua-debug-console.md) cover ten complete
CLI workloads and the existing 78 runtime/22 frontend cases. All seven final
JSON source-hash records match the tree. Native C modules, full resource/debug
and collector parity, later dialects, caches, Rust callbacks, machine code and
song/noteskin integration remain unfinished. The full objective stays active.
