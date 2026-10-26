# Lua indexed-assignment storage

Indexed targets reserve storage only for table/key expressions that need a
snapshot. Direct local references keep Lua 5.1's late reads; assignment conflicts
copy and rewrite only affected local addresses. Compact literal keys enter the
constant pool before right-hand evaluation, preserving signed-zero pooling, and
materialize in one reusable scratch slot immediately before each store.
Noncompact keys retain the ordinary early-materialization path. Stores run in
reverse assignment order, and GC maps retain pending tables, keys and values
through callbacks. The hidden key load adds no source hook event.

Assignment stores use the final right-hand expression's source line; vararg
result instructions use their own discharge line. This fixes multiline errors
that previously reported the left-hand line. Function definitions retain their
definition-line store behavior. No ISA or VM dispatch changes are required.
Values remain 16 bytes, frames 64 bytes and flat compiler tasks at most 64 bytes
on 64-bit targets. Production code remains safe Rust with zero normal/build
dependencies. Legacy production consumers are unchanged.

All **299 tests** pass: 285 integration groups, thirteen unit groups and one
documentation example. Three new groups compare 155 programs against vendored
official Lua 5.1.5 on Windows x86-64 MSVC, static CRT, C locale:

| Corpus | Programs |
| --- | ---: |
| 100, 150, 190 and 200 literal-key targets, bounded to 203 entry slots | 4 |
| Snapshots, local conflicts, signed zero, reverse stores, error bytes and multiline locations | 15 |
| 254–300 constant prefixes | 4 |
| Generated mixed literal/dynamic/local keys, conflicting targets and callback traces | 128 |
| Pending objects during GC and yielded multiple results | 4 |

Each case compiles, verifies, executes and drops on a requested 512 KB Rust
thread stack, with an instruction budget. Observations compare scalar number
bits, bytes, nil, booleans, arity and identity predicates. Integer sums/counts
avoid depending on unspecified table traversal order. Existing parser-budget,
capture, debug/hook, runtime/native error, coroutine, GC and library corpora pass.
Formatting, strict Clippy, whitespace and zero normal/build dependency checks
also pass.

```sh
cargo test -p deadlib-lua
cargo fmt -p deadlib-lua -- --check
cargo clippy -p deadlib-lua --all-targets --no-deps -- -D warnings
cargo tree -p deadlib-lua --edges normal,build
python scripts/check_rust_lua_suite.py --output docs/testing/rust-lua-index-targets-suite.json
```

The [official report](2026-10-25-rust-lua-index-targets-suite.json) preserves
sixteen unchanged, checksum-pinned files with `_U=true`, a 100-million-instruction
limit and 60-second per-file deadlines. Rust still passes eleven:
`constructs.lua`, `locals.lua`, `nextvar.lua`, `vararg.lua`, `pm.lua`, `events.lua`,
`gc.lua`, `closure.lua`, `db.lua`, `math.lua` and `errors.lua`. Fifteen reference
files pass. Rust stops at missing `string.dump` in `calls.lua:243`,
`os.setlocale` in `literals.lua:161` and `strings.lua:150`, `os.clock` in
`sort.lua:25`, and `os.tmpname` in `files.lua:19`. The retained reference
`files.lua` failure remains Windows open-input rename at line 59. These isolated
probes do not cover complete `all.lua` or the C API.

The [retained limit probes](2026-10-25-rust-lua-index-targets-limits.json) show
the original 150-target assignment now loads and returns `1,150` in both engines.
A variant first compiles scoped `{true,false,nil}` and a scoped list of integers
1–253. The reference still loads it and returns `1,150`; Rust reports
`function needs too many registers`. Backend nil/boolean materializations consume
constant slots absent from the reference pool, pushing numeric keys outside its
compact path. Full physical-allocation parity remains unfinished.

[Measurements](../performance/2026-10-25-rust-lua-index-targets.md) retain paired
timing, layouts, allocations, heap estimates and a parent comparison. Broader
debug/scratch/resource behavior, remaining libraries, binary chunks, native
modules, incremental GC, caches, later dialects and integrations remain open.
