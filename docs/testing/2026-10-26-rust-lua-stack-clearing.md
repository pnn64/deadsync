# Lua 5.1 stack clearing

Thread marking now clears unused Lua register cells to nil, including scalar
values. Previously, a hook could write 17 into an unused slot, run collection,
and still read 17 where official Lua 5.1.5 returned nil. The unchanged
[reproduction](lua51-rk-window-followups.lua) now matches the reference.

The root scan records live register positions as well as object edges, then
clears unused cells within Lua-owned windows. Native scratch, aliased arguments,
open upvalues and active hook/result windows remain protected. Hook inspection
and GC share the interrupted stack bound, including open results beyond 256
registers. The collector builtin preserves its pushed result in the caller's
spare slot, as required when a hook observes `collectgarbage('step')` afterward.

| Check | Result |
| --- | --- |
| Rust tests | 553 pass: 72 unit, 477 integration, 4 documentation |
| Formatting and strict Clippy | Pass |
| Production/build dependency tree | Only deadlib-lua |
| Standalone CLI | 181/219 raw; 219/219 after removing one exact known leading version banner |
| Process checks | 140/140 on one unchanged retry; initial 139/140 retained |
| Cache identity checks | 16/16 |
| Official files, C/Rust/cache store/cache hit | 17/18 in each mode; same Windows rename failure |

New differential tests cover eight Lua value kinds with independent strong roots,
source and native imports, stopped and stepped collection, suspended threads
with open upvalues, and 300 object results collected inside instruction hooks.
Fresh child processes store and hit seven valid cache entries, including the
scalar-clearing regression, and execute them again after collection.

The [parent CLI audit](2026-10-26-rust-lua-stack-clearing-parent-audit.json)
uses the preserved 559b20e7a executable, verified against its prior committed
audit. It fails exactly the new scalar-clearing case. The
[current audit](2026-10-26-rust-lua-stack-clearing-audit.json) matches that case
byte for byte. Both retain 38 raw version-banner differences and exit 1.

The [process audit](2026-10-26-rust-lua-stack-clearing-process.json) passes
after one unchanged retry. The [initial result](2026-10-26-rust-lua-stack-clearing-process-timeout.json)
retains a calendar_format_1 CLI timeout while the other two runners completed
the expected Windows CRT abort. No source, deadline or assertion changed.

The [official-suite report](2026-10-26-rust-lua-stack-clearing-suite.json)
retains all eighteen prior file hashes and assertions. Only files.lua:59 fails,
in all four modes, at the Windows open-input rename. The runner retains
`_U=true`, 100,000,000 instructions per entry, 60-second deadlines and its pinned
archive checksum. Cache runs record eighteen stores and eighteen hits.

[Validation metadata](2026-10-26-rust-lua-stack-clearing-validation.json) pins
six complete source maps, isolated build inputs, executables and check logs.
The final ordinary CLI build follows the other builders. Its Cargo target
started empty for the final source revision. The
[identity report](2026-10-26-rust-lua-stack-clearing-identity.json) checks the
unchanged cache wire format's build identity.

This is a correctness milestone. No performance benchmarks ran, and prior
performance reports remain unchanged. Full standalone Lua 5.1, all.lua/main.lua,
complete debug/native scratch behavior, later dialects and gameplay integration
remain unfinished. Compatibility failures and differential coverage take
priority over further performance tuning.
