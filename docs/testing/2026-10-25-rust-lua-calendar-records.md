# Lua calendar table storage validation

Calendar result tables reserve their known record-field count before insertion.
The hash bucket count stays below the existing strict three-quarter rebuild
threshold; entries retain insertion order and all operations use the same table
representation. The eight base fields plus optional `isdst` receive their normal
Lua values. The Windows corpus exercises the nine-field path; the native
unknown-DST eight-field path was not forced.

The VM charges the complete initial payload capacity to its arena allocation
debt and sets the pending collection flag before insertion. Subsequent growth
continues through the existing `raw_set` accounting. No new allocation pool,
object kind, dependency, unsafe code or native API is introduced. Production
consumers retain the legacy crate.

All **341 tests** pass: 327 integration groups, thirteen unit groups and one
documentation example. The added differential group exercises both local and
UTC result tables. It removes each key while traversing with `next`, checks the
sorted key set, reinserts fields, grows through 257 string and 257 integer keys,
uses table keys and a self-cycle, installs a metatable, performs explicit
collections, compares calendar round trips and verifies weak-reference clearing.
Traversal order itself remains unspecified and is not compared across engines.
Existing calendar field callbacks/finalizers, coroutine barriers, numeric/date
bytes, generated calendars, GC pressure, source/compiler and I/O tests pass.

The [93 isolated process probes](2026-10-25-rust-lua-calendar-records-process.json) still match full exit
statuses, stdout/stderr bytes and retained file contents across official Lua
5.1.5, the Rust runner and CLI. This includes invalid native format outcomes and
the three controlled timezone/DST environments from the
[calendar milestone](2026-10-25-rust-lua-calendar.md).

The [official suite report](2026-10-25-rust-lua-calendar-records-suite.json) retains sixteen unchanged,
checksum-pinned files, `_U=true`, 100-million-instruction budgets and sixty-second
file deadlines. Rust passes twelve and the reference fifteen. Rust still stops
at `calls.lua:243` (binary dumps), `literals.lua:161`, `strings.lua:150` and
`files.lua:22` (locale support). The reference retains the Windows open-input
rename failure at `files.lua:59`. The suite command exits unsuccessfully; these
remain isolated probes, not complete `all.lua` or internal C API coverage.

The crate's formatting, strict Clippy with dependencies excluded and whitespace
checks pass. The normal/build dependency tree contains only `deadlib-lua`.
All report source hashes match the final tree.

```sh
cargo test -p deadlib-lua
cargo fmt -p deadlib-lua --check
cargo clippy -p deadlib-lua --all-targets --no-deps -- -D warnings
cargo tree -p deadlib-lua --edges normal,build
python scripts/check_rust_lua_process.py --output docs/testing/2026-10-25-rust-lua-calendar-records-process.json
python scripts/check_rust_lua_suite.py --output docs/testing/2026-10-25-rust-lua-calendar-records-suite.json
```

The [measurements](../performance/2026-10-25-rust-lua-calendar-records.md) include paired parent/current
execution, allocation counts, GC behavior, a field-extension workload and the
48-runtime/twenty-two-frontend reference harness. They preserve the exact-capacity
tradeoff when programs add fields after construction. The validated runtime
contract remains Windows x86-64 MSVC, static CRT, binary64 and C locale; this
milestone adds no cross-platform runtime claim.

A separate follow-up probe confirms that `collectgarbage('collect')` after
`collectgarbage('stop')` restarts automatic collection in the Lua 5.1 reference,
while the independent engine remains stopped. A weak-value table filled with
10,000 temporary tables retains fewer than 10,000 in the reference and all
10,000 here. This existing control-state mismatch needs a compatibility fix;
the paired stopped-GC fixture explicitly stops again after every manual cycle.

Locale switching must coordinate numeric parsing/scanning, byte classification,
casing, collation, formatting and cached patterns. It remains unfinished, along
with binary chunks, native modules, incremental GC, broader diagnostics/resource
semantics, caches, host APIs, Lua 5.2–5.5, optional JIT and song/noteskin integration.
The full standalone-engine goal remains open.
