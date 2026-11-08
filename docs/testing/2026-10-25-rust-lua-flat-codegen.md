# Flat Lua code generation

The compiler uses a flat continuation stack for blocks, statements, expressions,
argument/return lists, table fields and nested functions. A task records only
the state needed after its children finish. Register results pass through a
small scalar field; immutable AST edges remain borrowed slices or indices.
Parent function state stays in a flat vector, and ancestor upvalue propagation
uses an iterative search and forward pass. Tasks have a compile-time 64-byte
size ceiling on 64-bit targets. No unsafe code or dependencies are added.

Compilation no longer consumes host stack in proportion to source nesting or
expression-tree depth. The old compiler's artificial 120-level recursive guard
is removed. The resumable parser retains its Lua syntax-depth checks, including
native loader depth. AST folding, verification and destruction already use flat
storage. The bytecode format, VM value layout and runtime frames are unchanged.

Continuation order preserves destination allocation, left/right operand reads,
assignment conflict copies, open results, table-list flushing, loop patching,
scope closure, source lines, call names, debug locals and GC register maps.
Nested functions finish before the parent's closure instruction, preserving
prototype order and source-line behavior.

All 274 tests pass: 260 integration groups, thirteen unit groups and one runnable
documentation example. Five new groups compare 273 execution programs against
vendored official Lua 5.1.5 on Windows x86-64 MSVC, static CRT and C locale.
Reference evaluation occurs outside the worker; Rust compiles, verifies, runs
and drops the programs on a thread with a requested 512 KB stack. Results compare
scalar number bits, booleans, nil, arity and byte strings. Rust instruction
budgets remain enabled during execution.

| Corpus | Paired programs |
| --- | ---: |
| Nested functions at 12, 59, 99, 150 and 190 levels, with ancestor captures and tail calls | 5 |
| Nested `do` blocks at 50, 120 and 190 levels | 3 |
| Shadowed captures at 20, 120 and 190 levels, closing into retained closures | 3 |
| `and`/`or` call chains with 128, 1,000 and 20,000 terms | 6 |
| Generated arithmetic, table, index, method, nested-function, call, logical and assignment contexts | 256 |

The existing capture-limit corpus now runs its previously excluded complete
59- and 60-capture nested fixtures, bringing that corpus to 400 paired loader
observations. They pass on ordinary debug test threads. Existing debug/hook,
runtime-diagnostic, weak/GC, coroutine and standard-library tests also pass.

An auxiliary parent/current executable compares 261 complete `Program` debug
representations: instructions, constants, register counts/maps, upvalue layouts,
prototype order, source lines and all debug metadata. The fixtures comprise the
256 generated programs plus nested functions, blocks, shadowed captures and two
short-circuit chains within the old compiler's limits. All representations match.
This is a corpus check, not a proof for every possible program. The archived
[measurement driver](../performance/2026-10-25-rust-lua-flat-codegen-compare.rs)
contains the comparisons and uses the checked-in fixture generator.

Formatting, strict Clippy and the zero normal/build dependency gate pass.
The [paired measurements](../performance/2026-10-25-rust-lua-flat-codegen.md)
cover the original runtime baseline, frontend scaling and two new deep cases.

```sh
cargo test -p deadlib-lua
cargo fmt -p deadlib-lua -- --check
cargo clippy -p deadlib-lua --all-targets --no-deps -- -D warnings
cargo tree -p deadlib-lua --edges normal,build
python scripts/check_rust_lua_suite.py --output docs/testing/rust-lua-flat-codegen-suite.json
```

The [official report](2026-10-25-rust-lua-flat-codegen-suite.json) keeps sixteen
unchanged probes, the pinned archive checksum, `_U=true`, a 100-million-instruction
budget and 60-second per-file deadlines. Rust still passes eleven: `constructs.lua`,
`locals.lua`, `nextvar.lua`, `vararg.lua`, `pm.lua`, `events.lua`, `gc.lua`,
`closure.lua`, `db.lua`, `math.lua` and `errors.lua`. Fifteen reference probes pass.

| Remaining Rust probe | Stopping point |
| --- | --- |
| `calls.lua` | `string.dump`, line 243 |
| `literals.lua` | `os.setlocale`, line 161 |
| `strings.lua` | `os.setlocale`, line 150 |
| `sort.lua` | `os.clock`, line 25 |
| `files.lua` | `os.tmpname`, line 19 |

The retained reference `files.lua` failure is the Windows rename of a still-open
default input at line 59. Independent probes do not cover complete `all.lua` or
the C API. Register allocation and resource diagnostics still need compatibility
work; removing host recursion does not make all legal expression layouts fit the
reference's register budget. A retained developer probe with `local x=1;return`
followed by a 200-term sum of `x` loads in C; Rust currently returns
`register-pressure:1: function needs too many registers`. The allocator reserves
operand temporaries before descending the left expression tree; reducing that
pressure while preserving operand reads remains required work.
Complete libraries, binary chunks, native modules,
incremental GC, debug/resource behavior, persistent caches, later dialects and
integration remain open. Production continues using the preserved legacy engine.
