# Lua GC control validation

Full collection, explicit GC steps and `Vm::collect_garbage` now re-enable
automatic collection. `restart` discards the old threshold and waits for a new
allocation rather than collecting at a VM frame boundary. `setpause` changes
apply on cycle completion. Successful finalizers restore the threshold and
stop state saved on entry; errors skip that restoration and preserve the last
control changes. Completed cycles apply the current pause setting.

This follows the control transitions in the vendored
[Lua 5.1 GC API](../../vendor/lua-src/lua-5.1.5/lapi.c) and
[GCTM/cycle implementation](../../vendor/lua-src/lua-5.1.5/lgc.c).
The continuation saves the threshold in its unused argument-start field and
the callback/stop flags in its program counter. Argument count remains zero,
so generic root tracing sees no argument values there. No new storage vector,
frame-size increase, dependency or unsafe code is introduced. Production
consumers continue to select the legacy crate.

All **350 tests** pass: 336 integration groups, thirteen unit groups and one
documentation example. Nine new groups cover:

| Area | Evidence |
| --- | --- |
| Resumption | Default/nil/NUL options, full collection, small/large explicit steps, restart and a later explicit stop. |
| Thresholds | A huge prior pause setting, restart activation and deferred pause changes. |
| Finalizers | Successful stop/restart changes, nested collection, arbitrary error identity and failed stop/restart changes. |
| Automatic finalizers | A successful tag method cannot leave collection stopped. |
| Shared state | A finalizer suspends/resumes a child coroutine; `xpcall` handler controls survive error handling. |
| Hooks | Call-hook changes precede the native cycle; return-hook changes follow it. |
| Invalid arguments | Validation failures preserve the stopped collector. |
| Host collection | Rust API results and subsequent reachability match the reference with successful/failing finalizers. |
| Generated controls | 64 deterministic sequences of five stop/restart/collect/step/count/pause operations. |

Reachability checks retain 1,000 weakly observed objects strongly during control
operations, then release the keeper and create 10,000 temporary tables. They
compare the weak table's emptiness, avoiding exact automatic collection counts.
Existing GC, userdata, callback, coroutine, hook, compiler, I/O and calendar
regressions pass. Formatting, strict crate Clippy with dependencies excluded and
owned-path whitespace checks pass. The normal/build dependency tree contains
only `deadlib-lua`.

All [105 process probes](2026-10-25-rust-lua-gc-controls-process.json) match full status codes,
stdout/stderr bytes and retained file contents across the official reference,
Rust runner and CLI. Twelve new GC probes include coroutine and hook cases.
The [retained-parent comparison](2026-10-25-rust-lua-gc-controls-regressions.json) records four of twelve
matching cases for parent `a7529feea`, versus all twelve now. The original
stop/full-collection regression was also observed failing before the fix.

## Remaining phase mismatch

The [standalone reproducer](lua51-gc-finalizer-phase.lua) changes the pause to
2,000,000,000 and raises an error in a finalizer. After releasing its strong
keeper and allocating temporary tables, the reference prints
`true false true false`; Rust prints `true false true true` (tab separated).
Raw statuses/output are retained separately in the regression JSON. This case
is **not included in the 105 passing process probes**.

The reference can finish an in-progress finalization phase and apply the large
pause without another mark/sweep. Rust's next atomic cycle traces again and
clears the weak values. Exact incremental phases, work budgets, negative/zero
step-multiplier edge behavior and automatic collection during finalizers remain
unfinished. This milestone fixes the tested control transitions and does not
establish complete GC compatibility or exact pause/step counts.

## Official suite and scope

The [official report](2026-10-25-rust-lua-gc-controls-suite.json) retains sixteen unchanged,
checksum-pinned files, `_U=true`, 100-million-instruction budgets and sixty-second
file deadlines. Rust passes twelve, the reference fifteen. Rust's remaining
stops are `calls.lua:243` (binary dumps), `literals.lua:161`, `strings.lua:150`
and `files.lua:22` (locale support). The reference retains its Windows open-input
rename failure at `files.lua:59`. The suite command exits unsuccessfully; these
are isolated probes, not complete `all.lua` or internal C API coverage.

```sh
cargo test -p deadlib-lua
cargo fmt -p deadlib-lua --check
cargo clippy -p deadlib-lua --all-targets --no-deps -- -D warnings
cargo tree -p deadlib-lua --edges normal,build
python scripts/check_rust_lua_process.py --output docs/testing/2026-10-25-rust-lua-gc-controls-process.json
python scripts/check_rust_lua_suite.py --output docs/testing/2026-10-25-rust-lua-gc-controls-suite.json
```

All report source hashes match the final tree. Runtime comparisons remain pinned
to Windows x86-64 MSVC, static CRT, binary64 and C locale. This safe Rust change
adds no cross-platform runtime claim. The [measurements](../performance/2026-10-25-rust-lua-gc-controls.md)
retain two runs for 49 runtime and twenty-two frontend workloads.

Incremental collection, locale support, binary chunks/native modules, broader
numeric/resource/debug semantics, caches, host APIs, later dialects, optional
JIT and song/noteskin integrations remain open. Arbitrary standalone Lua 5.1
and the full user objective remain unfinished.
