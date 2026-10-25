# Lua phased sweeping validation

The collector now retains explicit pause, sweep and finalization phases.
Automatic checks and explicit steps advance a bounded number of arena slots.
The default multiplier scans up to 128 slots per slice; positive work requests
scale that budget, zero multiplier removes the limit, and negative work requests
use the native unsigned shift convention. Full collection remains synchronous.

Each arena protects new allocations with its existing mark bitmap while sweeping
or finalization is pending. Re-interning an unswept white string preserves its
identity and marks it. String deletion repairs the linear-probing cluster before
freeing a slot, so later lookups cannot follow a dead/reused slot. Slot generations
still reject stale identities. Host pin/assignment validation rejects logically
dead values during sweeping, before physical reclamation reaches their slots.
OS builtin IDs are now included in host validation; a regression pins every
registered library function across collection.

After a finalizer error, automatic collection resumes the pending finalization
phase before tracing again. The [retained phase regression](lua51-gc-finalizer-phase.lua)
now prints the reference's `true false true false` in all three runners.
Queries and stop operations leave pending callbacks untouched; explicit steps
can dispatch them. If a callback starts another cycle with a step, its sweep
finishes before newly separated callbacks run.

All **356 tests** pass: 338 integration groups, seventeen unit groups and one
documentation example. Six new groups cover:

| Area | Evidence |
| --- | --- |
| Arena budget | Seven-slot batches, allocations between batches, reused slots, generation rejection and a later full sweep. |
| String clusters | Collision chains wrapping the bucket array, deletion, resurrection, reuse and complete cleanup. |
| Mutating graphs | One-slot sweeps interleaved with new tables, strings, closures, cells, coroutines and userdata; roots and finalizer counts survive. |
| Host builtins | Every registered function can be pinned, including OS functions, and retained across sweeping. |
| Pending callbacks | Queries preserve queued callbacks after an error; explicit steps resume them. |
| Nested steps | A finalizer can finish a phase and start another sweep before returning, without dispatching callbacks too early. |

The previously excluded high-pause finalizer-error case is restored to the
differential regression group. Zero-multiplier and negative-budget checks also
match the oracle. Existing GC/control generation, weak tables, hooks, coroutine,
userdata, compiler, I/O and calendar regressions pass. Formatting, strict crate
Clippy with dependencies excluded and owned-path whitespace checks pass. The
normal/build dependency tree contains only `deadlib-lua`; no new unsafe code or
Cargo dependency is introduced. Production consumers remain on the legacy crate.

All [109 process probes](2026-10-25-rust-lua-gc-sweep-process.json) match full status codes,
stdout/stderr bytes and recorded file contents across official Lua 5.1.5, Rust
runner and CLI. Four additions cover zero multiplier, negative work, the phase
regression and string/closure churn during stepping. The
[retained-parent report](2026-10-25-rust-lua-gc-sweep-regressions.json) records thirteen of sixteen
matching GC cases for parent `508b0e3ac`, versus all sixteen now. Three newly
passing cases are `gc_zero_multiplier`, `gc_negative_step_budget` and
`gc_finalizer_phase`; no mismatch is normalized or excluded from these totals.

The [official report](2026-10-25-rust-lua-gc-sweep-suite.json) retains sixteen unchanged,
checksum-pinned files, `_U=true`, 100-million-instruction budgets and sixty-second
file deadlines. Rust still passes twelve, the reference fifteen. Rust stops at
`calls.lua:243` (binary dumps), `literals.lua:161`, `strings.lua:150` and
`files.lua:22` (locale support). The reference retains its Windows open-input
rename failure at `files.lua:59`. The suite command exits unsuccessfully; these
are isolated files, not complete `all.lua` or internal C API coverage.

```sh
cargo test -p deadlib-lua
cargo fmt -p deadlib-lua --check
cargo clippy -p deadlib-lua --all-targets --no-deps -- -D warnings
cargo tree -p deadlib-lua --edges normal,build
python scripts/check_rust_lua_process.py --output docs/testing/2026-10-25-rust-lua-gc-sweep-process.json
python scripts/check_rust_lua_suite.py --output docs/testing/2026-10-25-rust-lua-gc-sweep-suite.json
```

All report hashes match the final tree. Runtime comparison remains pinned to
Windows x86-64 MSVC, static CRT, binary64 and C locale. No cross-platform runtime
claim is added. [Measurements](../performance/2026-10-25-rust-lua-gc-sweep.md) retain two runs for
50 runtime and twenty-two frontend workloads.

This is an incremental-sweeping stage, not a completed incremental collector.
Marking, root/weak processing, cache compaction and callback batches still have
atomic work. String-cluster repair and native destruction have variable costs;
bounded slot scans do not guarantee bounded wall-clock pauses. Exact PUC work
units, negative multiplier behavior and automatic collection inside tag methods
remain unfinished. Incremental marking requires mutator barriers and additional
phase tests. Locale support, binary chunks/native modules, broader diagnostics,
caches, host APIs, later dialects, optional JIT and integrations remain open.
The full standalone-engine objective remains unfinished.
