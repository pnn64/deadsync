# VM-owned mutable native stack

`HostCall::native_stack` now transfers arguments into a mutable, rooted native
register window. Temporary values no longer occupy closure captures. Positive
and negative indexes, nil-filled growth, pop, replacement, numeric reads,
capacity checks and protected nested calls share this window. The bound is
8,000 slots. The stack opens once per callback; dropping it without selecting
results returns no values while retaining the fault-inspection window.

Moving arguments also clears their old owned call temporaries. Otherwise the
suspended caller's GC map retained duplicate values even after the native stack
popped them. Tests verify that discarded arguments become collectible while
caller locals and open upvalues survive. Native frame metadata follows the live
window, so callbacks and error handlers inspect its current values.

Finishing selects the last values as results without compacting the window.
The internal host-call return carries both source position and count. Return
hooks see the full native window and can change the selected result slots before
the VM copies them to the caller. Existing host callbacks retain their ordinary
argument/result writer behavior.

| Check | Result |
| --- | --- |
| Rust tests | 566 pass: 72 unit, 490 integration, 4 documentation |
| New native-stack groups | 11 pass |
| Formatting and strict Clippy | Pass |
| Normal/build dependencies | Only deadlib-lua |
| Shared-source C boundary module | Program stdout/status match official Lua |
| C boundary dispatch cleanup | 1,973 operations and 1,973 returned Drop guards under panic=abort |
| Process checks | 140/140 |
| Standalone CLI | 183/221 raw; 221/221 after one exact leading version-banner removal |
| Library argument matrix | 5,493/5,507; fourteen loadlib differences retained |
| Official files, C/Rust/cache store/cache hit | 17/18 in every mode; same Windows rename failure |

The C boundary now uses `NativeStack` and two actual adapter-pointer captures,
instead of sixty captures containing its temporary roots. Expanded C fixtures
retain ninety-six distinct binary strings across collection, mutate slots and
read numeric strings beyond the original argument range. A return-hook case
inspects arguments and pushed values, changes the first selected result to 99,
and collects before returning. The same C source runs against the official
shared Lua DLL and the Rust development boundary.

Rust tests additionally retain three hundred native temporaries with zero
closure upvalues, release popped objects and original arguments, preserve
prefixes and nil result padding, inspect native fault locals, reject reopening
an emptied stack, enforce capacity checks, preserve fatal resource limits when
ignored, and exercise sorting, generic iterators and metamethod continuations.
The new engine paths use safe Rust and add no production dependency or C shim.
The existing C shim remains development-only.

Raw [boundary](2026-10-26-rust-lua-native-stack-boundary.json),
[process](2026-10-26-rust-lua-native-stack-process.json),
[CLI](2026-10-26-rust-lua-native-stack-cli.json),
[library](2026-10-26-rust-lua-native-stack-library.json) and
[official-suite](2026-10-26-rust-lua-native-stack-suite.json) observations are
retained. The strict CLI audit exits 1 for 38 intentional version banners. The
library audit also exits 1; its fourteen unsupported native-loader results have
not been hidden or reclassified.

All eighteen official file hashes, assertions, bootstrap and execution limits
remain unchanged. Only files.lua:59 fails, in all four modes, on Windows rename
of an open input. Cache modes record eighteen stores and eighteen hits. The
[validation metadata](2026-10-26-rust-lua-native-stack-validation.json) pins five
source maps, native artifacts, isolated build inputs and check logs. A fresh
Cargo target was used for the final Rust source revision. The final ordinary
CLI build followed the other builders; separate reference executables retain
the C library-audit and boundary identities.

This implements storage and native-frame integration, not a complete C API.
The development bridge's C state is still borrowed for each invocation. Its
nested calls use the Rust host API's protected boundary, which unwinds faulting
child frames; full unprotected `lua_call` error-stack behavior remains work.
Persistent per-thread C states, pseudo-index and complete auxiliary APIs,
C closure/userdata/thread operations, library ownership and actual production
`package.loadlib`/`require` integration remain unfinished. Broader debug/scratch
parity, all.lua/main.lua and full standalone Lua 5.1 are still open gates.
Later dialects and gameplay integration remain deferred. No performance
benchmarks ran.
