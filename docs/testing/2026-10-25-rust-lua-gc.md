# Lua 5.1 tracing collection validation

The engine reclaims tables, interned strings, Lua/native closures, upvalue cells,
threads and loaded code with an iterative tracing collector. Flat object arenas
reuse slots with generation-bearing identities. Root discovery covers live
compiler register maps, dynamic multiple results, native jobs, thread environments,
resumer chains, metatables and explicitly pinned host values. Escaping cells are
closed before unreachable thread stacks are released.

Ten new groups bring the integration total to 57. Three arena/verifier tests,
the documentation example and strict Clippy pass. Differential observations use
the vendored official Lua 5.1.5 interpreter on Windows x86-64 MSVC in the C locale.
They compare scalar bits/bytes, arity and identity predicates. Memory counts are
checked through relationships and bounded storage, rather than equality with the
C allocator.

Coverage includes:

- Cycles and weak keys, values and combined modes, including Lua 5.1's strong
  string treatment, strong values in weak-key tables, changing modes and NUL
  termination of mode strings.
- Suspended/failed thread reclamation with escaping cells, shared identities,
  nested resumers and pattern iterators retained across yields.
- Collection inside replacement callbacks, sorting, traversal, metamethods and
  protected error handlers, plus 500 open arguments.
- Stop/restart, count, full collection, pause/multiplier controls and budgeted
  step completion, including NUL-terminated option names.
- Pinned host values, released and foreign roots, collected string/table/native
  handles, slot reuse and per-VM program environments retained by host programs.
- Automatic collection in concatenation-only loops, large live arrays, a
  20,000-link graph, 150 live local values and twelve batches of 1,000 dynamic
  chunks. Churn reclaims over 20,000 objects and keeps estimated reserved storage
  below 2 MB in that regression workload.

```sh
cargo test -p deadlib-lua
cargo clippy -p deadlib-lua --all-targets --no-deps -- -D warnings
python scripts/check_rust_lua_suite.py --output docs/testing/rust-lua-gc-suite.json
```

The [official probe report](2026-10-25-rust-lua-gc-suite.json) runs thirteen
unchanged files with `_U=true`, a 100-million-instruction budget and 60-second
deadlines. It pins the archive checksum and records source hashes; the runner
rejects results if sources change during probing. All thirteen reference probes
pass. Rust passes `locals.lua`, `nextvar.lua`, `vararg.lua` and `pm.lua`.

| File still failing | Current stopping point |
| --- | --- |
| `constructs.lua` | `debug.getinfo` at line 189 |
| `calls.lua` | Reader-based `load` at line 239 |
| `events.lua` | `newproxy` at line 302 |
| `literals.lua` | Reader-based `load` in its helper |
| `closure.lua` | `debug.getinfo` at line 276, propagated through a wrapper at line 284 |
| `strings.lua` | `os.setlocale` at line 150 |
| `sort.lua` | `os.clock` at line 25 |
| `math.lua` | `io.tmpfile` at line 124 |
| `gc.lua` | `newproxy` at line 205 |

`gc.lua` gets through its table/string/function churn, step-size comparisons,
stop/restart pressure and weak-table assertions before reaching userdata. These
are independent probes, not a passing `all.lua` run or complete C API coverage.
No suite assertions are removed or rewritten.

Cycles currently complete atomically. `collectgarbage('step', n)` accumulates a
budget and then completes an entire cycle; incremental pause behavior is still
unfinished. Userdata/proxies, finalizers, registry/debug APIs and later dialect
GC rules remain open. The [measurements](../performance/2026-10-25-rust-lua-gc.md)
include timing and capacity-based heap estimates, not exact allocator peaks or RSS.
Production consumers continue to use the preserved legacy engine.
