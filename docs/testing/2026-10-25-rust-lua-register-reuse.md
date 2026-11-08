# Lua expression register reuse

The compiler reuses anonymous result registers for arithmetic and indexing,
allocating right-hand scratch storage after the left operand finishes. Direct
local operands remain register references, preserving Lua 5.1 reads after a
right-hand callback. Global/upvalue values and computed intermediates retain
their earlier captures. Concatenation continues copying its left operand before
evaluating the right side.

Named destinations use separate scratch storage until the complete expression
finishes. Unary expressions now preserve the destination during metamethod
dispatch too: for `x=-f()`, `__unm` sees the old `x` rather than the value returned
by `f`. Operations consume their input values before overwriting an aliased
result register. Existing GC maps track reused slots and continuation jobs retain
operands across callbacks. The ISA and VM execution loop are unchanged.

All 281 tests pass: 267 integration groups, thirteen unit groups and one runnable
documentation example. Seven new groups compare 92 programs against vendored
official Lua 5.1.5 on Windows x86-64 MSVC, static CRT and C locale. Code-generation
programs compile, verify and execute on a requested 512 KB Rust thread stack,
with instruction limits enabled. Comparisons cover scalar bits, booleans, nil,
byte strings, arity and full diagnostic bytes.

| New corpus | Paired observations |
| --- | ---: |
| Long arithmetic chains for five operations at four sizes, cancellation and signed zero | 22 |
| Index chains at 128, 200, 1,000 and 10,000 terms | 4 |
| Right-associated concatenation at 100, 150 and 190 terms | 3 |
| Named destinations, local/global/upvalue reads and metamethods | 10 |
| Weak roots and collection during arithmetic/index/unary callbacks | 3 |
| Generated mutating operand layouts across five operations | 40 |
| Error names and operand types with reused destinations | 10 |

Arithmetic cases assert at most four entry register slots; index cases assert at
most six. Evaluation order and grouping are preserved, including floating-point
cancellation, underflow/overflow and signed-zero results. NaN payload equality
is not claimed. The former adversarial test's 10,000-term arithmetic rejection
was incorrect: it now executes a defined-local version in both engines and
asserts bounded storage. The original 1,000-level parenthesis/block rejection
checks remain.

The earlier flat compiler, capture-limit, debug/hook, runtime-error, coroutine,
standard-library and GC corpora also pass. Strict Clippy, formatting and the zero
normal/build dependency gate pass. Production code remains safe Rust. The
[measurements](../performance/2026-10-25-rust-lua-register-reuse.md) retain paired
reference timings, heap/allocation observations, layouts and parent comparisons.

```sh
cargo test -p deadlib-lua
cargo fmt -p deadlib-lua -- --check
cargo clippy -p deadlib-lua --all-targets --no-deps -- -D warnings
cargo tree -p deadlib-lua --edges normal,build
python scripts/check_rust_lua_suite.py --output docs/testing/rust-lua-register-reuse-suite.json
```

The [official report](2026-10-25-rust-lua-register-reuse-suite.json) preserves
sixteen unchanged files, the pinned checksum, `_U=true`, a 100-million-instruction
budget and 60-second per-file deadlines. Rust passes eleven: `constructs.lua`,
`locals.lua`, `nextvar.lua`, `vararg.lua`, `pm.lua`, `events.lua`, `gc.lua`,
`closure.lua`, `db.lua`, `math.lua` and `errors.lua`. Fifteen reference files pass.

| Remaining Rust file | Stopping point |
| --- | --- |
| `calls.lua` | `string.dump`, line 243 |
| `literals.lua` | `os.setlocale`, line 161 |
| `strings.lua` | `os.setlocale`, line 150 |
| `sort.lua` | `os.clock`, line 25 |
| `files.lua` | `os.tmpname`, line 19 |

The retained reference `files.lua` failure is the Windows rename of an open
default input at line 59. Independent probes do not establish complete `all.lua`
or C API compatibility. This change does not establish exact register-limit
parity or acceptance of every legal expression layout. Deep table constructors,
nested call-target layouts, call/result scratch ranges, exact resource diagnostics
and reference register budgeting still need work. Fixed developer probes confirm
two remaining gaps: 190 nested table constructors and 300 chained empty calls
both load in the reference and return `function needs too many registers` here.
The original 200-term arithmetic probe now loads successfully.
Remaining libraries, binary
chunks, native modules, incremental GC, full debug/resource behavior, caches,
later dialects and integration also remain open. Production uses the preserved
legacy engine.
