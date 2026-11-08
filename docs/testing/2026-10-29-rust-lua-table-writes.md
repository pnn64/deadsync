# Recoverable table writes

Table array growth now reserves space for the inserted value and every
consecutive hash entry it will migrate before changing the array. Hash insertion
reserves entry and index buffers before removing tombstones, resizing indexes or
publishing a new key. Failed reservations preserve existing values, traversal
positions and pending scan state. Compaction can reuse tombstone storage while
the allocator rejects every request. Updates and deletions need no allocation.

VM writes validate the key and reserve collector work for both new strong edges
before mutation. Duplicate edges share one queue entry; weak edges require no
strong tracing entry. Allocation failure returns the fixed Lua memory diagnostic
without adding a source location. If an earlier buffer reservation succeeded,
its retained table capacity is accounted even when a later reservation fails.

The [optimized fault runs](2026-10-29-rust-lua-table-writes-faults.json) pass 99
rejection cases on each of Windows x64, Linux x64 and Linux i686, including 52
cases for table writes. These tests reject real allocator calls persistently
through the probed operation. They cover empty and growing hashes, numeric-entry
migration, collector edge reservation and retained-capacity accounting. Main and
selected native writes preserve their input values after failure. A native
protected call returns status 4 and skips an invalid error function. Lua `pcall`
and `xpcall` recover while rejection remains active through error handling;
`xpcall` does not invoke its handler. Existing storage-failure cases remain in
that 99-case gate. The allocator hook is confined to test binaries.

[Full validation](2026-10-29-rust-lua-table-writes-validation.json) passes 858
Windows tests and 859 on each Linux target, strict Clippy, formatting and
all-target checks without default features. The
[library audit](2026-10-29-rust-lua-table-writes-stdlib.json) matches all 5,507
cases. The [native audit](2026-10-29-rust-lua-table-writes-boundary-current.json)
passes all observation flags for 42 normal and 25 expected termination probes,
with balanced dispatcher, state and library lifetimes.

The final native audit initially found an unstable reference result in the live
package-path stack probe. The probe replaced the only Lua reference to a
translated string and forced collection while `findfile` retained its C pointer.
The [Lua 5.1 string-pointer contract](https://www.lua.org/manual/5.1/manual.html#lua_tolstring)
does not guarantee that pointer after its stack value is removed. The
[Linux AddressSanitizer control](2026-10-29-rust-lua-table-writes-reference-lifetime.json)
reproduces a heap use-after-free in unmodified Lua 5.1.5 `findfile`/`luaL_gsub`.
Retaining the string makes the reference control and Rust C-CLI control pass.
The negative control and original Windows discrepancy remain in the evidence.
The comparison fixture now retains the translated strings across its same stack
mutations and full collection. Its audit passes. The
[lifetime verifier](2026-10-29-rust-lua-table-writes-reference-lifetime-verification.json)
checks pinned reference sources, sanitizer linkage, source/artifact hashes and
both successful controls. The evidence verifier now checks every nested native
observation/completion flag, including the one that exposed this discrepancy.

[Windows debug](2026-10-29-rust-lua-table-writes-debug.json),
[Windows release](2026-10-29-rust-lua-table-writes-release.json) and
[Linux i686 release](2026-10-29-rust-lua-table-writes-i686.json) retain all 492
existing C-host, allocation and table-limit observations across the three
profiles, plus 18 expected Rust panic exits. All six i686 stock C-CLI suite runs
pass with official native modules and without T/testC. The
[i686 verifier](2026-10-29-rust-lua-table-writes-i686-verification.json) checks
sources, artifacts, ELF32 binaries, forwarding and module builds.
The [evidence manifest](2026-10-29-rust-lua-table-writes-evidence.json) binds these
reports and their proof tools to the current source.

This covers reservations for each primitive table write. Compiler/string
storage, closures and other collector/control buffers still contain infallible
allocations. Whole-interpreter recovery from persistent memory exhaustion,
custom allocator ownership, exact heap/GC accounting and broader resource and
continuation parity remain open. Core coverage is still 77/80: `lua_newstate`,
`lua_getallocf` and `lua_setallocf` are absent. Full Lua 5.1 parity is unfinished.
