# Lua 5.1 string interning validation

String interning hashes short slices through bounded word loads and longer
slices through four independent lanes per 32-byte block. Remaining words and
tail bytes participate before the final integer mix. All loads use safe Rust
array copies; no alignment or CPU-feature precondition is added. Every input
byte participates and full byte equality resolves hash collisions. String IDs,
storage, allocation rules, GC backshift deletion and bucket growth retain their
existing representation; no per-string/VM field or production dependency is added.

All **465 tests** pass: 43 unit, 421 integration and one documentation example.
Four new groups check arbitrary binary bytes and differing middle positions,
short/word/block boundaries through 65,536 bytes, 4,128 unaligned input slices
against an independent content map, and 2,048 table keys through growth,
collection, removal and object-slot reuse. A Lua script compares complete binary
observations with the C reference. Existing collector tests also exercise wrapped
probe clusters and reinterned strings. Formatting, strict all-target Clippy,
whitespace and normal/build dependency checks pass. Production remains safe Rust
with no C link and no normal/build dependencies.

The [release CLI audit](2026-10-26-rust-lua-intern-hash-audit.json) adds four isolated cases covering
binary middle changes, word boundaries, growth/sweeping and repeated long strings.
All four match complete status, stdout/stderr and file bytes. The full audit has
**158/196 raw matches**; all thirty-eight differences are the known version
banner. Removing at most one exact known leading banner gives **196/196**.
Raw data is retained and the strict command still exits 1.

All [140 process probes](2026-10-26-rust-lua-intern-hash-process.json) pass. The eighteen unchanged
[official files](2026-10-26-rust-lua-intern-hash-suite.json) keep their previous source hashes:
**17/18 pass in both engines**. Both retain files.lua:59's Windows open-input
rename failure. The command exits 1 with _U=true, 100-million-instruction limits
and 60-second deadlines. This is not all.lua/internal testC or full compatibility.

The unmodified Lua 5.1.5 CLI reference is pinned to archive SHA-256
2640fc56a795f29d28ef15e13c34a47e223960b0240e8cb0a82d9b0738695333,
MSVC /O2 /MT, binary64, 64-bit Lua integers, 32-bit native ints and C locale.
The [measurement report](../performance/2026-10-26-rust-lua-intern-hash.md) includes two comparisons
with committed parent 2a4dbb99c, process CPU/cycle counters, two C-reference runs,
allocation/heap observations and the denied CPU-sampling attempt.

```sh
cargo test -p deadlib-lua
cargo fmt -p deadlib-lua --check
cargo clippy -p deadlib-lua --all-targets --no-deps -- -D warnings
python scripts/check_rust_lua_cli.py --output docs/testing/2026-10-26-rust-lua-intern-hash-audit.json
python scripts/check_rust_lua_process.py --output docs/testing/2026-10-26-rust-lua-intern-hash-process.json
python scripts/check_rust_lua_suite.py --output docs/testing/2026-10-26-rust-lua-intern-hash-suite.json
```

All seven current-source JSON hash maps match the final tree. Both comparisons
also verify their archived parent inputs against its Git commit. All 103 existing
instruction/register layouts, the 4,432-byte VM and 16-byte Value remain unchanged.
Native/collector/resource parity, full standalone Lua 5.1, later dialects,
caches, Rust callbacks, optional machine code and song/noteskin integration
remain unfinished. The full objective stays active.
