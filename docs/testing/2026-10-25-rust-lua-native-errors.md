# Lua 5.1 native argument diagnostics

Native argument failures use shared cold formatters for caller names, argument
numbers and details. Method calls exclude `self` from numbering; an invalid
receiver reports the reference's bad-self message. Calls from native/protected
contexts without a recoverable name use `?`. Missing arguments report `no value`
separately from explicit nil. Raw byte field names, option strings and file-error
paths retain their original bytes through error construction and interning.

The formatters use existing frame/call metadata only on failure. Successful calls
keep their direct argument layouts and numeric native IDs. Type checks, domain
checks and option errors remain distinct. Common string/number helpers and base,
math, table, debug/hook, coroutine, reader, proxy, GC and I/O checks use the shared
formatters. Package checks retain their existing equivalent formatter.

Base raw-table operations validate their table before checking later arguments.
Metatable operations validate their arguments before checking protection, with
the debug variant's second-argument order preserved. Environment functions retain
their own validation order and distinguish invalid levels from lost tail frames.
These are observable changes when several arguments are invalid at once.

All 243 tests pass: 229 integration groups, thirteen unit groups and one runnable
documentation example. Seven new groups provide 362 reference observations:

| Corpus | Reference observations |
| --- | ---: |
| Aliases, methods, tail calls, native callers and binary names | 15 |
| Forty functions with seven missing/nil/type configurations | 280 |
| Validation order, options and argument domains | 32 |
| File userdata, read formats and seek/buffering arguments | 12 |
| Environment and debug-stack arguments | 14 |
| Metamethod dispatch, coroutines and hook-mutated arguments | 5 |
| Failed named-file operations and aliased callers | 4 |

The generated corpus compares success/failure and complete error bytes; successful
results are deliberately discarded because this corpus tests diagnostics. It
covers base, math, string, table, debug, coroutine, loader, proxy and I/O entry
points. Explicit cases cover method renumbering, bad receivers, anonymous native
callers, captured native aliases, embedded-NUL/invalid-UTF-8 names, option context,
multiple invalid arguments and a native argument replaced by a call hook.

The file cases use temporary file userdata or exclusively created directories
under `target/lua-native-error-fixtures`. Missing-file and missing-parent checks
leave those owned directories empty, then remove them. Tests never read stdin.
Both engines execute the same named chunk under an explicit Lua `pcall`.
The oracle is vendored official Lua 5.1.5 on Windows x86-64 MSVC, static CRT and
C locale. Formatting, strict Clippy and the zero normal/build dependency gate
pass. Engine code remains safe Rust.

```sh
cargo test -p deadlib-lua
cargo fmt -p deadlib-lua -- --check
cargo clippy -p deadlib-lua --all-targets --no-deps -- -D warnings
cargo tree -p deadlib-lua --edges normal,build
python scripts/check_rust_lua_suite.py --output docs/testing/rust-lua-native-errors-suite.json
```

The [official report](2026-10-25-rust-lua-native-errors-suite.json) retains the
sixteen unchanged probes, pinned archive checksum, `_U=true`, a 100-million-
instruction budget and 60-second per-file deadlines. Rust still passes ten:
`constructs.lua`, `locals.lua`, `nextvar.lua`, `vararg.lua`, `pm.lua`, `events.lua`,
`gc.lua`, `closure.lua`, `db.lua` and `math.lua`. Fifteen reference probes pass.

`errors.lua` now passes the initial syntax, operand-name, native argument and
line-error assertions. It stops when recursive `y()` reaches the engine's Lua
call-stack limit at line 142. That limit currently raises an uncatchable
`ErrorKind::Limit` instead of the reference's catchable stack overflow. Correct
depth, recovery reserve, handlers and overflow diagnostics remain required work.

| Remaining probe | Rust stopping point |
| --- | --- |
| `calls.lua` | `string.dump`, line 243 |
| `literals.lua` | `os.setlocale`, line 161 |
| `strings.lua` | `os.setlocale`, line 150 |
| `sort.lua` | `os.clock`, line 25 |
| `errors.lua` | Lua call-stack limit, line 142 |
| `files.lua` | `os.tmpname`, line 19 |

The reference `files.lua` failure remains the Windows rename of a still-open
default input file at line 59. These independent probes are not complete
`all.lua` or C API coverage. The new corpus does not establish every library
error, scratch layout, coercion, locale/device/path case or resource limit.
Binary chunks, remaining OS/base functions, native C modules, incremental GC,
persistent caches, later dialects and gameplay integration remain unfinished.
Production continues using the preserved legacy engine. See the
[paired measurements](../performance/2026-10-25-rust-lua-native-errors.md).
