# Lua 5.1 compiler register budgeting

The streaming parser tracks reference logical register use separately from the
backend's physical storage. Requests reaching 250 logical slots fail with
`function or expression too complex`. Checks preserve the current token and
reader/error ordering, including unfinished source. This closes the recorded
boundary where `return print(...)` with 249 literal arguments incorrectly loaded;
248 arguments remain accepted.

Flat 16-byte expression descriptors track local, temporary, indexed, relocatable,
call, vararg and conditional forms. Per-function constant pools distinguish
compact constant operands from register operands without copying string bytes.
Names use the existing lexical binding lookup. Constructors retain pending list
fields and fifty-item flushes; open call arguments are checked before consuming
the closing parenthesis. Other discharge operations run at their reference
continuations. Accounting storage is included in parser heap estimates and freed
before code generation.

The backend reserves assignment and return slots as expressions finish. Fixed
call/vararg results grow their destination ranges after operand evaluation and
remain in the GC register map. This permits reference-valid large result lists
that the previous backend rejected after reserving all destinations in advance.
The physical ISA still has 256 slots. The VM instruction loop, public API,
16-byte values and 64-byte frames are unchanged. Production code remains safe
Rust, with zero normal/build dependencies.

All **296 tests** pass: 282 integration groups, thirteen unit groups and one
documentation example. Eight new groups contribute 3,624 protected loader
comparisons and five executed programs against vendored official Lua 5.1.5 on
Windows x86-64 MSVC, static CRT, C locale. Loader observations compare success,
complete diagnostic bytes, protected status and reader-call counts.

| New coverage | Paired observations |
| --- | ---: |
| Calls, methods, returns and constructor limits with 0–200 active locals | 228 |
| Arithmetic, indexing, logical/concat, closure, call and vararg lifetimes | 280 |
| Error precedence, chunk widths and injected reader errors | 115 |
| 254–300 constant-pool boundaries and independent function budgets | 281 |
| Assignments, fixed multiple results and generic-for header scratch | 158 |
| Constructor lookahead and delayed expression discharge | 150 |
| Conditions, indexed-LHS conflicts and generated nested expressions | 2,412 |
| Executed 199-result lists, 247-argument calls, varargs, GC and 190 nested tables | 5 |

The generated subset has 256 expression shapes at nine argument positions,
providing 2,304 observations. Execution checks compile, verify, run and drop on
a requested 512 KB Rust thread stack, comparing scalar number bits, byte strings,
booleans, nil, arity and identity predicates. Existing debug/hook, coroutine,
GC, lexical/capture, diagnostic and library tests also pass. Formatting, strict
Clippy, diff whitespace and zero normal/build dependency checks pass.

```sh
cargo test -p deadlib-lua
cargo fmt -p deadlib-lua -- --check
cargo clippy -p deadlib-lua --all-targets --no-deps -- -D warnings
cargo tree -p deadlib-lua --edges normal,build
python scripts/check_rust_lua_suite.py --output docs/testing/rust-lua-register-budget-suite.json
```

The [official report](2026-10-25-rust-lua-register-budget-suite.json) retains
sixteen unchanged files, the pinned archive checksum, `_U=true`, a
100-million-instruction limit and 60-second per-file deadlines. Rust passes the
same eleven files: `constructs.lua`, `locals.lua`, `nextvar.lua`, `vararg.lua`,
`pm.lua`, `events.lua`, `gc.lua`, `closure.lua`, `db.lua`, `math.lua` and
`errors.lua`. Fifteen reference files pass. Remaining Rust stops are
`string.dump` in `calls.lua:243`, `os.setlocale` in `literals.lua:161` and
`strings.lua:150`, `os.clock` in `sort.lua:25`, and `os.tmpname` in
`files.lua:19`. The reference `files.lua` still fails at the Windows rename of
an open default input at line 59. These are independent probes, not complete
`all.lua` or C API coverage.

Physical allocation still rejects some reference-valid source. A retained
developer probe initializes `local t={}`, assigns 150 distinct literal values to
`t[1],...,t[150]` in one statement, and returns `t[1],t[150]`. The reference loads
and returns `1,150`; Rust reports `function needs too many registers` because
indexed targets retain register materializations. This milestone does not claim
complete register/resource parity. Broader debug/scratch behavior, remaining
libraries, binary chunks, native modules, incremental GC, caches, later dialects
and integrations remain unfinished. Production continues using the legacy engine.

[Measurements](../performance/2026-10-25-rust-lua-register-budget.md) retain
paired timing, allocation, heap and parent-comparison evidence, including the
frontend cost of logical accounting.
