# Protected Lua calls from Rust callbacks

HostCall::call invokes a Lua/native function or callable value while retaining
the active Rust callback, original arguments, parent locals and upvalues.
HostCall::call_and_return appends all nested results in their existing register
range. A hidden protected continuation returns child runtime errors to Rust
with their original Lua error value. Outer handlers are left intact. Nested
calls use the outer entry's remaining instruction counter; a resource-limit
error remains fatal even when Rust ignores it or returns a different result.
Cleanup returns to the owning coroutine and closes discarded open upvalues.

Returned result slices and child error values remain collector roots until a
later call/result write reuses that storage. Retain objects in captures or
explicit results before then. Raw object IDs remain scoped to their originating
VM and may coincide across VMs. Child coroutines can resume/yield normally;
execution cannot suspend a Rust callback's stack. Rust panics retain the earlier
contract: unwind past Lua handlers, discard the VM, skip Lua finalizers on drop.

| Check | Result |
| --- | --- |
| Tests | 523: 64 unit, 455 integration, 4 doc |
| New reentry integration groups | 11/11 |
| Protected C API differential programs | 18/18 |
| Strict Clippy / format | Pass |
| Normal/build dependencies | Only deadlib-lua |
| CLI | 165/203 raw; 203/203 after exact known leading banner removal |
| Process probes | 140/140 |
| Official files: reference, Rust, cache store, cache hit | 17/18 in each mode |
| Official cache outcomes | 18 Stored; 18 fresh-process Hit |
| Build-identity fixture | 16/16 |

The eleven groups cover multiple/empty/nil results, callable metamethods,
repeated nested calls with explicit result writes, runtime error identity and
escaped upvalues, sticky instruction quotas, coroutine callers and resumed
children, retained result/error roots across GC and finalizers, quota aborts in
child coroutine chains, 7,998-result success versus the 8,000-slot native bound,
recursive native stack overflow, and nested Rust panic/drop behavior. The two
documentation examples register addition and invoke Lua from a Rust callback.

The C API oracle is a raw lua_CFunction which copies its original arguments,
uses lua_pcall with LUA_MULTRET and returns success/error plus result values.
Preserving the original slots allows direct comparison of debug.getlocal and
debug.setlocal inside the live native frame. Eighteen programs check exact
scalar number bits and byte strings, error-object identity predicates, callable
values, hooks, reader errors beneath outer xpcall handlers, finalizer errors,
environments, native-slot inspection/mutation, recursive calls and coroutine
yield restrictions. The oracle's fixed arguments fit Lua's reserved C stack;
Lua errors stay inside lua_pcall and never longjmp across Rust drop state.
Production remains safe Rust with no normal/build dependency on this oracle.

All eighteen upstream file entries retain their prior source hashes. Each mode
fails only files.lua:59, the existing Windows open-input rename failure. Suite
exit 1 remains expected. Bootstrap _U=true, 100,000,000 instructions and a
60-second per-file deadline remain. Cached programs are file-entry programs;
dynamic loads compile normally. No upstream assertions are modified. Archive
SHA-256 is 49e4ca6561f82ea605908c5041ab5fad66ed9930fa0686675bd51b02767f18ad.
The earlier unchanged-main.lua Windows module/shell failures remain an open
gate; these eighteen probes do not establish complete standalone compatibility.

The strict CLI audit builds the ordinary release executable last and exits 1:
38 cases differ only in the intentional version banner. The supplementary
comparison removes at most one exact known leading banner. The official Lua
5.1.5 reference archive SHA is
2640fc56a795f29d28ef15e13c34a47e223960b0240e8cb0a82d9b0738695333.
Binary/source hashes are checked. The byte-exact platform contract remains
x64 Windows, MSVC /O2 /MT, binary64, 64-bit Lua integers, 32-bit native ints and
C locale. No cross-platform libm bit guarantee follows.

~~~sh
cargo test -p deadlib-lua
cargo fmt -p deadlib-lua --check
cargo clippy -p deadlib-lua --all-targets --no-deps -- -D warnings
python scripts/check_rust_lua_process.py --output docs/testing/2026-10-26-rust-lua-host-reentry-process.json
python scripts/check_rust_lua_suite.py --cached --output docs/testing/2026-10-26-rust-lua-host-reentry-suite.json
python scripts/check_rust_lua_cache_identity.py --output docs/testing/2026-10-26-rust-lua-host-reentry-identity.json
python scripts/check_rust_lua_cli.py --output docs/testing/2026-10-26-rust-lua-host-reentry-audit.json
~~~

All ten current-source maps match their inputs. Two full reference runs retain
all 103 instruction/register layouts; five cache fixtures preserve layouts and
file sizes. The shared remaining counter changes Vm from 4,432 to 4,440 B.
Value remains 16 B and Frame 64 B. The configured optional quota retains its
full u64 range; nested calls do not reset it. No new per-call heap buffer is
needed. Two callback runs assert zero Rust allocations/reallocations/requests
after warmup in all six fixtures, including nested Lua calls.
[Measurements](../performance/2026-10-26-rust-lua-host-reentry.md) record the slower interpreter/C API
tradeoffs and process counters. Raw [CLI](2026-10-26-rust-lua-host-reentry-audit.json),
[process](2026-10-26-rust-lua-host-reentry-process.json), [suite](2026-10-26-rust-lua-host-reentry-suite.json) and
[identity](2026-10-26-rust-lua-host-reentry-identity.json) results are retained. Full Lua 5.1, remaining
library/debug/resource gates, later dialects, optional native code and gameplay
integration remain unfinished; the full objective stays active.
