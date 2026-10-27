# Unary operators and source hook compatibility

Unary minus, logical negation and length read local operands directly. Their
previous temporary copy produced an extra observable instruction hook and
could snapshot a value before the unary instruction's callback. Constant
logical negation now folds to one boolean load without interning an unused
operand. Negating a comparison toggles an inversion bit on its existing boolean
materialization, retaining the separate comparison/callback boundary.

The comparison predicate is unchanged: inverting less-than to greater-or-equal
would alter NaN behavior and select a different metamethod. Metamethod identity,
argument order, returned truthiness and errors therefore keep the original
operation. The verifier accepts only inversion bits zero/one on a materializer
immediately following its matching comparison and rejects jumps into that step.
The interpreter's no-hook fused path honors the same bit. Lua 5.1 export emits
the corresponding boolean loads; the private cache retains the flag. Wire
layout stays unchanged and Cargo's compiler identity invalidates older entries.

| Check | Result |
| --- | --- |
| Tests | 528: 65 unit, 459 integration, 4 doc |
| Hook integration groups | 24/24, including 4 new groups |
| New native-export group | 24 operator/negation layouts, two oracle comparisons each |
| Strict Clippy / format | Pass |
| Normal/build dependencies | Only deadlib-lua |
| Current CLI | 168/206 raw; 206/206 after exact known leading banner removal |
| Preserved parent CLI | 165/206 raw; 203/206 after the same banner removal |
| New CLI regressions | 3 parent failures; 3 current/reference exact matches |
| Process probes | 140/140 |
| Official reference/Rust/cache store/cache hit | 17/18 in each mode |
| Official cache outcomes | 18 Stored; 18 fresh-process Hit |
| Build-identity fixture | 16/16 |

The four new hook groups compare 46 compound expression layouts; six local
mutation cases for minus/not/length, including parentheses and collection;
180 generated comparison/negation programs plus a completed-predicate mutation
case; and nine multiline layouts plus two signed-zero/constant-pool controls.
Comparisons span six operators, one through five negations, ordinary numbers,
NaNs on either side, infinities, signed zeros, and table metamethods returning
booleans or objects. Hooks compare complete event/current-line byte strings.
The export group exercises six operators with one through four negations,
ordinary/NaN inputs and hook traces in both the C VM and shared interpreter.
Cache tests add complete metadata/number-bit round trips and a third valid
fresh-process source entry which contains an inverted comparison.

The previous ordinary release CLI was copied before rebuilding and its SHA-256
verified against the committed c2a443bb2 audit. The parent probe intentionally
uses --rust-binary and reports that source provenance is not established by
that invocation. Its binary identity is instead pinned by the earlier committed
ordinary-build audit; current source hashes describe this probe's case corpus.
Only unary_local_hook_events, negated_comparison_hook_events and
constant_not_hook_events fail its supplementary comparison. All three match
raw current/reference stdout, stderr and exit status. Parent raw
[audit](2026-10-26-rust-lua-unary-hooks-parent-audit.json) and current [audit](2026-10-26-rust-lua-unary-hooks-audit.json) are retained.

The strict current CLI audit exits 1 because 38 cases differ only in the
intentional version banner. Supplementary comparison removes at most one exact
known leading banner. The ordinary CLI build runs after the process/suite/build
checks; focused timing then reads those executables without rebuilding them.
The reference archive SHA remains
2640fc56a795f29d28ef15e13c34a47e223960b0240e8cb0a82d9b0738695333.
Its source and current binary hashes are verified. The contract remains x64
Windows, MSVC /O2 /MT, binary64, 64-bit Lua integers, 32-bit native ints, C locale.

All eighteen official files retain prior hashes and unchanged assertions.
Only files.lua:59 fails in every mode, including C Lua, at the existing Windows
open-input rename. The suite exits 1. Bootstrap _U=true, 100,000,000 instructions
and 60-second per-file deadlines remain. Archive SHA-256 is
49e4ca6561f82ea605908c5041ab5fad66ed9930fa0686675bd51b02767f18ad.
These independent entries do not establish all.lua/main.lua or full standalone
compatibility; the earlier Windows module/shell failures remain open gates.

All thirteen current-source maps match their inputs. Two full reference runs
retain 81 runtime and 22 frontend workloads; 14 of the 103 layouts shrink.
Five cache fixtures, six callback fixtures and two focused current/parent/C
process runs are retained. Vm stays 4,440 B, Value 16 B and Frame 64 B. There are
no new runtime fields, opcodes, dependencies or production unsafe blocks.
[Performance evidence](../performance/2026-10-26-rust-lua-unary-hooks.md) records work/layout reductions,
timing tradeoffs and accounting limits. Raw [process](2026-10-26-rust-lua-unary-hooks-process.json),
[suite](2026-10-26-rust-lua-unary-hooks-suite.json) and [identity](2026-10-26-rust-lua-unary-hooks-identity.json) results are retained.
Complete hook/debug/resource parity, full Lua 5.1, later dialects, native code
and gameplay integration remain unfinished; the full objective stays active.

~~~sh
cargo test -p deadlib-lua
cargo fmt -p deadlib-lua --check
cargo clippy -p deadlib-lua --all-targets --no-deps -- -D warnings
python scripts/check_rust_lua_process.py --output docs/testing/2026-10-26-rust-lua-unary-hooks-process.json
python scripts/check_rust_lua_suite.py --cached --output docs/testing/2026-10-26-rust-lua-unary-hooks-suite.json
python scripts/check_rust_lua_cache_identity.py --output docs/testing/2026-10-26-rust-lua-unary-hooks-identity.json
python scripts/check_rust_lua_cli.py --output docs/testing/2026-10-26-rust-lua-unary-hooks-audit.json
~~~
