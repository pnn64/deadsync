# Lua incremental marking validation

The collector now advances a persistent mark frontier before its bounded arena
sweep. Default zero-budget steps visit up to 128 frontier jobs; an object scan
can enqueue many edges, and duplicate jobs also consume credit. Existing work
multiplier/extra-budget rules continue to apply. Full collection discards partial
marking and retraces current roots before synchronous sweeping.

Write barriers preserve new strong references in reached tables, Lua/native
closure environments, userdata environments/metatables, thread environments and
shared cells. Table metatable changes requeue the table. Final marking refreshes
roots, reached thread stacks, open cells and reached tables with metatables;
this handles register changes and indirect weak-mode upgrades. Weak collectable
edges remain weak while Lua 5.1 string edges remain strong. New mutable objects
are marked and queued, and new strings/file records are protected as leaves.

Builtin validation retains the previous cycle's mark mask during propagation,
so existing live IDs can be pinned before tracing reaches them. Previously
retired IDs still fail validation. Pending finalizers and nested step-created
marking finish their graph/sweep phases before callback dispatch continues.

All **364 tests** pass: 339 integration groups, twenty-four unit groups and one
documentation example. Seven new unit groups cover:

| Area | Evidence |
| --- | --- |
| Frontier and table barriers | One-job slices, strong white keys/values added to a reached table, and a weak white value reclaimed in that same cycle. |
| Metatable changes | Weak-to-strong upgrades through a shared `__mode` field and removal of a table metatable. |
| Environments | Old white graphs assigned to Lua/native functions and userdata; userdata metatable edges survive. |
| Open cells | A reached cell changes in an unreachable suspended thread; atomic refresh preserves its value before closing/freeing the thread. |
| Native handles | Valid builtin pins during partial marking, rejection of retired IDs, and type-metatable roots independent of owner mark state. |
| Cell writes/closure | Writes to closed shared cells and coroutine-driven closing of open cells preserve new references. |
| Full collection | Explicit full collection restarts a partial mark and reclaims a target conservatively marked by a discarded edge. |

The barrier cases complete the active cycle without restarting its marks; a new
full mark could conceal a missing barrier. A new differential integration group
combines closures, custom environments, coroutines, userdata, metatables and
finalizers. It retains identities and reports sum 2,080, zero premature callbacks
and 64 final callbacks. Existing generated GC controls, weak graphs, hooks,
coroutines, loading, I/O, compiler, numeric and calendar tests also pass.

All [111 process probes](2026-10-25-rust-lua-gc-mark-process.json) match full exit status,
stdout/stderr bytes and recorded file contents across official Lua 5.1.5,
the Rust process runner and CLI. Two additions exercise table mutation during
stepping and the combined closure/environment/coroutine/finalizer graph.
Formatting, strict crate Clippy with dependencies excluded, owned-path whitespace
checks and the normal/build dependency tree pass. The latter contains only
`deadlib-lua`. No new unsafe code or Cargo dependency is introduced. Production
consumers remain on the legacy crate.

The [official report](2026-10-25-rust-lua-gc-mark-suite.json) retains sixteen unchanged,
checksum-pinned files, `_U=true`, 100-million-instruction limits and sixty-second
file deadlines. Rust passes twelve; the reference passes fifteen. Rust stops
at `calls.lua:243` (binary dumps), `literals.lua:161`, `strings.lua:150` and
`files.lua:22` (locale support). The reference retains its Windows open-input
rename failure at `files.lua:59`. The suite command exits unsuccessfully; these
are isolated files, not complete `all.lua` or internal C API coverage.

```sh
cargo test -p deadlib-lua
cargo fmt -p deadlib-lua --check
cargo clippy -p deadlib-lua --all-targets --no-deps -- -D warnings
cargo tree -p deadlib-lua --edges normal,build
python scripts/check_rust_lua_process.py --output docs/testing/2026-10-25-rust-lua-gc-mark-process.json
python scripts/check_rust_lua_suite.py --output docs/testing/2026-10-25-rust-lua-gc-mark-suite.json
```

All recorded source hashes match the final tree. Runtime observations remain
pinned to Windows x86-64 MSVC, static CRT, binary64 and C locale. The
[measurements](../performance/2026-10-25-rust-lua-gc-mark.md) retain two runs for 51 runtime
and twenty-two frontend workloads, allocation counts, capacity estimates and
a separately qualified retained-parent process diagnostic.

This stage slices the initial mark frontier and sweeping. Individual object
scans, initial/final root preparation, weak processing, finalizer discovery,
cache rebuilding and callback batches still contain atomic work. String-cluster
repair and native destruction also have variable costs; job/slot counts do not
guarantee wall-clock pause bounds. Exact PUC work units, negative multiplier
behavior and automatic collection inside tag methods remain unfinished. Locale,
binary chunks/native modules, broader diagnostics, caches, host APIs, later
dialects, optional JIT and integrations remain open. The full standalone-engine
objective remains unfinished.
