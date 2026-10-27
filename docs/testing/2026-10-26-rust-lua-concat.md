# Lua 5.1 source concatenation validation

Eligible right-associated source concatenations now extend one existing range
instruction rather than creating every binary intermediate. Operand expressions
still run left to right. The runtime uses the same native-style string grouping,
numeric coercion and resumable metamethod path as imported chunks. Left-associated
groups remain separate. A direct right-hand concat (including parentheses) may
extend a contiguous range; a logical expression cannot be treated as that concat,
because another path may jump past its instruction.

The range keeps the innermost instruction's before-GC map, which roots every
pending operand through callbacks, and rebuilds its after-map for the outer result.
No operand is reassociated or evaluated early, and named destinations retain their
old value until completion. The existing range opcode, continuation frames and
buffer pool are reused. No VM value/frame field, production dependency or unsafe
code is added. Copied concat operands now retain their operator's line in both
hook and ordinary location metadata. A forward conditional edge bypasses an
opposite test of the same unchanged register; that test cannot jump on this edge.

All **398 tests** pass: 43 unit tests, 354 integration tests and one doc example.
Five new groups cover:

| Area | Evidence |
| --- | --- |
| Evaluation and grouping | Calls, table metamethod results, explicit left/right parentheses, logical arms, late local mutation and named assignment targets match C scalar results and callback traces. |
| GC and errors | Collection inside concat callbacks retains pending operands; arbitrary error objects preserve identity; yielding retains the Lua 5.1 metamethod restriction. |
| Generated contexts | 256 deterministic depth-four concat/logical trees compare complete scalar/callback observations. |
| Hooks | Multiline, parenthesized and conditional functions compare exact line/count event sequences and currentline fields. The new probe exposed and fixed copied-operand line metadata and a redundant logical test. Instruction counts for arbitrary programs remain representation-dependent. |
| Native export | A 190-operand chain, local mutation, metamethod grouping and error observations execute identically in Rust/C from the exported chunk and match original-source C execution. |

All [140 process probes](2026-10-26-rust-lua-concat-process.json) match complete exit status,
stdout/stderr bytes and recorded file bytes across the C oracle, Rust runner
and CLI. Two additions cover source-range collection/dump reload and exact
multiline hook observations. The [official report](2026-10-26-rust-lua-concat-suite.json)
keeps sixteen unchanged checksum-pinned files, `_U=true`, 100-million-instruction
budgets and 60-second file deadlines. Both engines pass **15/16**; both fail the
Windows `files.lua:59` open-input rename assertion. The command remains
unsuccessful. These isolated files do not establish full-suite or C API coverage.

Formatting, strict crate Clippy without dependencies and whitespace checks pass.
The normal/build dependency tree contains only `deadlib-lua`. Production consumers
continue to use the legacy engine.

```sh
cargo test -p deadlib-lua
cargo fmt -p deadlib-lua --check
cargo clippy -p deadlib-lua --all-targets --no-deps -- -D warnings
cargo tree -p deadlib-lua --edges normal,build
python scripts/check_rust_lua_process.py --output docs/testing/2026-10-26-rust-lua-concat-process.json
python scripts/check_rust_lua_suite.py --output docs/testing/2026-10-26-rust-lua-concat-suite.json
```

Final report source hashes match the current tree. The
[measurements](../performance/2026-10-26-rust-lua-concat.md) retain an unchanged-compiler
baseline and two final runs over 64 runtime and 22 frontend workloads. The
reference contract remains Windows x86-64 MSVC official Lua 5.1.5, static CRT,
binary64 and initial C locale. Full standalone compatibility, source/register
resource limits, broader debug/collector/diagnostic coverage, cross-platform
validation, persistent caching, native modules, later dialects, host APIs,
optional JIT and song/noteskin integrations remain unfinished.
