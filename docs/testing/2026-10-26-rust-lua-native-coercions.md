# Lua 5.1 native coercion and substitution validation

Native string checks convert numeric arguments in their actual slots, including
format arguments, loader source/names and radix conversion. Numeric tostring
updates its argument when no __tostring handler applies; missing arguments use
the reference's named argument diagnostic. Scheduled native calls produce
results separately from inspected arguments. Pattern, I/O and native iterator
calls that can partially produce results also preserve arguments during errors.
These changes use existing registers/frames and add no VM or Error field.

Gsub reads the current replacement slot at each match and no longer stores an
obsolete replacement root. Replaced functions/tables can collect after their
actual references disappear. Source/pattern strings remain cached as in the C
implementation. Numeric replacements convert only when a match uses them;
maximum-count validation precedes replacement-type validation.

The substitution's native stack contains its arguments, partial buffer strings
and returned values. The pinned Windows build has a 512-byte luaL buffer.
Buffer flushes, value appends and length-based reductions preserve observable
slots. Callback/callable scratch follows persistent stack state. Changed
replacement types use the current native stack rather than a Rust unreachable
branch. Buffer concatenations share the VM's normal concatenation machinery:
Lua metamethods can run, collect, invoke nested substitutions, change slots or
fail. A flat action vector resumes interrupted buffer writes without a recursive
Rust interpreter. It allocates only when such concatenation suspends; queued
string identities participate in tracing, and quota aborts discard their jobs.

All **461 tests** pass: 43 unit, 417 integration and one documentation example.
Twelve new groups cover numeric coercion, multiple results, partial failures,
live replacement dispatch, lazy conversion, old-root collection, buffer slots
at twelve sizes through 40,000 bytes, 96 generated type/buffer argument changes,
metamethods, nested jobs, yields/errors and recovery after an instruction abort.
The original probes reproduced unchanged numeric slots, OOO versus ONN output
and a missing numeric-replacement conversion against the C oracle. Formatting,
strict all-target Clippy, whitespace and normal/build dependency checks pass.
No production dependency, unsafe code or C linkage is added.

The [release CLI audit](2026-10-26-rust-lua-native-coercions-audit.json) adds twelve independent cases. All
new cases match complete exit status, stdout, stderr and file bytes. The full
audit retains **154/192 raw matches**; all thirty-eight differences are the
intentional version banner. Removing at most one exact known leading banner
gives **192/192**. Raw observations remain recorded and the strict command exits 1.

All [140 process probes](2026-10-26-rust-lua-native-coercions-process.json) pass. The eighteen unchanged
[official files](2026-10-26-rust-lua-native-coercions-suite.json) retain their previous checksums and
**17/18 pass in both engines**. Both retain files.lua:59's Windows open-input
rename failure. The suite command exits 1 with _U=true, 100-million-instruction
limits and 60-second deadlines. This is not all.lua, internal testC or a claim
of complete standalone compatibility.

The unmodified official Lua 5.1.5 CLI uses pinned archive SHA-256
2640fc56a795f29d28ef15e13c34a47e223960b0240e8cb0a82d9b0738695333,
MSVC /O2 /MT, binary64, 64-bit Lua integers and 32-bit native ints. Local pinned
definitions were checked in the [string library](https://www.lua.org/source/5.1/lstrlib.c.html),
[base library](https://www.lua.org/source/5.1/lbaselib.c.html) and
[auxiliary buffer library](https://www.lua.org/source/5.1/lauxlib.c.html).
Mutations that invalidate raw C pointers or stack bounds are outside the verified
contract. Wider C scratch behavior, resource/collector timing and terminal/device
semantics remain open.

```sh
cargo test -p deadlib-lua
cargo fmt -p deadlib-lua --check
cargo clippy -p deadlib-lua --all-targets --no-deps -- -D warnings
python scripts/check_rust_lua_cli.py --output docs/testing/2026-10-26-rust-lua-native-coercions-audit.json
python scripts/check_rust_lua_process.py --output docs/testing/2026-10-26-rust-lua-native-coercions-process.json
python scripts/check_rust_lua_suite.py --output docs/testing/2026-10-26-rust-lua-native-coercions-suite.json
```

All five final JSON source-hash maps match the tree. Two [measurement runs](../performance/2026-10-26-rust-lua-native-coercions.md)
cover 81 runtime and 22 frontend workloads. Full Lua 5.1 compatibility, later
dialects, caches, public Rust callbacks, optional machine code and song/noteskin
integration remain unfinished. The full objective stays active.
