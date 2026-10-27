# Lua call and constructor temporary slots

Constructors and calls reuse a destination when it is the top anonymous result
slot. Named destinations and non-top slots use separate storage. List fields
remain adjacent to their table, preserving the existing fifty-item flush and
open-result instruction layout.

Calls evaluate the callee before reserving argument slots, then reserve arguments
one at a time. Methods evaluate their receiver into the working slot, retain it
in the self slot before method lookup, release lookup-key scratch and evaluate
arguments. Callee/receiver snapshots survive argument callbacks. Chained targets
can reuse the same working area. Copies of already aligned fixed/open results
are omitted; existing VM return/top handling supplies the same result range.

Record fields reserve value storage after dynamic keys. Compact literal keys keep
their constant-pool insertion order but defer register materialization until the
value finishes. Signed-zero pooling is preserved. Numeric/boolean/nil keys beyond
the compact pool range, and strings beyond the compact index range, retain the
ordinary key-first path. Effectful keys still execute before their values.

The ISA, VM loop, GC implementation and value/frame layouts are unchanged.
The flat compiler retains its 64-byte task ceiling on 64-bit targets. No unsafe
production code or normal/build dependencies are added.

All 288 tests pass: 274 integration groups, thirteen unit groups and one runnable
documentation example. Seven new groups compare 182 programs against vendored
official Lua 5.1.5 on Windows x86-64 MSVC, static CRT and C locale. Code-generation
cases compile, verify, execute and drop on a requested 512 KB Rust thread stack,
with instruction budgets enabled. Comparisons include scalar bits, nil, booleans,
byte strings, arity, identity predicates and complete error bytes.

| New corpus | Paired observations |
| --- | ---: |
| List, record, numeric/boolean-key and constructor-key nesting at 60, 125 and 190 levels | 15 |
| Empty/fixed-argument and method chains at 300, 1,000 and 10,000 calls | 9 |
| Preallocated results, named targets, snapshots and open results | 10 |
| Signed-zero pooling, invalid/literal keys and 254–300-constant prefixes | 9 |
| Weak objects, pending fields and coroutine/open-result recovery during GC | 5 |
| 128 generated mixed constructors crossing fifty list items | 128 |
| Chained call/method and constructor failure context | 6 |

Call-chain cases assert at most twelve entry register slots. The generated
constructors mix dynamic/literal keys, calls, list entries and final open returns;
known key accesses and explicit traces avoid relying on unspecified traversal or
hole-border choices. The original developer probes with 190 nested constructors
and 300 chained calls now load successfully, matching the reference.

An old verifier corruption case selected the register below a call; that register
is now a valid live local. It now uses the callee slot, which is always outside a
call's temporary range, and still checks rejection. Error context identifies each
corruption case. The production verifier rules are unchanged.

The previous compiler, capture, GC, coroutine, debug/hook, runtime-error and
library corpora pass. Formatting, strict Clippy and the zero normal/build dependency
gate pass. The [measurements](../performance/2026-10-25-rust-lua-call-table-slots.md)
retain paired timings, allocation/heap estimates, layouts and a parent comparison.

```sh
cargo test -p deadlib-lua
cargo fmt -p deadlib-lua -- --check
cargo clippy -p deadlib-lua --all-targets --no-deps -- -D warnings
cargo tree -p deadlib-lua --edges normal,build
python scripts/check_rust_lua_suite.py --output docs/testing/rust-lua-call-table-slots-suite.json
```

The [official report](2026-10-25-rust-lua-call-table-slots-suite.json) preserves
sixteen unchanged files, the pinned checksum, `_U=true`, a 100-million-instruction
budget and 60-second per-file deadlines. Rust still passes eleven: `constructs.lua`,
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
default input at line 59. These independent probes do not cover complete
`all.lua` or the C API. Exact reference register/resource limits, diagnostic
boundaries and broader debug/scratch behavior remain open. A fixed loader probe
with `return print(...)` containing 249 literal `1` arguments is rejected by the
reference as `function or expression too complex near '<eof>'`, while this engine
still accepts it; both accept 248. This remaining boundary needs reference
budgeting rather than a blanket reduction of backend storage.
Remaining libraries, binary chunks, native modules, incremental GC, persistent
caches, later dialects and integration are also unfinished. Production uses the
preserved legacy engine.
