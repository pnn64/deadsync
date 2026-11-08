# Compiler constant-pool validation

Function constants retain append order and existing typed equality. Small pools
use linear lookup; subsequent requests after 64 entries use a temporary flat
u32 index. Probes compare complete values, including all string bytes. Numeric
equality pools both zero signs and leaves NaNs distinct. The compiler's existing
first-zero-sign selection is preserved. Borrowed string lookups copy bytes only
for a new value. Function finalization releases the index and retains the same
immutable constant array. No runtime or cache-format field is added.

| Check | Result |
| --- | --- |
| Tests | 492: 64 unit, 426 integration, 2 doc |
| Strict Clippy / format | Pass |
| Normal/build dependencies | Only deadlib-lua |
| CLI | 161/199 raw; 199/199 after exact known leading banner removal |
| Process probes | 140/140 across Rust API, C API and CLI |
| Official files: reference, Rust, cache store, cache hit | 17/18 in each mode |
| Official cache outcomes | 18 Stored; 18 fresh-process Hit |
| Build-identity fixture | 16/16 |

Three new private tests compare 12,000 mixed requests against an independent
linear oracle; exercise 196 deliberate hash collisions through wrapped probes
and growth; and verify ownership of borrowed 4,097-byte strings. They check
append IDs, original number bits, infinities, signed zeros, NaNs, binary bytes
and owned/borrowed lookup routes. Three integration groups compare with C Lua
5.1.5: 34 growing pools with both first-zero signs, 200 binary keys/values sharing
prefixes and suffixes, and 64 nested method/capture functions. Integral numbers
exposed as integers by the development binding are observed as binary64 values.
This normalization belongs to the test observer.

Two direct runs compare committed 0bb97ec97 with the final sources. Identical
benchmark-only compiler-size accessors are appended in isolated source copies.
A shared explicit test build identity lets both engines generate byte-identical
complete cache files and cross-read them for all 19 fixtures. Instruction counts,
register counts, numerical result bits and runtime heap observations also match.
Normal Cargo build-identity invalidation remains active in production.

All 18 official files retain their previous source SHAs. Each cached program is
the file-entry program; dynamic loads still compile normally. Every mode fails
only files.lua:59, the existing Windows open-input rename failure. Bootstrap
_U=true, 100,000,000 instructions and 60-second per-file deadlines are retained.
No assertions are changed or removed. The suite exits 1 as expected. Official
archive SHA-256 is
49e4ca6561f82ea605908c5041ab5fad66ed9930fa0686675bd51b02767f18ad.

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
python scripts/check_rust_lua_cli.py --output docs/testing/2026-10-26-rust-lua-constant-pool-audit.json
python scripts/check_rust_lua_process.py --output docs/testing/2026-10-26-rust-lua-constant-pool-process.json
python scripts/check_rust_lua_suite.py --cached --output docs/testing/2026-10-26-rust-lua-constant-pool-suite.json
python scripts/check_rust_lua_cache_identity.py --output docs/testing/2026-10-26-rust-lua-constant-pool-identity.json
~~~

All ten current-source maps and both archived-parent maps match their inputs.
Two full reference runs preserve all 103 instruction/register layouts; five
Cargo cache workloads preserve layouts and file sizes. Vm remains 4,432 B and
Value 16 B. [Measurements](../performance/2026-10-26-rust-lua-constant-pool.md) include temporary allocation
costs, tracked live layout sizes and process CPU counters. Raw
[CLI](2026-10-26-rust-lua-constant-pool-audit.json), [process](2026-10-26-rust-lua-constant-pool-process.json),
[suite](2026-10-26-rust-lua-constant-pool-suite.json) and [identity](2026-10-26-rust-lua-constant-pool-identity.json) results are retained.
Full standalone Lua 5.1, remaining library/debug/resource boundaries, later
dialects, host callbacks, native code and song/noteskin integration remain
unfinished. The full objective stays active.
