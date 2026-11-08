# Register/constant operands and Lua hook windows

Arithmetic, comparisons and table reads/writes now accept register-or-constant
operands in the eight-byte ISA. Registers occupy 0..255; constants use their
pool index plus 256 on RK-capable fields. The verifier checks each mode and its
bounds. The interpreter reads constants directly from the VM's program pool.
Native imports retain RK fields without hidden loads, and source exports emit
native RK fields or bounded scratch loads when a pool index exceeds native RK.

The parser records both reference operand eligibility and each function's
logical stack high-water mark. The source backend removes marked constant-load
placeholders at the consuming instruction and restores the earlier live mask.
Nested left constants pass directly to the instruction, preserving the right
subexpression's visible temporary slots. Non-RK operands still materialize in
registers. Single nonlocal stores use a local RHS directly at the store's hook
boundary. No opcode, VM/frame field or normal/build dependency is added.

Source debug metadata separates the logical stack bound from physical scratch
registers. Hook inspection exposes unnamed slots and the registry hook-table
slot. Return hooks use the actual result top, including zero-result returns;
open-result windows can exceed the fixed register bitmap. Named slots which
overlap callback storage resolve into the callback's live storage. GC roots
protect temporary writes during hooks, and reads of reclaimed temporary object
handles yield nil. Existing broader stack-layout differences remain below.

The private persistent representation advances from schema 1 to schema 2 to
store the logical debug bound. Old-version payloads with corrected CRCs are
rejected; source compilation regenerates entries. RK fields, logical bounds,
native wire data and execution survive round trips and fresh-process reloads.

| Check | Result |
| --- | --- |
| Rust unit / integration / doc tests | 72 / 473 / 4 = 549 |
| Formatting / strict Clippy | Pass |
| Normal/build dependencies | Zero |
| CLI raw stdout/stderr/status | 180/218 |
| CLI after one exact known leading banner is removed | 218/218 |
| New CLI cases | Four parent failures; four current exact matches |
| Process probes | 140/140 |
| Official C / Rust / cache store / fresh cache hit | 17/18 in each mode |
| Official cache outcomes | 18 Stored; 18 Hit |
| Cache build identity | 16/16 |

Five new hook groups include 48 function/window/mask combinations, 84 operand
and value combinations, metamethod/error order, local store mutation, temporary
writes with collection, reclaimed handles and 300 open results. Unit coverage
adds malformed operand/debug bounds, schema-2 metadata round trips and
checksum-correct old-schema rejection. Native export tests compare loop,
capture and table-store traces against source C and execute imports in both
engines. Fresh-process cache coverage adds a sixth valid entry (five text, one
native), stored then hit and reexecuted after GC in independent children.

The preserved b56425ea7 parent CLI is byte-identical to its committed
[ordinary-build audit](2026-10-26-rust-lua-predicates-audit.json). The expanded
[parent audit](2026-10-26-rust-lua-rk-windows-parent-audit.json) fails exactly rk_prior_debug_export_gaps,
hook_window_gc_mutation, source_return_hook_window and rk_field_store_local_hooks.
The [current audit](2026-10-26-rust-lua-rk-windows-audit.json) matches raw output/status in all four.
Strict audit exit 1 is retained for 38 version-banner differences. The
supplementary comparison removes at most one exact known leading banner.
An early release CLI check runs before broad measurements, and the final
ordinary release build follows every other builder. Focused timing reads the
final audited executables without rebuilding.

The unchanged [predicate gap fixture](lua51-predicate-known-gaps.lua) now matches
C completely: source and exported loops each deliver 13 instruction events,
and the nil predicate's unnamed slot observes nil, false, false and 42.
The [stack audit](2026-10-26-rust-lua-rk-windows-stack-audit.json) retains both engines' raw output and
binary/fixture/audit hashes. The additional
[follow-up fixture](lua51-rk-window-followups.lua) retains a matching entry-hook
control for ignored arguments and one remaining difference: a scalar written
to an unused slot is not cleared as C clears the region above an active native
stack during collection. This failing observation is outside the passing
selected CLI corpus. Complete debug/stack/GC parity is not claimed.

All eighteen official sources and assertions retain their prior hashes. Only
files.lua:59 fails, at the Windows open-input rename, in all four modes including
C. The suite exits 1. Bootstrap _U=true, 100,000,000 instructions and 60-second
file deadlines remain. Archive SHA is
49e4ca6561f82ea605908c5041ab5fad66ed9930fa0686675bd51b02767f18ad.
These independent entries do not establish all.lua/main.lua or full Lua 5.1;
earlier Windows module/shell failures remain open.

All 13 full source maps match the final Lua sources and recorded build
inputs. The [snapshot manifest](../performance/2026-10-26-rust-lua-rk-windows-workspace-inputs/manifest.json)
pins the parent-based isolated checkout and frozen Cargo manifest/lock inputs.
Lua, legacy bridge, vendored C and referenced Lua fixtures use exact main-worktree
bytes. Every retained Cargo profile starts in a new, initially absent target
directory so copied timestamps cannot admit older artifacts. Source overrides
in performance JSONs resolve the manifest/lock snapshots. The official CLI
archive SHA is 2640fc56a795f29d28ef15e13c34a47e223960b0240e8cb0a82d9b0738695333.
The reference contract remains x64 Windows, MSVC /O2 /MT, binary64, 64-bit Lua
integers, 32-bit native ints and C locale. New runtime/compiler paths use safe
Rust; existing isolated native platform shims remain.

Two broad runs retain 81 runtime and 22 frontend fixtures; 77 of 103
preceding layouts change and both current sets agree. Cache, callback and
parent/current/C RK measurements also have independent confirmation runs.
Vm remains 4,440 B, Value 16 B and Frame 64 B; logical bounds are per-prototype
metadata. [Measurements](../performance/2026-10-26-rust-lua-rk-windows.md) retain allocation, heap,
compile, runtime and CPU-counter observations and their limitations. Raw
[process](2026-10-26-rust-lua-rk-windows-process.json), [suite](2026-10-26-rust-lua-rk-windows-suite.json) and
[identity](2026-10-26-rust-lua-rk-windows-identity.json) results are retained.
Full standalone Lua 5.1, later dialects, machine-code compilation and gameplay
integration remain unfinished. The complete goal stays active.

~~~sh
cargo test -p deadlib-lua
cargo fmt -p deadlib-lua --check
cargo clippy -p deadlib-lua --all-targets --no-deps -- -D warnings
python scripts/check_rust_lua_process.py --output docs/testing/2026-10-26-rust-lua-rk-windows-process.json
python scripts/check_rust_lua_suite.py --cached --output docs/testing/2026-10-26-rust-lua-rk-windows-suite.json
python scripts/check_rust_lua_cache_identity.py --output docs/testing/2026-10-26-rust-lua-rk-windows-identity.json
python scripts/check_rust_lua_cli.py --output docs/testing/2026-10-26-rust-lua-rk-windows-audit.json
~~~
