# Registered Rust native callbacks

Vm::new_host_function registers a Rust function pointer as a native Lua function.
HostCall borrows its original arguments and writes results separately, preserving
argument values during writes. Numeric native IDs use the existing closure arena
and call frames. Up to 60 copied Lua captures are traced and mutable through the
collector barrier. The context supplies numeric coercion, strings, raw tables,
userdata and exact Lua error values. Userdata inherits the native environment.

Callbacks are synchronous. Invoking Lua or suspending from inside Rust is not
part of this interface. Callbacks return catchable runtime errors through error
or raise; the latter preserves nil, false and object identities. Rust panics
follow the build strategy. An unwind bypasses Lua handlers, marks the VM as
interrupted and disables Lua finalization when that VM is discarded. Abort builds
retain Rust's abort behavior. The existing shutdown flag supplies this state;
no per-VM field is added.

| Check | Result |
| --- | --- |
| Tests | 511: 64 unit, 444 integration, 3 doc |
| New native callback integration groups | 15/15 |
| Additional generated binary replacements | 2,000/2,000 against C Lua 5.1.5 |
| Strict Clippy / format | Pass |
| Normal/build dependencies | Only deadlib-lua |
| CLI | 165/203 raw; 203/203 after exact known leading banner removal |
| Process probes | 140/140 across Rust API, C API and CLI |
| Official files: reference, Rust, cache store, cache hit | 17/18 in each mode |
| Official cache outcomes | 18 Stored; 18 fresh-process Hit |
| Build-identity fixture | 16/16 |

The fifteen groups cover argument/result aliasing, empty and nil results,
numeric strings and native argument errors, captures, coroutine callers, call
and return hooks with collection, arbitrary error identity and recovery,
mutable userdata/native environments, unreachable capture cycles and distinct
registrations, invalid/stale handles, pattern replacements and metamethods,
2,000 incremental capture mutations, native result limits and instruction abort,
collection/shutdown finalizers, preload module caching, independent cached-program
bindings in two VMs, and Rust unwinding with skipped Lua finalization on drop.
The new documentation example registers and invokes a native addition function.

Raw object identities remain scoped to their originating VM; equal numeric IDs
can occur in different VMs. Capture/result validation checks liveness within the
current VM and cannot prove the origin of a raw ID. Root handles check VM ownership.
This makes the existing raw-value precondition explicit rather than promising
cross-VM detection. Captures/results retain Lua objects; Rust host objects,
callback reentry/yield and richer host lifetime facilities remain open.

The generated replacement test independently compares complete results/errors
against C Lua using binary subjects and replacements, captures, position captures,
empty/anchored matches, malformed replacement references and bounded counts.
The earlier 544 format cases remain. A separate exploratory unchanged main.lua
CLI run at baseline 1ae9c93af stopped at its first module-file command in both Windows executables:
temporary filenames contain dots and require replaces them with separators.
An additional loader setup reached the same Windows shell-quoting failure in
both. These experiments do not establish full main.lua passage; that gate stays
open and is not included among the eighteen file-entry probes.

All eighteen official files retain their preceding source hashes. Each mode
fails only files.lua:59, the existing Windows open-input rename failure. The
suite exits 1 as expected. Bootstrap _U=true, 100,000,000 instructions and
60-second per-file deadlines remain. Cached programs are file-entry programs;
dynamic loads compile normally. No assertions are changed or removed. Official
suite archive SHA-256 is
49e4ca6561f82ea605908c5041ab5fad66ed9930fa0686675bd51b02767f18ad.

The strict CLI audit builds the ordinary release executable and exits 1 because
38 cases differ only in the intentional version banner. At most one exact known
leading banner is removed for the additional comparison. Reference archive SHA is
2640fc56a795f29d28ef15e13c34a47e223960b0240e8cb0a82d9b0738695333.
Its binary/source hashes are verified. The byte-exact contract remains x64 Windows,
MSVC /O2 /MT, binary64, 64-bit Lua integers, 32-bit native ints and C locale.

~~~sh
cargo test -p deadlib-lua
cargo fmt -p deadlib-lua --check
cargo clippy -p deadlib-lua --all-targets --no-deps -- -D warnings
python scripts/check_rust_lua_cli.py --output docs/testing/2026-10-26-rust-lua-host-functions-audit.json
python scripts/check_rust_lua_process.py --output docs/testing/2026-10-26-rust-lua-host-functions-process.json
python scripts/check_rust_lua_suite.py --cached --output docs/testing/2026-10-26-rust-lua-host-functions-suite.json
python scripts/check_rust_lua_cache_identity.py --output docs/testing/2026-10-26-rust-lua-host-functions-identity.json
~~~

All ten current-source maps match their inputs. Two full reference runs preserve
all 103 instruction/register layouts; five Cargo cache fixtures preserve layouts
and file sizes. Vm remains 4,432 B and Value 16 B. Two callback runs assert zero
Rust allocation/reallocation requests in all warmed measured Rust paths.
[Measurements](../performance/2026-10-26-rust-lua-host-functions.md) retain the interpreter/C-bridge timing
tradeoffs, estimated heaps and process CPU counters. Raw [CLI](2026-10-26-rust-lua-host-functions-audit.json),
[process](2026-10-26-rust-lua-host-functions-process.json), [suite](2026-10-26-rust-lua-host-functions-suite.json) and
[identity](2026-10-26-rust-lua-host-functions-identity.json) results are retained. Full standalone Lua 5.1,
remaining library/debug/resource gates, later dialects, optional native code and
song/noteskin integration remain unfinished. The full objective stays active.
