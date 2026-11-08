# Lua reference constant operands

The frontend retains sparse metadata for compact assignment/constructor keys
selected by Lua 5.1 logical pool rules. Code generation consumes those values
instead of inferring compactness from its own pool. Numeric/boolean/nil slots
materialized only by the backend no longer force reference-compact keys into
persistent registers. Folded boolean and logical keys use the same path.
Noncompact or effectful keys keep ordinary evaluation and storage.

Metadata consists of expression indices and scalar values; string entries point
to existing AST literals without copying bytes. Identifier/method pool entries
are associated with a literal when equal bytes occur. Only deferred store keys
are retained; read-only index chains need no key metadata. The vector is sorted
once and queried by binary search during code generation. Parser heap estimates
include these vectors; metadata is dropped with the AST after compilation.

Each function also records the sign of its first reference-pooled zero. Backend
zero constants use that sign, preventing extra materializations from selecting
the wrong canonical zero. For example, `if 0 then end;return 1/-0.0` now returns
negative infinity, as in the reference. Local-store, constructor,
loaded-function and nested-function cases preserve independent prototype pools.
Equal-length local assignment stores now contribute numeric constants to logical
pool accounting, including assignments that reserve no new register.

The runtime ISA/dispatch, verifier rules, public API and value/frame layouts are
unchanged. Parser descriptors remain 16 bytes and compiler tasks retain their
64-byte ceiling on 64-bit targets. Production code remains safe Rust, with zero
normal/build dependencies. Legacy production consumers remain operational.

All **303 tests** pass: 289 integration groups, thirteen unit groups and one
documentation example. Four new groups compare 519 observations against vendored
official Lua 5.1.5 on Windows x86-64 MSVC, static CRT and C locale:

| Corpus | Paired observations |
| --- | ---: |
| Large backend pools, 100–200 indexed targets, identifier/method string reuse | 10 |
| Folded keys and effectful short-circuit controls | 11 |
| Signed-zero pool order, assignments, scopes and loaded/nested functions | 138 |
| Local-store pool boundaries, 200 active locals and chunk widths 0/7/128 | 360 |

Executed cases compile, verify, run and drop on a requested 512 KB Rust thread
stack under an instruction budget. Comparisons include scalar number bits,
byte strings, nil, booleans, arity and identity predicates. Reciprocals expose
zero signs without relying on integer conversions. Loader cases compare full
error bytes, protected status, acceptance and reader-call counts. Existing
debug/hook, GC, coroutine, resource, runtime/native diagnostic and library tests
pass. Formatting, strict Clippy, whitespace and zero normal/build dependency
checks pass.

```sh
cargo test -p deadlib-lua
cargo fmt -p deadlib-lua -- --check
cargo clippy -p deadlib-lua --all-targets --no-deps -- -D warnings
cargo tree -p deadlib-lua --edges normal,build
python scripts/check_rust_lua_suite.py --output docs/testing/rust-lua-reference-constants-suite.json
```

The [official report](2026-10-25-rust-lua-reference-constants-suite.json) retains
sixteen unchanged checksum-pinned files, `_U=true`, a 100-million-instruction
limit and 60-second per-file deadlines. Rust still passes eleven: `constructs.lua`,
`locals.lua`, `nextvar.lua`, `vararg.lua`, `pm.lua`, `events.lua`, `gc.lua`,
`closure.lua`, `db.lua`, `math.lua` and `errors.lua`. Fifteen reference files pass.
Remaining Rust stops are `string.dump` in `calls.lua:243`, `os.setlocale` in
`literals.lua:161` and `strings.lua:150`, `os.clock` in `sort.lua:25`, and
`os.tmpname` in `files.lua:19`. The reference `files.lua` still fails at Windows
open-input rename line 59. These are isolated probes, not complete `all.lua` or
C API coverage.

The [retained limit probes](2026-10-25-rust-lua-reference-constants-limits.json)
confirm that both bare and prefixed 150-target assignments now load and return
`1,150`. The unused-condition zero probe returns negative infinity in both
engines. Full physical-allocation parity remains open: a function with 100
ordinary locals, `local x=1`, and `return 2^...^x` with 190 literal powers loads
and returns infinity in Lua but still exhausts Rust backend registers.

[Measurements](../performance/2026-10-25-rust-lua-reference-constants.md) preserve
paired timings, allocator/heap observations, layouts and a parent comparison.
Broader source/resource/debug/scratch behavior, remaining libraries, binary
chunks, native modules, incremental GC, caches, later dialects and integrations
remain unfinished.
