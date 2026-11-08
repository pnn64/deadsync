# Lua 5.1 runtime operand diagnostics

Runtime type errors identify scoped locals, globals, upvalues, fields and methods
for calls, indexing, arithmetic, concatenation and length. Arithmetic chooses the
first operand that cannot convert to a number; concatenation chooses the first
operand that is neither a string nor a number. Comparison diagnostics retain the
reference's type-pair wording, including its string/thread grouping quirk.

Operand names are reconstructed only while formatting a failure. The lookup
combines existing local scopes, method call sites and verified register
producers. It follows moves to lower registers and forward control flow, handles
the distinction between logical tests and assignments, and treats call results
as anonymous values. Key names come from the table-read producer, before later
register reuse. This adds no per-instruction provenance storage or work to
successful numeric/table dispatch.

Direct indexing errors retain the original operand name. After `__index` or
`__newindex` redirects to another value, that value is anonymous, as in the
reference. Nonfunction metamethods likewise report the failed call without
borrowing the caller's operand name. Protected handlers and coroutine errors keep
their original locations and messages. Runtime diagnostic construction and error
interning now preserve non-UTF-8 names through `Error::message_bytes()`; Rust text
and display remain readable with replacement characters.

All 236 tests pass: 222 integration groups, thirteen unit groups and one runnable
documentation example. Ten new groups include 977 comparisons of complete
runtime error bytes with the reference and one host raw-byte API check:

| Corpus | Reference observations |
| --- | ---: |
| Calls: locals, globals, fields, methods and upvalues | 12 |
| Direct and redirected indexing | 17 |
| Arithmetic, length and concatenation operands | 19 |
| Generated operand types and origins | 160 |
| Ordered comparisons across type pairs | 188 |
| Short-circuit expressions and temporary call results | 181 |
| Distinct-name logical trees with globals, locals and upvalues | 384 |
| Callbacks, loops, tail calls and local scopes | 8 |
| Protected handlers, coroutines and nonfunction metamethods | 8 |

The logical-tree corpus covers both tree shapes and every pair of `and`/`or`
operators with eight value configurations. It checks both arithmetic failures and
failed calls, including cases where the symbolic name differs from the value
selected at runtime. Other generated cases cover nil, booleans, tables,
functions, threads and nonnumeric strings. Field names include invalid UTF-8 and
embedded NUL bytes. Comparison cases cover all failing pairs among seven types
with `<`, `<=`, `>` and `>=`.

Tests compare the same named chunk in both engines under an explicit Lua
`pcall`. The reference is vendored official Lua 5.1.5 on Windows x86-64 MSVC,
static CRT and C locale. No engine unsafe code or runtime dependency was added.
Formatting, strict Clippy and the zero normal/build dependency gate pass.

```sh
cargo test -p deadlib-lua
cargo fmt -p deadlib-lua -- --check
cargo clippy -p deadlib-lua --all-targets --no-deps -- -D warnings
cargo tree -p deadlib-lua --edges normal,build
python scripts/check_rust_lua_suite.py --output docs/testing/rust-lua-runtime-errors-suite.json
```

The [official report](2026-10-25-rust-lua-runtime-errors-suite.json) retains the
sixteen unchanged probes, pinned archive checksum, `_U=true`, 100-million-
instruction budget and 60-second per-file deadlines. These are independent
probes, not complete `all.lua` or C API coverage. Rust passes ten probes:
`constructs.lua`, `locals.lua`, `nextvar.lua`, `vararg.lua`, `pm.lua`, `events.lua`,
`gc.lua`, `closure.lua`, `db.lua` and `math.lua`. Fifteen reference probes pass.

| Remaining probe | Rust stopping point |
| --- | --- |
| `calls.lua` | `string.dump`, line 243 |
| `literals.lua` | `os.setlocale`, line 161 |
| `strings.lua` | `os.setlocale`, line 150 |
| `sort.lua` | `os.clock`, line 25 |
| `errors.lua` | Native argument-name assertion through checker line 12 |
| `files.lua` | `os.tmpname`, line 19 |

A separate call-hook trace runs the unchanged `errors.lua`, recording the first
argument on entry to `checkmessage`. Its last checked program is the tail call
`return math.sin("a")`. The preceding operand-name assertions now pass; native
argument messages still lack the expected function name. The official report
itself runs without that diagnostic hook. The retained reference `files.lua`
failure remains the Windows rename of a still-open default input at line 59.

Runtime/native argument messages, broader control-flow/debug behavior,
register/upvalue/constant limits and complete
source diagnostics still need compatibility work. Binary chunks, remaining
OS/base functions, native C modules, incremental GC, persistent caches, later
dialects and gameplay integration remain unfinished. Production continues using
the preserved legacy engine. See the
[paired measurements](../performance/2026-10-25-rust-lua-runtime-errors.md).
