# Lua deferred constant operands

Numeric arithmetic left operands and reference compact constants for arithmetic
and comparisons materialize after the right operand finishes. The right side
reuses the working slot. A left constant uses that slot when the right result
is a direct local reference, or one temporary otherwise on this path. Deep right-associated
chains no longer keep a register for every constant left operand.

Small arithmetic/unary trees containing numeric literals and variable references
retain grouped eager loads when a conservative node count fits the remaining
physical registers. Parsing computes a capped 32-node hint once in existing AST
span padding on 64-bit targets; code generation uses one lookup, with no walk
or extra allocation. Explicit calls, indexing, constructors and other expression
forms use deferral. This preserves the existing small-kernel instruction layout
while bounding register use in larger trees.

The existing reference-constant metadata now covers binary operands as well as
store keys. Plain numeric literals use the backend's folded-literal fast path;
folded logical numerals and reference compact strings/booleans/nil use sparse
metadata. Backend pool insertion still occurs before right-hand code generation.
Per-function zero signs retain reference numeric pooling. Dynamic operands,
noncompact reference constants and concatenation keep ordinary evaluation.

Named destinations retain their old values until the operation completes.
Metamethod arguments preserve their original types, including numeric strings
when numeric conversion falls back to a metamethod. GC maps retain pending right
results; deferred loads remain hidden from source line hooks. One flat compiler
continuation handles the deferred load. The ISA, VM dispatch, verifier rules,
public API and value/frame layouts are unchanged. Compiler tasks remain at most
64 bytes and parser descriptors 16 bytes on 64-bit targets. Production code
remains safe Rust with zero normal/build dependencies. Legacy consumers remain
operational.

All **309 tests** pass: 295 integration groups, thirteen unit groups and one
documentation example. Six new groups compare 933 observations against vendored
official Lua 5.1.5 on Windows x86-64 MSVC, static CRT and C locale:

| Corpus | Paired observations |
| --- | ---: |
| Deep numeric chains, six arithmetic operators and up to 200 active locals | 57 |
| Named destinations, late/captured reads, signed zero and complete errors | 12 |
| Six metamethods with five left types, pending roots and coroutine results | 33 |
| Generated literal-left arithmetic trees with callback traces | 256 |
| Numeric-string/logical/boolean/nil chains, comparisons and noncompact pools | 63 |
| Generated mixed expression pressure with large pools and 0–200 locals | 512 |

Executed cases compile, verify, execute and drop on a requested 512 KB Rust thread
stack under an instruction budget. Numeric-chain cases assert at most 216 entry
slots. Number bits, byte strings, nil, booleans, arity and identity predicates are
compared. NaN results are observed through a Lua predicate and normalized to a
string; payload bits are excluded. Loader cases compare acceptance, full error
bytes, protected status and reader-call counts. Existing debug/hook, GC,
coroutine, diagnostic, compiler-resource and library tests pass. Formatting,
strict Clippy, whitespace and zero normal/build dependency checks pass.

```sh
cargo test -p deadlib-lua
cargo fmt -p deadlib-lua -- --check
cargo clippy -p deadlib-lua --all-targets --no-deps -- -D warnings
cargo tree -p deadlib-lua --edges normal,build
python scripts/check_rust_lua_suite.py --output docs/testing/rust-lua-deferred-operands-suite.json
```

The [retained probes](2026-10-25-rust-lua-deferred-operands-limits.json) now load
and execute both 190-power chains with 100 ordinary locals and `local x=1`,
returning infinity for numeric and numeric-string left operands. Both bare and
prefixed 150-target assignments return `1,150`; the unused-condition zero probe
returns negative infinity. The 512 generated pressure cases find no new
acceptance/diagnostic disagreement. This corpus does not prove complete physical
allocation or debug/scratch parity.

The [official report](2026-10-25-rust-lua-deferred-operands-suite.json) preserves
sixteen unchanged checksum-pinned files, `_U=true`, a 100-million-instruction
limit and 60-second per-file deadlines. Rust still passes eleven: `constructs.lua`,
`locals.lua`, `nextvar.lua`, `vararg.lua`, `pm.lua`, `events.lua`, `gc.lua`,
`closure.lua`, `db.lua`, `math.lua` and `errors.lua`. Fifteen reference files pass.
Remaining Rust stops are `string.dump` in `calls.lua:243`, `os.setlocale` in
`literals.lua:161` and `strings.lua:150`, `os.clock` in `sort.lua:25`, and
`os.tmpname` in `files.lua:19`. The retained reference `files.lua` failure remains
Windows open-input rename at line 59. These isolated probes do not cover complete
`all.lua` or the C API.

[Measurements](../performance/2026-10-25-rust-lua-deferred-operands.md) retain
paired timings, allocations, heap estimates, layouts and a parent comparison.
Remaining libraries, binary chunks, native modules, incremental GC, caches,
later dialects, broader resource/debug behavior and integrations remain unfinished.
