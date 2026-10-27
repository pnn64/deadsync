# Native table operations use the shared VM scheduler

`NativeStack` now creates tables with array/record hints and supports raw or
metamethod-aware reads and writes, field and integer-key operations, actual
metatable access, traversal and ordinary stack insertion/removal. The C fixture
adds sixteen ABI operations, including `lua_createtable`, the get/set families,
metatable functions, `lua_next`, type/raw-equality checks and stack movement.

Metamethod-aware operations use the existing VM lookup and assignment logic.
A hidden host action retains inputs and drives the same child-call scheduler
used by native calls. Reads select one result; writes discard results. No
Lua wrapper function is introduced. Child coroutines share the quota and
scheduler, and yielding across the native boundary remains an error.

Failed operations retain the original native inputs and failing Lua frames for
outer error handlers. A handler can inspect and mutate open Lua locals before
protected recovery closes their cells. Field names are rooted internal
temporaries and do not appear as extra native stack values. Pending errors
cannot be swallowed by later result writes. Raw writes and metatable changes
use the existing collector barriers. Table creation reserves storage before
publishing the object; impossible hints return an error without changing the
stack. This does not make all VM allocation failures recoverable.

The initial adapter counted the native caller and its table metamethod
continuation as two C boundaries. Its retained
[report](2026-10-26-rust-lua-native-tables-depth-before.json) records 98 recursive
calls versus 196 in official Lua. The exact
[fixture](2026-10-26-rust-lua-native-tables-depth-before.lua) and
[source patch](2026-10-26-rust-lua-native-tables-depth-before.patch) reconstruct
that initial source revision from `2936d0bf1`, allowing checkout line endings.
The report pins the initial runner and native artifacts.

The result continuation now shares its caller's C boundary. Depth comparisons
also exposed a reference-context mismatch: the Rust runner used embedded entry
while its reference used the standalone CLI. The runner now begins/ends the
standalone context, and depth accounting excludes the outer main already
represented by the implicit root-call convention. Both engines reach 196
ordinary table calls and 219 calls while handling overflow, then return the same
errors. Rust tests separately check embedded entry at 197/220. The development
runner uses a 16 MiB host stack to exercise these logical limits without first
exhausting its Rust/C stack.

| Check | Result |
| --- | --- |
| Rust tests | 597 pass: 73 unit, 520 integration, 4 documentation |
| New native-table groups | 10 pass |
| Formatting and strict Clippy | Pass |
| Normal/build dependencies | Only deadlib-lua |
| Shared-source C module | Program stdout/status match official Lua |
| C dispatch cleanup | 3,386 operations, 3,386 returned Drop guards; panic=abort |
| C state lifetime | 70 created, 69 collected before shutdown, 70 destroyed |
| Recursive table calls | 196 ordinary; 219 during overflow handling, both engines |
| Cache identity checks | 16/16 |
| Process checks | 140/140 |
| Standalone CLI | 183/221 raw; 221/221 after one exact leading banner removal |
| Library argument matrix | 5,493/5,507; fourteen loadlib differences retained |
| Official files, C/Rust/cache store/cache hit | 17/18 in every mode; same Windows rename failure |

The C fixture builds a table containing a real C closure and exercises raw
access, proxy chains, indexing/assignment callbacks, collection, protected
metatables, traversal, stack movement, fault-local inspection, field privacy,
nil/NaN keys, invalid traversal keys, recursion reserves and yield rejection.
The prior closure, persistent-state, error-identity, hook and finalizer cases
remain. Identical C module source runs against unchanged official Lua 5.1.5
and the Rust development boundary; no C Lua implementation executes Rust cases.

Raw [boundary](2026-10-26-rust-lua-native-tables-boundary.json),
[cache identity](2026-10-26-rust-lua-native-tables-identity.json),
[process](2026-10-26-rust-lua-native-tables-process.json),
[CLI](2026-10-26-rust-lua-native-tables-cli.json),
[library](2026-10-26-rust-lua-native-tables-library.json) and
[official-suite](2026-10-26-rust-lua-native-tables-suite.json) observations are
retained. Strict CLI and library audits still exit 1 for the documented banner
and native-loader differences. The official suite keeps all eighteen file
hashes, assertions, bootstrap and limits. Only files.lua:59 fails in each mode,
on Windows rename of an open input. Cache modes record eighteen stores and
eighteen hits. [Validation metadata](2026-10-26-rust-lua-native-tables-validation.json)
pins sources, artifacts, workspace inputs, logs and the initial reconstruction.
A fresh Cargo target built the final source revision. Separate retained
reference executables preserve the library-audit and shared-DLL identities.

The final checks ran after Cargo cleanup recovered space from disposable
intermediate builds. Completed milestone artifacts and the initial comparison
binaries remain intact. Production engine changes use safe Rust and add no
dependency or C shim. The C bridge remains a development fixture for its active
thread. Complete auxiliary/conversion/comparison APIs, inactive/cross-thread
access, C userdata/thread operations, production library caching/unload and
actual `package.loadlib`/`require` integration remain unfinished. Broader native
debug/scratch/GC behavior, all.lua/main.lua and full standalone Lua 5.1 remain
open gates. Later dialects and gameplay integration stay deferred.
No performance benchmarks ran.
