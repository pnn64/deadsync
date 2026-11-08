# Persistent Lua program validation

ProgramCache persists the shared compiler representation, including constants,
captures/bindings, collector maps, debug metadata, source pools and retained
native Lua 5.1 words. Loading preserves the normal Program host lifetime and VM
binding rules. The [API and format](../lua-cache.md) describe the private schema,
trusted-directory contract, limits, build/target/locale identity and fallback.

| Check | Result |
| --- | --- |
| Tests | 485: 60 unit, 423 integration, 2 doc |
| Strict Clippy / formatting | Pass |
| Normal/build dependencies | Only deadlib-lua |
| CLI | 161/199 raw; 199/199 after one exact known leading banner removal |
| Process probes | 140/140 across Rust API, C API and CLI |
| Official files: reference / ordinary Rust | 17/18 each |
| Official files: cache store / fresh-process hit | 17/18 each |
| Cache outcomes for all official files | 18 Stored; 18 Hit |
| Isolated build-identity fixture | 16/16 |

Ten new unit tests exercise complete metadata round trips, binary strings and
number bits, native sources/captures/re-export, exact source/name/build/locale
matching, deliberately misindexed entries, all truncation points, single-byte
damage, trailing data, payload mutations with corrected checksums, invalid
instructions/GC/debug/native-tree data, size budgets, I/O fallback, corruption
replacement, concurrent writers and clone/VM-binding lifetime behavior.

Two new integration tests include a helper launched in six fresh child
processes: source/native stores, corresponding hits and repeated syntax errors.
They execute coroutines, closures and collection from the restored code. The
new documentation example also compiles, reloads and executes a disk entry.

The identity fixture runs the actual build script in an isolated copy. Source
and manifest/build-script edits, a new source path, target fields, encoded flags,
debug and optimization settings change the identifier; identical/restored inputs
and CRLF checkout normalization do not. This tests identity generation, not
cross-target execution or cryptographic authentication. Cargo's
[source-directory watch](https://doc.rust-lang.org/cargo/reference/build-scripts.html#change-detection)
provides new-file change detection.

The official suite keeps all 18 source SHAs identical to the preceding run.
Each cache store/hit is a separate process and applies to the file-entry
program; dynamic loads inside those tests still use ordinary compilation.
All four modes fail only files.lua:59, the same Windows open-input rename
failure. No assertion is changed or excluded. Bootstrap _U=true, the
100,000,000-instruction cap and 60-second per-file deadline are retained.
The command exits 1 as expected; this is not full official-suite passage.

The strict CLI audit also exits 1: 38 cases expose the intentional DeadSync
banner. Normalization removes only one exact known leading banner, without
suppressing other bytes or status differences. The normal release runner is
built without a prebuilt override. The official Lua 5.1.5 CLI archive SHA is
2640fc56a795f29d28ef15e13c34a47e223960b0240e8cb0a82d9b0738695333;
reference executable/source hashes are verified. Assumptions remain x64 Windows
MSVC /O2 /MT, binary64, 64-bit Lua integers, 32-bit native ints and C locale.

~~~sh
cargo test -p deadlib-lua
cargo fmt -p deadlib-lua --check
cargo clippy -p deadlib-lua --all-targets --no-deps -- -D warnings
python scripts/check_rust_lua_cli.py --output docs/testing/2026-10-26-rust-lua-program-cache-audit.json
python scripts/check_rust_lua_process.py --output docs/testing/2026-10-26-rust-lua-program-cache-process.json
python scripts/check_rust_lua_suite.py --cached --output docs/testing/2026-10-26-rust-lua-program-cache-suite.json
python scripts/check_rust_lua_cache_identity.py --output docs/testing/2026-10-26-rust-lua-program-cache-identity.json
~~~

All eight current-source maps match the final tree. Both cache benchmark
build-script outputs match their recorded runtime build identities and binary
hashes. Two full reference runs preserve all 103 instruction/register layouts,
the 4,432-byte VM and 16-byte Value. [Measurements](../performance/2026-10-26-rust-lua-program-cache.md)
include disk reload/store costs, allocations, heap estimates and process CPU.

Directory eviction, a memory cache, package/CLI cache integration and broader
format/resource coverage remain open. Full arbitrary standalone Lua 5.1,
remaining libraries/diagnostics/native boundaries, later dialects, Rust host
callbacks, native machine code and song/noteskin integration remain unfinished.
The full objective stays active.
