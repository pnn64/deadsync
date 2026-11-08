# Lua clock and rename validation

`os.clock` and `os.rename` now use the pinned native platform semantics.
The official `sort.lua` file passes unchanged, increasing Rust's result from
eleven to **twelve of sixteen** isolated official probes. The library remains
independent of C Lua, with zero normal/build dependencies. Production consumers
continue to use the legacy crate.

MSVC `clock` reports wall time since CRT initialization in signed 32-bit
milliseconds. The wrapper calls that native clock and divides by 1000, preserving
the shared process epoch, resolution and native overflow/failure return convention.
A timer started at VM creation would not match. The declared Linux/Android/Apple
wrappers use their native clock types and million-tick scale; other targets
currently return an unsupported-clock error. Those non-Windows declarations have
compile coverage only. Native unavailable/overflow clock states and processes
running beyond the MSVC limit were not forced in this corpus.
See [Microsoft's clock contract](https://learn.microsoft.com/en-us/cpp/c-runtime-library/reference/clock?view=msvc-170).

Windows rename calls `MoveFileExW` with `COPY_ALLOWED`, matching the installed
SDK 10.0.26100.0 CRT's `_wrename`. `REPLACE_EXISTING` is absent. Same-volume moves
therefore retain native atomic destination handling, with no check-before-move
window. Cross-volume copy behavior is requested through the same flag, but no
cross-volume test was performed. POSIX uses `std::fs::rename`, including native
replacement rules. The Windows ABI follows
[MoveFileExW](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-movefileexw).
Arguments validate the source before the destination. Results preserve one true
on success or nil, complete source-path/error text and numeric errno on failure.
Strings stop at their first NUL; numeric filenames coerce before the OS call.

The existing Windows bridge now resides under `os/platform`, retaining the
process-launch implementation. Scoped unsafe code is limited to platform ABI
declarations/calls; compiler, heap, bytecode and interpreter storage remain safe
Rust. Native path encoding uses initialized stack arrays and owned spill buffers.
No Lua pointers or callbacks cross these boundaries. Windows error mapping now
also covers missing-drive/network/path/oversized-name errors, executable-format
errors, cross-device moves and nonempty directories. Broader errno/device cases
and Windows code-page parity remain unfinished; the byte-exact path contract
continues to cover ASCII with UTF-8 conversion in the engine.

All **327 tests** pass: 313 integration groups, thirteen unit groups and one
documentation example. Nine new groups cover:

| Group | Evidence |
| --- | --- |
| Clock API | One numeric result, ignored arguments and method self, native environments, coroutine/module/debug identity predicates. |
| Clock values | 512 Rust readings bracketed by official readings in the same process; stable millisecond brackets compare exact number bits. |
| Clock epoch | An idle wait before creating a VM on another OS thread retains the native wall-time/process epoch. |
| Rename arguments | Full diagnostic bytes for missing, nil and wrong types, first-before-second validation, aliases and method self. |
| Rename data/results | Success arity, data bytes, repeated/self renames, embedded NULs, invalid/missing components and oversized names. |
| Existing/open files | Existing destination contents survive Windows failures; open-handle outcomes match the reference. |
| Directories/case | Directory moves and case-only file changes retain observable results and file bytes. |
| Long paths | Valid ASCII names exceeding the stack-storage bound retain their complete names and contents. |
| Atomic destination handling | 64 barrier-controlled rename/create races per engine, 128 total; exactly one wins and neither overwrites the other's contents. |

Each filesystem regression uses a freshly created owned fixture directory.
Existing I/O, protected-call, GC, hook, coroutine, parser/compiler and resource
tests pass. Formatting, strict Clippy and whitespace checks pass. The normal/build
dependency tree contains only `deadlib-lua`.

The [76 isolated process cases](2026-10-25-rust-lua-clock-rename-process.json) pass across official Lua
5.1.5, the Rust runner and standalone CLI. They retain the earlier shell, exit,
stdio and file checks and add clock-context predicates and actual numeric
filename renames in an isolated working directory. Full exit statuses,
stdout/stderr bytes and recorded file contents are compared. Official files and
assertions remain unchanged; clock checks use explicit bracketing and context
predicates.

Compile-only checks also pass for x86-64 GNU/Linux, ARM64 macOS, x86 Windows MSVC
and ARM64 Windows MSVC; see the [log](2026-10-25-rust-lua-clock-rename-cross-check.log). These check Rust
code paths and types, and do not establish linking, ABI/runtime or oracle parity
on those targets.

```sh
cargo test -p deadlib-lua
cargo fmt -p deadlib-lua -- --check
cargo clippy -p deadlib-lua --all-targets --no-deps -- -D warnings
cargo tree -p deadlib-lua --edges normal,build
python scripts/check_rust_lua_process.py --output docs/testing/2026-10-25-rust-lua-clock-rename-process.json
python scripts/check_rust_lua_suite.py --output docs/testing/2026-10-25-rust-lua-clock-rename-suite.json
```

The [official report](2026-10-25-rust-lua-clock-rename-suite.json) retains sixteen unchanged checksum-pinned
files, `_U=true`, a 100-million-instruction budget and sixty-second file deadlines.
Rust passes `constructs.lua`, `locals.lua`, `nextvar.lua`, `vararg.lua`, `pm.lua`,
`events.lua`, `gc.lua`, `closure.lua`, `db.lua`, `math.lua`, `errors.lua` and
`sort.lua`. Fifteen C-reference files pass. Rust's remaining stops are
`calls.lua:243` (binary dumps) and `literals.lua:161`, `strings.lua:150`,
`files.lua:22` (locale support). The retained C `files.lua:59` open-input rename
failure remains. The suite command still exits unsuccessfully; these probes
are not a complete `all.lua` or C API run. The runtime contract remains Windows
x86-64 MSVC, static CRT, binary64 numbers and the C locale.

[Measurements](../performance/2026-10-25-rust-lua-clock-rename.md) retain two runs for 45 runtime and
twenty-two frontend workloads. Calendar/time-zone and locale functions, binary
chunks, native modules, incremental GC, broader resource/debug/numeric behavior,
caches, later dialects and integrations remain unfinished. Stdout/host shutdown,
device/encoding, long-path and resource-exhaustion behavior need broader checks.
This milestone does not complete arbitrary standalone Lua 5.1.
