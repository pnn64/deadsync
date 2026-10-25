# Lua 5.1 function metadata and stack inspection

`debug.getinfo`, `debug.getupvalue` and `debug.setupvalue` now use shared compiler
and VM metadata. The parser retains token endings, expression completion and
materialization lines, function boundaries and loop headers across input chunks.
Prototypes own definition ranges, local register ranges, upvalue names, call sites
and sorted active-line sets. The verifier checks metadata bounds and ordering.
Heap estimates include its storage.

The VM resolves source information and caller names during inspection. Ordinary
execution keeps the existing 16-byte values, 64-byte frames and numeric builtin
dispatch. Stack inspection walks flat frames, hides internal continuation records
and represents lost tail calls without recreating frames. Suspended, normal and
failed coroutine stacks share the same inspection path. Upvalue mutation uses
the existing open/closed cells, including cells in other thread stacks.

All 106 tests pass: 93 integration groups, twelve unit groups and one documentation
example. Formatting and strict Clippy pass; the normal/build dependency tree
contains only `deadlib-lua`, which continues to forbid unsafe code. Differential
checks use vendored official Lua 5.1.5 on Windows x86-64 MSVC, static CRT and C
locale, comparing number bits, byte strings and exact result counts.

Eleven new integration groups cover:

- Source and short-source names, truncation, NUL termination, definitions and
  every `getinfo` option, including native functions and selected field presence.
- Names inferred from local/upvalue/global calls, fields, methods, dynamic keys,
  logical expressions, factories, native callbacks and metamethods.
- Current lines across multiline parentheses, table/string call arguments and
  split local-function declarations.
- Active-line sets for statement/expression forms and 512 deterministic layouts
  across 32 templates. Each layout is checked as a main chunk and nested function,
  including LF/CRLF and comment-separated tokens. The compiler excludes synthetic
  operand copies and follows Lua 5.1 value-materialization locations.
- Native upvalue counts, Lua upvalue names/order, mutation shared by closures,
  suspended thread cells and cells closed by collection.
- Active/suspended/failed/dead coroutine stacks, native boundaries, lost tail
  records, out-of-range levels and Lua 5.1's negative-level tail-record behavior.
- Error-handler inspection before faulting frames unwind; function identity,
  active-line tables and source metadata retained through collection.
- Argument validation order, numeric coercions, options containing NUL bytes and
  zero-result native/invalid upvalue queries.
- 30,000 tail calls inspected without recursive stack walking, 4,000 transient
  compiled functions and pinned native inspection functions after library removal.

Scanner split tests now compare token endings as well as starts. A new verifier
group rejects corrupt local ranges/registers, upvalue-name counts, call sites and
active-line ordering. Existing reader, coroutine, GC and userdata tests remain
enabled.

```sh
cargo test -p deadlib-lua
cargo fmt -p deadlib-lua -- --check
cargo clippy -p deadlib-lua --all-targets --no-deps -- -D warnings
cargo tree -p deadlib-lua --edges normal,build
python scripts/check_rust_lua_suite.py --output docs/testing/rust-lua-debug-suite.json
```

The [official report](2026-10-25-rust-lua-debug-suite.json) keeps the checksum-pinned
archive, unchanged source files, `_U=true`, a 100-million-instruction budget and
60-second per-file deadlines. All fourteen reference probes pass. Rust passes
eight files: `constructs.lua`, `locals.lua`, `nextvar.lua`, `vararg.lua`, `pm.lua`,
`events.lua`, `gc.lua` and `closure.lua`. The constructs/closure passes are new.
The runner adds the official `db.lua` file; it passes its initial source/name
checks and stops at the first hook invocation. No assertions are removed or
rewritten.

| Remaining failing file | Current stopping point |
| --- | --- |
| `calls.lua` | `string.dump`, line 243 |
| `literals.lua` | `os.setlocale`, line 161 |
| `strings.lua` | `os.setlocale`, line 150 |
| `sort.lua` | `os.clock`, line 25 |
| `math.lua` | `io.tmpfile`, line 124 |
| `db.lua` | `debug.sethook`, line 19 |

These remain independent probes rather than full `all.lua`, complete debug/C API
coverage or a guarantee for arbitrary source layouts. Local inspection/mutation,
tracebacks, hooks, registry inspection, complete diagnostic text, broader resource
and source/register compatibility, binary loading/dumping and remaining libraries
are unfinished. Collection cycles remain atomic. Production uses the legacy
engine and the full standalone Lua 5.1 gate remains open.

The [Lua 5.1 debug API](https://www.lua.org/manual/5.1/manual.html#5.9) and vendored
reference source define the comparison semantics. The
[paired measurements](../performance/2026-10-25-rust-lua-debug.md) record inspection
cost, metadata storage and existing execution/compilation workloads.
