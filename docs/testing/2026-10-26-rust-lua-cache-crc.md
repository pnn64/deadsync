# Cache CRC validation

The cache CRC32C advances eight input bytes per block through independent table
lookups, followed by a scalar tail. Safe byte-array loads support unaligned
slices. One compile-time static table uses 8,192 B, compared with the parent's
1,024 B lookup table. Polynomial, initial/final complements, footer encoding and
schema version 1 are preserved. Normal Cargo build-identity invalidation still
applies after the implementation changes.

| Check | Result |
| --- | --- |
| Tests | 486: 61 unit, 423 integration, 2 doc |
| Strict Clippy / format | Pass |
| Normal/build dependencies | Only deadlib-lua |
| CLI | 161/199 raw; 199/199 after exact known leading banner removal |
| Process probes | 140/140 across Rust API, C API and CLI |
| Official files: reference, Rust, cache store, cache hit | 17/18 in each mode |
| Official cache outcomes | 18 Stored; 18 fresh-process Hit |
| Build-identity fixture | 16/16 |

The new test compares an independent bitwise polynomial calculation across
8,581 inputs: 16 offsets and every length through 512, larger block/tail
boundaries, uniform binary buffers and all single-byte values. Existing tests
retain known CRC vectors, all truncation points, bit corruption, corrected-CRC
payload mutations, invalid metadata, exact source/name/build/locale guards,
resource limits, concurrent writers, native export and program/VM lifetimes.

Two direct comparisons with committed c47b2d415 check production checksum calls
and the full cache API. Identical benchmark-only forwarding/size functions are
appended in isolated source copies. A shared fixed benchmark build identity lets
the two engines cross-read entries. Each also generates byte-identical cache
files from source and native inputs. This isolates mathematical/wire equivalence;
production build/target/source/locale checks remain active.

All official files retain their previous source SHAs. Each cache entry applies
to the file-entry program; dynamic loads still compile normally. Every mode
fails only files.lua:59, the existing Windows open-input rename failure.
Bootstrap _U=true, 100,000,000 instructions and 60-second per-file deadlines are
retained. No assertions are changed or removed; the suite exits 1 as expected.

The strict CLI audit also exits 1 because 38 cases print the intentional
DeadSync version banner. Only one exact known leading banner is normalized.
The audit builds the normal release executable. Reference archive SHA-256 is
2640fc56a795f29d28ef15e13c34a47e223960b0240e8cb0a82d9b0738695333.
Its binary/source hashes are checked. Platform assumptions remain x64 Windows,
MSVC /O2 /MT, binary64, 64-bit Lua integers, 32-bit native ints and C locale.

~~~sh
cargo test -p deadlib-lua
cargo fmt -p deadlib-lua --check
cargo clippy -p deadlib-lua --all-targets --no-deps -- -D warnings
python scripts/check_rust_lua_cli.py --output docs/testing/2026-10-26-rust-lua-cache-crc-audit.json
python scripts/check_rust_lua_process.py --output docs/testing/2026-10-26-rust-lua-cache-crc-process.json
python scripts/check_rust_lua_suite.py --cached --output docs/testing/2026-10-26-rust-lua-cache-crc-suite.json
python scripts/check_rust_lua_cache_identity.py --output docs/testing/2026-10-26-rust-lua-cache-crc-identity.json
~~~

All ten current-source maps and both archived-parent maps match their inputs.
Two full reference runs preserve all 103 instruction/register layouts; the
five Cargo cache workloads retain their previous layouts and file sizes.
Vm remains 4,432 B and Value 16 B. [Measurements](../performance/2026-10-26-rust-lua-cache-crc.md)
include the added read-only lookup storage, paired allocations/heaps and CPU
counters. Full standalone Lua 5.1, remaining library/debug/resource boundaries,
later dialects, host callbacks, native code and song/noteskin integration remain
unfinished. The full objective stays active.
