# Resumable parser and reader-based Lua 5.1 loading

Fixed-source compilation and text `load` share a scanner and a parser with flat
grammar continuations. The parser retains pending expressions, statements and
constructor lists across input requests. It does not restart parsing a retained
prefix. Reader callbacks run through the existing explicit VM frame stack, with
separate loading jobs for nested calls and coroutine contexts.

All 82 integration groups, eleven unit groups and the documentation example
pass. Strict Clippy passes and the normal/build dependency tree contains only
`deadlib-lua`. The engine continues to forbid unsafe code. Differential tests use
vendored official Lua 5.1.5 on Windows x86-64 MSVC, the static CRT and C locale;
numbers are compared by bits and strings by bytes.

Three parser unit groups add every two-way split and one-byte delivery across
the grammar, 2,000 generated arbitrary byte inputs, a 10,000-field constructor and
early-error input requests. Thirteen reader integration groups cover:

- Chunk boundaries, nil/empty EOF, numeric reader results, first-result selection,
  callbacks receiving zero arguments, native readers and embedded NUL bytes.
- Arbitrary error identity, strict reader argument types and argument-check order.
  Runtime reader errors inherit the active outer `xpcall` handler; syntax errors
  bypass it. An intervening `pcall` resets that handler. Handlers can collect and
  load another chunk.
- Reader request traces for valid and invalid programs across every two-way split,
  including break/vararg errors and declaration-time local limits. Limits account
  for parameters, method `self`, compatibility `arg` and hidden loop locals.
- Thread environments changed by the final reader callback, escaped closures,
  varargs and shared heap identity. Reader-function and `load` environments do not
  replace the destination thread environment.
- Collection during parsing, retained reader/name values, and the reserved third
  argument retained by Lua 5.1's `lua_settop(3)` until a reader string replaces it.
  Extra arguments beyond that slot can be finalized during the callback.
- Nested readers, 300 nested loads at the inherited C-call limit, prohibited
  reader yields and child coroutines that can yield normally.
- One-byte readers for 100 KB comments/strings and a 1,000-field constructor,
  generated widths 1 through 13, and 8,000 repeated loads with collected code,
  closures and transient state. Reusable reserved storage stays below 4 MB in
  the churn regression.
- Instruction aborts in readers on the main thread and coroutines, followed by
  successful execution with restored main-thread state.

Lua 5.1 probes the input once before starting its parser. A nil or empty first
reader result therefore does not permanently close the stream: the reader is
called again. Later nil/empty results end input. Tests compare these calls and
errors with the reference, including a throwing second callback.

```sh
cargo test -p deadlib-lua
cargo clippy -p deadlib-lua --all-targets --no-deps -- -D warnings
cargo tree -p deadlib-lua --edges normal,build
python scripts/check_rust_lua_suite.py --output docs/testing/rust-lua-reader-suite.json
```

The [official report](2026-10-25-rust-lua-reader-suite.json) records thirteen
unchanged probes, archive/source checksums, `_U=true`, a 100-million-instruction
budget and 60-second file deadlines. All thirteen reference probes pass. Rust
retains six passes: `locals.lua`, `nextvar.lua`, `vararg.lua`, `pm.lua`, `events.lua`
and `gc.lua`. No assertions are removed or rewritten.

| Remaining failing file | Current stopping point |
| --- | --- |
| `constructs.lua` | `debug.getinfo`, line 189 |
| `calls.lua` | Passes reader `load`; reaches `debug.getinfo`, line 241 |
| `literals.lua` | `debug.getinfo`, line 11 of the constructed chunk at source line 112 |
| `closure.lua` | `debug.getinfo`, line 276; propagated at line 284 |
| `strings.lua` | `os.setlocale`, line 150 |
| `sort.lua` | `os.clock`, line 25 |
| `math.lua` | `io.tmpfile`, line 124 |

The earlier userdata/scanner reports attributed the `literals.lua` stopping point
to reader loading; the current probe identifies `debug.getinfo` inside its helper.
These are independent files, not full `all.lua` or C API coverage. Binary chunks,
dumping, file loaders, remaining libraries/debug APIs, complete diagnostics and
broader compilation resource limits remain open. Collection cycles still complete
atomically. Arbitrary standalone Lua 5.1 compatibility is not established and
production consumers continue to use the legacy engine.

The [paired measurements](../performance/2026-10-25-rust-lua-reader.md) retain
execution, compilation, allocation and capacity-based heap data for this change.
