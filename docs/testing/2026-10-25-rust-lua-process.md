# Lua process validation

`os.execute` supplies Lua 5.1 numeric shell statuses; `os.exit` converts its
optional number to the pinned integer status and terminates without Lua
finalizers or protected-call recovery. Exit flushes active-VM streams and shared
stdout. Windows query calls inspect `COMSPEC`; commands use the CRT's current-
directory/extension/PATH selection and raw command-line convention. Missing
shells return numeric -1. An explicitly empty `COMSPEC` aborts, matching the
pinned CRT's default invalid-parameter handler's observed status and output.

The Windows bridge fixes observable changes introduced by `Command`: batch
files used as `COMSPEC` are rejected while a PE executable renamed `.cmd` runs
as an executable, and the original shell name appears without injected quotes
in `%CMDCMDLINE%`. Argument/path selection remains safe Rust. A small private
module owns UTF-16 buffers, declares Windows ABI records, creates/waits for
processes and closes both returned handles through RAII. Compile-time assertions
check the measured x64 record layouts. `unsafe_code` is denied crate-wide and
allowed only in that module; no Lua value, VM storage pointer or Rust callback
crosses the system boundary. The measured allocation reduction is documented
in the [performance report](../performance/2026-10-25-rust-lua-process.md).

The ABI contract follows [CreateProcessW](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-createprocessw)
and [STARTUPINFOW](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/ns-processthreadsapi-startupinfow).
The distinction between availability queries and command execution, and the
caller's responsibility to flush streams, follows the
[MSVC system reference](https://learn.microsoft.com/en-us/cpp/c-runtime-library/reference/system-wsystem?view=msvc-170)
and installed SDK 10.0.26100.0 CRT observations. No C runtime shell function is
used by the Rust engine.

Piped stdout now shares a process-wide safe-Rust logical-byte buffer across
`print` and standard-output userdata. Windows newline translation happens when
emitting bytes. Default blocks, explicit full/line/no buffering, exactly full
buffers and first-write bulk output retain the tested CRT ordering. Thus a child
can print before a buffered earlier Lua line. `os.execute` does not flush named
files or stdout. Explicit flush, process exit and VM shutdown emit pending
stdout. Collecting its Lua userdata leaves the process stdout resource intact.

All **318 tests** pass: 304 integration groups, thirteen unit groups and one
documentation example. The new integration group compares complete process-
argument errors, aliases, method diagnostics and native environments. Existing
I/O, GC, callbacks, hook, coroutine, compiler and resource tests pass. Formatting,
strict Clippy and whitespace checks pass. The normal/build dependency tree still
contains only `deadlib-lua`; production consumers remain on the legacy engine.

Separately, [74 isolated process cases](2026-10-25-rust-lua-process-probes.json) pass across vendored
official Lua 5.1.5, the Rust example runner and the standalone CLI. All three use
the same controlled chunk display name. The runner compares full exit statuses,
stdout/stderr bytes and two generated file contents, with a ten-second deadline
per process. Windows x86-64 MSVC, static CRT, C locale and ASCII paths/environment
are the pinned contract.

| Cases | Coverage |
| --- | --- |
| Shell statuses/text | Nil queries, empty commands, raw signed statuses, metacharacters, quotes, newlines and NUL truncation. |
| Exit | Default/nil, fractions, signed/large/nonfinite values and numeric strings; no return to Lua. |
| Control flow | `pcall`, `xpcall`, coroutines and hooks cannot recover from process exit; userdata finalizers do not run. |
| Files/stdio | Buffered and already closed files, stdout/stderr, named-file non-flushing, default stdout boundaries and tiny full/line/no buffers. |
| Shell selection | Missing/empty/relative/directory/invalid/batch/renamed-PE `COMSPEC`, extension search, missing PATH, quoted paths containing semicolons and empty PATH components. |
| Metadata/lifetimes | Exact shell command-line bytes for direct/relative/fallback cases; stdout userdata collection before execute/exit. |

The source archive's safe implementation was rebuilt in isolation and compared
on 73 of these cases; 57 matched and sixteen retained counterexamples. Its
launcher adds quotes, executes batch files and flushes stdout too early. The
extra historical comparison omits `comspec_binary_cmd` because that launcher
would interpret PE binary contents as shell commands. The final Rust runner,
CLI and C reference still execute and pass that case; none of their assertions
are omitted or normalized. The [rebuild metadata](2026-10-25-rust-lua-process-std-rebuild.json)
records commands and archive/executable hashes. Historical durations in this
probe report are not performance measurements.

```sh
cargo test -p deadlib-lua
cargo fmt -p deadlib-lua -- --check
cargo clippy -p deadlib-lua --all-targets --no-deps -- -D warnings
cargo tree -p deadlib-lua --edges normal,build
python scripts/check_rust_lua_process.py --output docs/testing/2026-10-25-rust-lua-process-probes.json
python scripts/check_rust_lua_suite.py --output docs/testing/2026-10-25-rust-lua-process-suite.json
```

The [official suite report](2026-10-25-rust-lua-process-suite.json) retains sixteen unchanged,
checksum-pinned files, `_U=true`, a 100-million-instruction budget and sixty-second
file deadlines. Rust still passes eleven files; fifteen C-reference files pass.
Rust stops at `calls.lua:243` (`string.dump`), `literals.lua:161`,
`strings.lua:150` and `files.lua:22` (`os.setlocale`), and `sort.lua:25`
(`os.clock`). The C reference retains its `files.lua:59` open-input rename failure.
That command still exits unsuccessfully. These are isolated file probes, not
complete `all.lua` or C API coverage.

Terminal behavior, unusual buffer sizes, code pages, complex device/permission
errors and long/UNC shell paths need broader checks. POSIX code preserves native
wait status, but POSIX signal handling and reference parity are unverified.
Exit flushes current-VM streams and shared stdout; named streams owned by other
VMs and external host buffers are outside the verified exit-resource scope.
Shutdown flush timing with several embedded VMs also needs further work.
Process-global stdout storage is outside `Vm::heap_stats` estimates. The C
reference's fatal exception payload/debugger behavior is not compared beyond
process status and output.

Clock/calendar/locale/rename support, binary chunks, native modules, incremental
GC, broader numeric/resource/debug parity, caches, later dialects and gameplay
integrations remain unfinished. This milestone does not complete standalone Lua.
