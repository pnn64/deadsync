# Lua 5.1 live native argument validation

Sort rereads table/comparator argument slots after callbacks, including changes
from debug.setlocal and default order metamethods. Its job stores pending operand
values and ranges without retaining obsolete table/comparator copies. Replaced
objects can be collected once their actual Lua references disappear. Initial
function validation remains; subsequent replacements use native call semantics,
including nil/default comparison and callable tables/numbers. Table access is
deferred until needed, preserving completion when a changed slot is unused.

Table foreach/foreachi return non-nil callback results before rechecking a mutated
table. Iteration key/value/bound state precedes the callback area, so inserted
callable self arguments and discarded wide native results cannot overwrite it.
Completed callback scratch is discarded before the next traversal step. Native
local inspection/mutation maps the preserved foreach key/value slots.

Operand failures in C frames keep their native origin: failed calls, comparison,
tag loops, invalid next keys and rawset nil/NaN keys do not gain a Lua caller's
source prefix. Equivalent Lua operations retain their source locations. The cold
error path uses existing visible-frame metadata and error-value roots; no frame,
VM-header or Error field is added. Argument/domain diagnostics retain their
separate Lua 5.1 caller-location convention. Missing pcall/xpcall slots now use
reference argument numbering, method handling and inferred names.

All **441 tests** pass: 43 unit, 397 integration and one documentation example.
Eleven additions cover table/comparator replacement, old-root collection,
completed operations after slot changes, callable/default/invalid comparisons,
thirty generated sort stage changes, traversal replacement, metamethod changes,
discarded unpack results, native/Lua error origins and six required-slot cases.
The first live-argument probe reproduced stale behavior against the C oracle.
Formatting, strict all-target Clippy, whitespace checks and the normal/build
dependency tree pass. No production dependency, unsafe code or C link is added.

The [release CLI audit](2026-10-26-rust-lua-native-args-audit.json) adds twelve independent cases
against unmodified official Lua 5.1.5. All new cases match complete process status,
stdout/stderr and file bytes. The full audit retains **124/162 raw matches**;
all thirty-eight differences are intentional version banners. Removing at most
one exact known leading banner from each stderr gives **162/162**. Raw data and
banners remain recorded, and the strict command still exits 1.

All [140 process probes](2026-10-26-rust-lua-native-args-process.json) pass. Eighteen
[official isolated files](2026-10-26-rust-lua-native-args-suite.json) keep their previous checksums:
**17/18 pass in both engines**. Both retain files.lua:59's Windows open-input
rename failure. That command still exits 1 with _U=true, 100-million-instruction
limits and 60-second deadlines. This is not all.lua, internal testC or complete
standalone coverage.

The oracle is pinned to archive SHA-256
2640fc56a795f29d28ef15e13c34a47e223960b0240e8cb0a82d9b0738695333, built
with MSVC /O2 /MT, binary64, 64-bit Lua integers and 32-bit native ints. Definitions
come from the official [table library](https://www.lua.org/source/5.1/ltablib.c.html),
[base library](https://www.lua.org/source/5.1/lbaselib.c.html),
[runtime diagnostics](https://www.lua.org/source/5.1/ldebug.c.html) and
[auxiliary library](https://www.lua.org/source/5.1/lauxlib.c.html). Development
binding patches remain isolated from production.

```sh
cargo test -p deadlib-lua
cargo fmt -p deadlib-lua --check
cargo clippy -p deadlib-lua --all-targets --no-deps -- -D warnings
python scripts/check_rust_lua_cli.py --output docs/testing/2026-10-26-rust-lua-native-args-audit.json
python scripts/check_rust_lua_process.py --output docs/testing/2026-10-26-rust-lua-native-args-process.json
python scripts/check_rust_lua_suite.py --output docs/testing/2026-10-26-rust-lua-native-args-suite.json
```

All final recorded source hashes match the tree. Two [measurement runs](../performance/2026-10-26-rust-lua-native-args.md)
cover 78 runtime and 22 frontend workloads. Mutations are compared where the C
reference has defined behavior; non-table replacements are tested only on paths
that return before raw table access. Complete C scratch-slot behavior, wider
native callback/resource cases, collector timing, console signals/modes, native
modules, later dialects, caches, Rust callbacks, JIT and song/noteskin integration
remain unfinished. The full objective stays open.
