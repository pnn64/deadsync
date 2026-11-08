# Imported upvalue debug metadata

Native Lua 5.1 chunks retain executable capture cells independently of their
debug-name table. A stripped chunk can have two cells and no names; a partially
stripped chunk can retain only the first name. The Rust VM previously exposed
the padded internal names through `debug.getupvalue` and `debug.setupvalue`.
For a nameless closure retaining values 17 and 29, those calls incorrectly read
the values and changed them to 100 and 200. Official Lua returns no results and
leaves both cells unchanged.

Debug access now uses the original native name count. Actual cells remain
available to execution, shared closures and suspended coroutine frames, and
`debug.getinfo(..., 'u').nups` still reports the actual capture count. Missing
internal names use `?` for operand errors, matching Lua 5.1's diagnostic
fallback. An explicitly retained empty name remains an empty name. Dumping
native functions continues to write only the original retained names.

The standalone audit compiles the unchanged fixture with official Lua 5.1.5,
then rewrites only debug-name strings/counts and optional location metadata.
Code, constants and executable captures remain unchanged. Eighteen chunks
cover zero, one and two retained names; ordinary, empty and embedded-NUL name
strings; and retained versus stripped source/line/local metadata. Every chunk
checks open captures, closed captures, cells in a suspended coroutine,
redumped/reloaded functions and operand errors. Each runs in the official C
CLI, Rust CLI, persistent-cache creation and a fresh-process cache reload.
No Lua bootstrap is injected.

The [before-fix audit](2026-10-27-rust-lua-upvalue-names-before.json) retains
twelve mismatches out of eighteen in each Rust mode. The final
[Windows x64](2026-10-27-rust-lua-upvalue-names-windows.json),
[Linux x64](2026-10-27-rust-lua-upvalue-names-linux.json) and
[Linux i686 SSE2](2026-10-27-rust-lua-upvalue-names-i686.json) audits match all
eighteen chunks in all modes. Cache statuses must be `Stored` and `Hit` in
their respective processes. Source bytes, transformed chunks, executable
hashes, return codes, raw stdout/stderr and completion markers are retained.

| Check | Result |
| --- | --- |
| Native name audit per platform | 18/18 in C, Rust direct, cache store and cache hit |
| Rust regression suite per platform | 632 pass: 75 unit, 553 integration, 4 documentation |
| New unit groups | Partial/stripped debug access and missing-name operand diagnostics |
| Static checks | Rustfmt and strict Clippy; no normal/build dependencies |

The [validation record](2026-10-27-rust-lua-upvalue-names-validation.json)
binds fresh Cargo targets, source snapshots, official source/archive hashes,
workspace inputs, commands, logs and private executable artifacts. The final
checks build their binaries from source. Before and after audits use the same
fixture, transformer and cache runner.

Reproduce the native audit with:

```powershell
python scripts/check_rust_lua_upvalue_names.py --output target/lua-upvalue-names.json
```

The native C API, inactive/cross-thread C stacks, production native loading,
internal T/testC conformance tests and broader language/library/GC/resource
behavior remain unfinished. These finite regressions do not establish full
standalone Lua 5.1 parity. Later dialects, performance tuning and gameplay
integration remain deferred. The legacy engine remains the production runtime.
