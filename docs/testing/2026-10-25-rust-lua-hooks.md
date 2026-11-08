# Initial Lua 5.1 hook validation

`debug.sethook` and `debug.gethook` now control each thread's call, return,
tail-return, line and instruction-count events. Flat continuation frames deliver
callbacks without recursively entering the interpreter. Each thread has 48 bytes
of hook state; values remain 16 bytes and call frames 64 bytes on this target.
Compiler hook maps store one line number per instruction, with zero for omitted
compiler work. The verifier checks map lengths and comparison-result structure.

Hooks can inspect and mutate named locals, parameters and mapped temporaries,
collect garbage, run protected calls and resume child coroutines. Delivery is
disabled inside the hook itself, while count scheduling still advances through
its Lua instructions. Replacing or disabling a hook takes effect when control
returns. Callback errors retain arbitrary payloads and faulting stacks; protected
unwind and quota abort restore hook delivery. Native/metamethod yield barriers
also apply to hook callbacks.

Finalizer callbacks suppress hook delivery on their own thread while count
scheduling continues. The suppression depth moves with the thread, so a resumed
child can still use its own hooks. Automatic collection preserves a pending call
event while finalizers run; protected unwind restores the prior suppression state.

New threads inherit mask/count controls but have no registered Lua callback.
Hook functions are held strongly by a VM-wide map keyed by numeric thread identity;
keys do not retain their thread. This mirrors the reference registry's strong
function values and non-owning thread keys, without storing C pointers. Map
capacity and prototype hook maps are included in heap estimates.

Several compiler changes preserve observable execution order:

- A single-local return uses that local's actual register. A return hook can
  change the returned value after captured upvalues have already been closed.
- Relocatable single assignments write their destination directly, avoiding an
  extra copy. Table constructors and short-circuit expressions retain the prior
  destination until their evaluation is finished.
- Comparison and boolean materialization are separate steps, so a count hook
  between them cannot change an already determined result. Between those steps,
  the private boolean result is excluded from observable temporary/root maps.
  Numeric execution fuses the steps when line/count hooks are off.
- Initial nil markers avoid clearing fresh slots again after a call hook has
  changed them. Existing named locals and loop bodies retain their nil stores.
- Hook maps omit synthetic relocations, RK constant loads and branch work already
  represented by the comparison. Returning from a callback skips those omitted
  instructions when recovering the previous visible line.

All 148 tests pass: 134 integration groups, thirteen unit groups and a runnable
documentation example. Formatting, strict Clippy and the normal/build dependency
gate pass. The engine forbids unsafe code and has no normal/build dependencies.
Eighteen new integration groups cover:

- Function identity, canonical masks, embedded NULs, signed/wrapped counts,
  non-finite conversion, thread controls and disabling hooks.
- Call/return/tail-return order, visible function names/lines and collection.
- Parameter mutation before tail-call collapse, preserving both caller and callee
  during the call callback.
- Line traces across branches, repeats, numeric/generic loops, calls and returns.
- Non-recursive delivery, caught callback failures and subsequent hook recovery.
- Empty numeric loops at count intervals 1, 4 and 4,000.
- Inherited controls, missing inherited callbacks and explicit coroutine hooks.
- Child resumes, collection and rejection of a direct yield from the hook.
- Handler/mask replacement from inside an active callback.
- Closed upvalues versus return-register mutation.
- `xpcall` inspection of hook failures before unwind and arbitrary error identity.
- Strong hook-function storage with non-owning thread keys through collection.
- Quota abort inside a coroutine hook and later main-thread hook delivery.
- Twenty-seven generated count traces: nine expression/control-flow layouts at
  intervals 1, 2 and 4, comparing exact current-line sequences.
- Mutation between comparison and materialization, with collection in the hook.
- Entry callbacks, initial nil locals, explicit nil assignments and loop lifetimes.
- Temporary writes in a call hook remaining alive during collection, then becoming
  collectible after the target overwrites them.
- Finalizer suppression and pending entry events under automatic collection
  pressure across 300 proxy/vararg-call iterations.

The oracle is vendored official Lua 5.1.5 on Windows x86-64 MSVC, static CRT and C
locale. Tests enter through plain `lua_pcall` with no host error-handler value
below the chunk. Scalar observations compare exact number bits, string bytes and
result arity. The quota-recovery group is engine-only.

This is initial hook coverage. Instruction-event parity for all source layouts
has not been established. Call-hook temporary bounds currently follow the
independent VM's compiled register window; exact PUC scratch-register footprints
remain open. Broader native/parser scratch stacks, diagnostic text, registry
exposure and resource limits also remain unfinished. The verified cases above
do not establish complete debug-library or standalone Lua compatibility.

```sh
cargo test -p deadlib-lua
cargo fmt -p deadlib-lua -- --check
cargo clippy -p deadlib-lua --all-targets --no-deps -- -D warnings
cargo tree -p deadlib-lua --edges normal,build
python scripts/check_rust_lua_suite.py --output docs/testing/rust-lua-hooks-suite.json
```

The [official probe report](2026-10-25-rust-lua-hooks-suite.json) retains fifteen
unchanged files, the pinned archive checksum, individual source hashes, `_U=true`,
a 100-million-instruction budget and 60-second per-file deadlines. All fifteen
reference probes pass; Rust retains eight passes: `constructs.lua`, `locals.lua`,
`nextvar.lua`, `vararg.lua`, `pm.lua`, `events.lua`, `gc.lua` and `closure.lua`.
`db.lua` advances from missing `sethook` to the `io.read` lookup at line 311,
after its initial line/call/local/upvalue assertions.

| Remaining failing file | Current stopping point |
| --- | --- |
| `calls.lua` | `string.dump`, line 243 |
| `literals.lua` | `os.setlocale`, line 161 |
| `strings.lua` | `os.setlocale`, line 150 |
| `sort.lua` | `os.clock`, line 25 |
| `math.lua` | `io.tmpfile`, line 124 |
| `db.lua` | `io.read`, line 311 |
| `errors.lua` | Syntax-message assertion in `checksyntax`, line 20 |

Assertions are unchanged. These are independent probes, not full `all.lua` or C
API coverage. Remaining libraries, binary chunks, incremental collection pauses,
complete diagnostics/debug behavior, broader resource limits and later dialects
remain unfinished. Production continues to use the preserved legacy engine.
See the [paired performance report](../performance/2026-10-25-rust-lua-hooks.md).
