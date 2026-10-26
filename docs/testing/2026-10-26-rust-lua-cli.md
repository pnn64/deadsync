# Lua 5.1 standalone entry validation

Host chunk arguments and calls into live Lua functions, builtins and callable
objects now share the existing register/coroutine driver. Handles are checked
before mutation. Immediate builtin results become result-register roots, and
dead argument copies are discarded. Errors and instruction aborts use the same
cleanup as ordinary chunk execution. Empty-argument execute retains its existing
entry path. Raw table construction uses existing write barriers; global writes
honor __newindex. Direct host file loading uses the VM's streams, shebang/binary
handling and verifier independently of a replaced global loadfile. It suppresses
an artificial Lua loadfile hook invocation while preserving installed hook state.

The CLI supports chained attached/separate -e and -l actions, script positional
arguments, negative arg indices, --, explicit -, implicit piped stdin and
LUA_INIT code/files. Initialization runs before argument validation. Argument
values remain rooted through global callbacks and loading; their temporary table
root is removed before execution so a replaced arg table can be collected.
Windows stderr uses native text newlines and C-string truncation.

All **407 crate tests** pass: 43 unit tests, 363 integration tests and one doc
example. Nine new host groups cover binary/nil arguments, reusable closures and
builtins, raw/callable tables, foreign handles, error recovery, immediate-result
roots, replaced loaders, imported fixed/old-vararg parameters, instruction aborts
and global assignment callbacks. Formatting, strict crate Clippy, whitespace
checks and the normal/build dependency tree pass. The tree contains only the
independent crate; no production dependency, C Lua linkage or unsafe code is added.

All [140 process probes](2026-10-26-rust-lua-cli-process.json) still match complete exit status,
stdout/stderr and recorded file bytes across the C oracle, Rust runner and CLI.
Those API runners now name their root @probe.lua and the CLI receives a relative
probe.lua, matching its real file-loader source name. Ambient LUA_INIT is cleared
for this comparison; dedicated standalone cases test initialization separately.

The [official file report](2026-10-26-rust-lua-cli-suite.json) adds unchanged attrib.lua and
verybig.lua to the existing sixteen checksum-pinned files. Both engines pass
**17/18**; both retain the Windows files.lua:59 open-input rename failure. The
runner keeps _U=true, 100-million-instruction budgets and 60-second deadlines.
The command exits unsuccessfully. These isolated files are not all.lua or full
testC coverage. api.lua/code.lua return early without the absent internal testC
library; they are not counted as new coverage. Native-library branches requiring
external test libraries remain unexercised. Combined-suite, main.lua and the
memory-heavy big.lua resource contract still require work.

The new [standalone CLI audit](2026-10-26-rust-lua-cli-audit.json) matches **32/43** complete
process observations against an unmodified official executable. All failed rows
are retained, and the command exits unsuccessfully:

| Remaining difference | Retained cases |
| --- | --- |
| Imported-function uncaught-error location and traceback | binary_script_error |
| Outer C main-frame return hook | init_hooks |
| Default/numeric/custom traceback handling | runtime_error, numeric_error, custom_traceback |
| Usage text and option diagnostics | missing_e, missing_l, unknown_option, invalid_version_tail |
| Project version banner | version; DeadLua identifies itself rather than using the official Lua release banner |
| Interactive execution | interactive; -i is not implemented |

Successful standalone cases include command/module ordering, startup strings/files,
file/stdin argument tables, quoting, real dash/option-named files, source names,
headers/CRLF, native chunks with parameters/varargs, loader replacement, syntax/
missing-file/non-string errors and argument-table collection. This is a functional
entry milestone, not a complete CLI or standalone-engine compatibility claim.

The CLI oracle comes from the [official Lua source archive](https://www.lua.org/ftp/),
SHA-256 2640fc56a795f29d28ef15e13c34a47e223960b0240e8cb0a82d9b0738695333.
Its [standalone implementation](https://www.lua.org/source/5.1/lua.c.html) defines
the option/initialization/argument behavior. The developer builder compiles it
unchanged with MSVC /O2 /MT on Windows x86-64. Source and binary hashes and the
command are retained in the audit. Other process/benchmark oracles still use the
vendored development-only Lua 5.1.5 binding. The argv/path claim is ASCII Windows;
other platforms, encodings and console/signal behavior require validation.

```sh
cargo test -p deadlib-lua
cargo fmt -p deadlib-lua --check
cargo clippy -p deadlib-lua --all-targets --no-deps -- -D warnings
python scripts/check_rust_lua_process.py --output docs/testing/2026-10-26-rust-lua-cli-process.json
python scripts/check_rust_lua_suite.py --output docs/testing/2026-10-26-rust-lua-cli-suite.json
python scripts/check_rust_lua_cli.py --output docs/testing/2026-10-26-rust-lua-cli-audit.json
```

All final report source hashes match the tree. Two
[measurement runs](../performance/2026-10-26-rust-lua-cli.md) cover 65 runtime and 22 frontend
workloads. Full standalone compatibility, register/resource limits, broader
collector/debug/diagnostic behavior, persistent caching, Rust callback registration,
native modules, later dialects, optional JIT and integrations remain unfinished.
