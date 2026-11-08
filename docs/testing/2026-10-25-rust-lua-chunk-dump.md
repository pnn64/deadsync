# Lua 5.1 function dump validation

`string.dump` now emits target-native Lua 5.1 chunks from functions compiled by
the independent Rust compiler. The exporter lowers the shared register ISA,
fixes native jumps and child indices, emits captures and remaps debug ranges.
Native comparisons materialize booleans, method lookup uses SELF, and generic
iterator helpers become TFORLOOP with compacted temporary-register gaps.
Unused named slots still contribute to native stack size. Active scope events
and indexed child slots avoid rescanning all earlier loop scopes/children.

The wire writer uses a flat continuation stack for nested prototype/debug
sections. Constants and strings are read from immutable program metadata, not
recompiled source or C. Imported prototypes retain their native code, original
lines/local offsets, child indices, constant/name counts and vararg flags for
re-export. This storage is included in VM heap estimates. Source programs do
not allocate that optional native metadata.

Dumps omit captured values and function environments, like Lua 5.1. Loading
creates nil root cells and uses the loading thread's environment. Additional
arguments, including a strip flag, are ignored. Type checks and native-function
errors match the reference probes. Discarded argument values are cleared;
return hooks see the retained function, result and actual registry hook table.
The hook table now backs get/set/delivery directly, so mutations through debug
inspection affect the live callback rather than a detached observation.

All **388 tests** pass: 349 integration groups, 38 unit groups and one doc example.
Eight additional groups cover:

| Area | Evidence |
| --- | --- |
| Source export | Arithmetic/comparisons, table/method calls, loops, closures, old/modern varargs, tail/open results, unused locals and debug scopes execute in both engines. Exported results are also compared with the original source in C. |
| Register pressure | Generated call, return, expression and constructor widths approach native limits; source variants rejected by C are excluded from matching-acceptance claims. |
| Native re-export | Full and stripped official fixtures covering all 38 opcodes and a 25,551-element extended constructor match the C writer byte-for-byte. |
| Generated functions | 128 deterministic programs combine nested captures, iterator scopes, comparisons, logical expressions and open calls. Their dumps run in Rust/C and match original-source scalar observations. |
| Lua API | Closure-cell reset/mutation, source environments, ignored flags, reader reloads, native-function/type errors, arity and return-hook slots. |
| Hook registry | An exposed table is the registered backing table; replacing its thread callback changes gethook and delivery, and disabling the hook clears its entry. |

Stripped wire fixtures are derived by clearing source/debug fields explicitly;
the Lua 5.1 C dump API has no operative strip argument. Malformed mutated input
continues to be checked in Rust without executing it in the C oracle.

All [136 process probes](2026-10-25-rust-lua-chunk-dump-process.json) match complete status,
stdout/stderr bytes and recorded file bytes across the C oracle, Rust runner
and CLI. Four additions cover dump/root-cell reload, exact native re-export
through binary files and byte readers, argument errors/return hooks, and 32
generated dump/load programs. Native file comparisons use the same official
input chunk in every engine. Source-compiled output streams are not compared
byte-for-byte with the C compiler's different instruction stream.

The [official report](2026-10-25-rust-lua-chunk-dump-suite.json) keeps sixteen unchanged,
checksum-pinned files, `_U=true`, 100-million-instruction budgets and 60-second
file deadlines. `calls.lua` now passes, bringing Rust to **15/16**, matching
the reference's pass count. Both retain the Windows `files.lua:59` open-input
rename failure. The command remains unsuccessful; this is isolated file
coverage, not `all.lua`, complete C API coverage or full Lua 5.1 compatibility.

Formatting, strict crate Clippy without dependencies, whitespace checks and
the normal/build dependency tree pass. That tree contains only `deadlib-lua`;
no new production unsafe code or dependency is added. Production consumers
continue to use the legacy engine.

```sh
cargo test -p deadlib-lua
cargo fmt -p deadlib-lua --check
cargo clippy -p deadlib-lua --all-targets --no-deps -- -D warnings
cargo tree -p deadlib-lua --edges normal,build
python scripts/check_rust_lua_process.py --output docs/testing/2026-10-25-rust-lua-chunk-dump-process.json
python scripts/check_rust_lua_suite.py --output docs/testing/2026-10-25-rust-lua-chunk-dump-suite.json
```

The output contract is Windows x86-64 MSVC Lua 5.1.5, little endian, size_t 8,
int/instruction 4 and binary64 8. Source-compiled dumps are interoperable native
chunks, not copies of the C compiler's code generation. Export enforces native
register, constant, child and signed-jump encoding limits; broader stress around
those limits and uncommon IR forms remains open. Adversarial C-accepted chunk
metadata, general debug mutation and exact collector/allocation equivalence
are outside the current matching-reference claim. Other platforms need runtime
validation. Persistent caching, native modules, later dialects, host APIs,
optional JIT and integrations are unfinished; the full objective remains open.

All recorded source hashes match the final tree. The [performance report](../performance/2026-10-25-rust-lua-chunk-dump.md)
retains two runs over 60 runtime and twenty-two frontend workloads.
