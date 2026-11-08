# Lua 5.1 userdata and finalization validation

Full userdata now has generation-bearing identity, VM-owned byte storage, an
environment and an optional metatable. `newproxy` uses a weak set of valid
metatables and accepts values bearing those raw metatables, including protected
metatables. Host accessors validate live userdata handles and preserve storage
size and identity.

The collector separates unreachable userdata before preserving their graphs.
It orders pending finalizers by creation sequence rather than reusable slot
index. Lua 5.1 weak values clear before callbacks, while pending userdata remains
available as weak keys. Finalized userdata runs no second callback after
resurrection. Finalizers look up the current `__gc` at dispatch time.

Callbacks run through explicit VM frames; nested collection does not recursively
enter the Rust interpreter. Automatic interruptions retain completed-opcode
destinations and open result ranges. Host-triggered collection retains prior
return values during nested callbacks. Finalizer errors preserve their Lua value
and leave remaining callbacks pending. `Vm::collect_garbage` therefore now returns
`Result<HeapStats, Error>`.

Pending callback graphs are preserved after discovering new finalizable userdata.
A differential regression attaches new userdata to a callback left pending by an
earlier error and verifies that both callbacks run in the reference order.

VM destruction separates existing userdata even when reachable, continues after
Lua errors and does not automatically finalize new userdata created by shutdown
callbacks. Configured instruction limits also cover finalizer execution; pending
callbacks continue after an aborted callback during destruction.

Twelve new differential/adversarial groups bring the integration total to 69.
Three arena/verifier tests, the documentation example and strict Clippy pass.
Observations compare result arity, number bits, byte strings and identity
predicates with vendored official Lua 5.1.5 on Windows x86-64 MSVC in the C locale.
Traversal order and object addresses are excluded from equality claims.

Coverage includes:

- Proxy validation and raw/protected metatables, userdata environments, table
  keys, identity and indexing/call/arithmetic/comparison/length/string methods.
- Reverse creation order, callback replacement, resurrection and once-only
  finalization, with 100 batches that reclaim and reuse slots.
- Weak userdata keys/values and weak finalizer functions.
- Arbitrary Lua errors, protected handlers that collect again, pending-callback
  recovery, native yield restrictions, child coroutines and inherited C-call limits.
- Automatic collection interrupted after new objects and open results, nested
  collection with 500 arguments, host return roots and userdata byte access.
- Zero/incomplete instruction budgets, later VM recovery and shutdown continuing
  after errors without repeatedly finalizing newly created userdata.

```sh
cargo test -p deadlib-lua
cargo clippy -p deadlib-lua --all-targets --no-deps -- -D warnings
python scripts/check_rust_lua_suite.py --output docs/testing/rust-lua-userdata-suite.json
```

The [official report](2026-10-25-rust-lua-userdata-suite.json) preserves thirteen
unchanged probes, `_U=true`, a 100-million-instruction budget and 60-second
deadlines. The archive checksum and source hashes are recorded and source changes
during probing invalidate the report. All thirteen reference files pass. Rust
passes `locals.lua`, `nextvar.lua`, `vararg.lua`, `pm.lua`, `events.lua` and `gc.lua`.
The latter two are new passes. `gc.lua` includes finalization/weak-table checks,
errors, a 200,000-link graph and 1,000 threads with escaping open cells.

| Remaining failing file | Stopping point |
| --- | --- |
| `constructs.lua` | `debug.getinfo`, line 189 |
| `calls.lua` | Reader-based `load`, line 239 |
| `literals.lua` | Reader-based `load` in a helper |
| `closure.lua` | `debug.getinfo`, line 276; propagated through a wrapper at line 284 |
| `strings.lua` | `os.setlocale`, line 150 |
| `sort.lua` | `os.clock`, line 25 |
| `math.lua` | `io.tmpfile`, line 124 |

These are independent probes, not full `all.lua` or C API coverage. No assertions
are removed or rewritten. The official runner captures output before VM drop;
the separate CLI regression checks observable shutdown output and error recovery.
Collection cycles and budgeted steps remain atomic. Incremental pause behavior,
light userdata, remaining libraries/debug APIs, complete diagnostics and broader
resource/platform coverage are unfinished. The engine still cannot run arbitrary
standalone Lua programs. Production consumers retain the legacy engine.
