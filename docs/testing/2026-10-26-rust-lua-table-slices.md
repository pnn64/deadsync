# Lua 5.1 table propagation validation

Table propagation visits at most 64 physical array/hash slots per work item,
including holes and tombstones. Continuations resume after child edges. Small
mixed tables share one slot budget. Compaction, hash-to-array moves and metatable
changes restart pending scans. Metatable writes retain a single pending scan;
fresh marks discard state left by interrupted cycles.

Each scan snapshots its weak-key/value flags. The final atomic metatable
refresh observes mutations to shared __mode fields before weak cleanup, avoiding
repeated long mode-string reads per slice. A zero-null metatable identity leaves
scan state within the existing 88-byte table header. VM stays 4,432 B and Value
16 B. Production code remains safe Rust without normal/build dependencies.

| Check | Result |
| --- | --- |
| Tests | 472: 50 unit, 421 integration, 1 doc |
| Strict Clippy / format | Pass |
| Normal/build dependencies | Only deadlib-lua |
| Standalone CLI | 161/199 raw; 199/199 after one exact leading known banner removal |
| Process probes | 140/140 across Rust API, C API and CLI |
| Unchanged official files | Rust 17/18; C reference 17/18 |

Seven new collector tests cover array/hash/mixed frontier bounds, physical
hole/tombstone budgets, compaction across cursors, hash-to-array migration,
queued/completed metatable writes, partial weak-mode changes/removal and cycle
restart. Three CLI probes mutate tables during partial collection and compare
final contents, numeric results and identity.

The strict CLI command exits 1: 38 cases expose the intentional DeadSync version
banner. Normalization removes only one exact known leading banner; no other
byte/status differences are suppressed. The audit builds the normal release
executable without a prebuilt override.

The official runner also exits 1. Both engines fail files.lua:59 at a Windows
rename of an open input file. All 18 source SHAs match the previous run and the
pinned archive. No assertions are removed or changed. Bootstrap _U=true,
100,000,000 instructions and 60-second per-file deadlines are retained.
The remaining official-suite gates are unfinished.

The reference is official Lua 5.1.5, archive SHA-256
2640fc56a795f29d28ef15e13c34a47e223960b0240e8cb0a82d9b0738695333,
MSVC /O2 /MT, x64 Windows, binary64, 64-bit Lua integers, 32-bit native ints and C
locale. Reference binary/source hashes are verified. Cross-platform byte parity
is not established.

~~~sh
cargo test -p deadlib-lua
cargo fmt -p deadlib-lua --check
cargo clippy -p deadlib-lua --all-targets --no-deps -- -D warnings
python scripts/check_rust_lua_cli.py --output docs/testing/2026-10-26-rust-lua-table-slices-audit.json
python scripts/check_rust_lua_process.py --output docs/testing/2026-10-26-rust-lua-table-slices-process.json
python scripts/check_rust_lua_suite.py --output docs/testing/2026-10-26-rust-lua-table-slices-suite.json
~~~

All seven current JSON source maps and both archived-parent maps match their
inputs. Two full reference runs retain all 103 instruction/register layouts.
[Measurements](../performance/2026-10-26-rust-lua-table-slices.md) include step latency, cycle cost,
allocation/heap estimates and process CPU counters.

This bounds table slot propagation, not complete collector pauses: root
snapshots, other object scans, __mode reads, final metatable/weak processing,
heap statistics, cache rebuilding and finalizers retain atomic work. Full
standalone Lua 5.1, broader reference/fuzz/resource coverage, later dialects,
caches, host callbacks, native code and song/noteskin integration remain open.
The full objective stays active.
