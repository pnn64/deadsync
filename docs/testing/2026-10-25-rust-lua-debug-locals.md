# Lua 5.1 local inspection and operand references

`debug.getlocal` and `debug.setlocal` now inspect and mutate parameters, scoped
locals, loop controls and mapped temporary/argument slots. They operate on the
current thread or another live thread's retained stack. Writes update the actual
register shared by open upvalue cells. Returned values and newly written objects
remain visible to collection.

This work also fixes a compiler compatibility bug. In the reference, a local
operand can be read after a callback on its right has changed that local:

```lua
local a = 1
local function f() a = 2; return 3 end
return a + f(), a  -- 5, 2
```

The previous compiler copied `a` before calling `f`, producing `4, 2`. Arithmetic,
comparison and indexed operands now reference the local register directly.
Indexed assignments copy a table/key local when a later assignment target would
overwrite it. This preserves conflicting addresses while allowing the reference's
later reads after right-hand callbacks. Function arguments and concatenation
retain their earlier value copies where Lua 5.1 requires them.

Each prototype has one flat byte arena for temporary register indices. Call sites
store ranges into it, so inspection omits compiler-only operand copies and loop
tests. The compiler maintains active-name masks instead of rescanning all earlier
declarations. Verification checks ranges, ordering, the call's register boundary
and membership in its pre-call GC map. Metadata sizes are included in heap estimates.
Named lookups use indices into prototypes and intern the existing name bytes.

All 115 tests pass: 102 integration groups, twelve unit groups and one documentation
example. Formatting, strict Clippy and the normal/build dependency gate pass.
The engine continues to forbid unsafe code and has no normal/build dependencies.
Nine new integration groups cover:

- Parameter order, vararg compatibility locals, shadowed scopes, numeric/generic
  loop controls, named values and temporary bounds.
- Mutation of shared cells in active, suspended and failed threads, including
  collection afterward and the suspended yield frame's temporary setter slot.
- Native argument checks, coercions, result arity, negative/lost-tail levels and
  setter validation order.
- Temporary observations in 25 generated expression contexts, including nested
  calls, methods, constructors, arithmetic, comparisons and logical expressions.
- Callback operand order, indexed writes, required assignment snapshots,
  concatenation and function-argument copies.
- All 512 ordered triples from eight local/indexed assignment targets, with a
  right-hand callback changing their tables and keys.
- Protected/native callback boundaries, table traversal, output conversion,
  replacement callbacks and the reader's three retained argument/reserved slots.
- Collection after changing a named register or pending call argument to a new
  table, plus pinned native inspection functions after removing the debug library.

The differential oracle is vendored official Lua 5.1.5 on Windows x86-64 MSVC,
static CRT and C locale. Scalars are compared by number bits/string bytes and
exact result counts. Local-inspection tests use a plain `lua_pcall` entry with no
host error-handler value below it. The ordinary mlua evaluation wrapper puts a
C error-handler function on that base stack, which Lua 5.1 can expose through
negative/lost-tail local queries; that embedding artifact is absent from `Vm::execute`.

PUC's reader/parser also exposes its private lexer table and an internal `proto`
value beyond the three base reader argument/reserved slots. The independent
parser does not use those C stack records. Their representation, broader native
scratch stacks and complete source/register diagnostics remain outside the current
verified compatibility scope. The tests preserve the ordinary reader-slot checks
and explicitly identify this boundary rather than treating private compiler data
as portable Lua language values.

```sh
cargo test -p deadlib-lua
cargo fmt -p deadlib-lua -- --check
cargo clippy -p deadlib-lua --all-targets --no-deps -- -D warnings
cargo tree -p deadlib-lua --edges normal,build
python scripts/check_rust_lua_suite.py --output docs/testing/rust-lua-debug-locals-suite.json
```

The [official report](2026-10-25-rust-lua-debug-locals-suite.json) retains fourteen
unchanged probes, archive/source checksums, `_U=true`, a 100-million-instruction
budget and 60-second per-file deadlines. All fourteen reference probes pass. Rust
retains eight passes: `constructs.lua`, `locals.lua`, `nextvar.lua`, `vararg.lua`,
`pm.lua`, `events.lua`, `gc.lua` and `closure.lua`.

| Remaining failing file | Current stopping point |
| --- | --- |
| `calls.lua` | `string.dump`, line 243 |
| `literals.lua` | `os.setlocale`, line 161 |
| `strings.lua` | `os.setlocale`, line 150 |
| `sort.lua` | `os.clock`, line 25 |
| `math.lua` | `io.tmpfile`, line 124 |
| `db.lua` | `debug.sethook`, line 19 |

No assertions are removed or rewritten. These are independent probes, not full
`all.lua` or C API coverage. Tracebacks and hooks remain unfinished, along with
remaining libraries, binary loading/dumping, incremental collection pauses,
broader diagnostics/resources and later dialects. The full standalone Lua 5.1
gate remains open and production continues to use the legacy engine.

See the [Lua 5.1 debug API](https://www.lua.org/manual/5.1/manual.html#5.9) and the
[paired measurements](../performance/2026-10-25-rust-lua-debug-locals.md).
