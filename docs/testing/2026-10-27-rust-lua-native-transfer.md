# Native thread creation and stack transfer

NativeStack::new_thread now creates and pushes an empty thread sharing the VM's
heap, registry and current thread globals. It checks collection before creation
and inherits hook controls without copying the parent's Lua hook callback.
The development C adapter adds lua_newthread and lua_xmove. The adapter still
belongs to the test harness; production continues to use deadlib-lua-legacy.

Entry functions live on the actual thread stack, replacing the hidden entry
function handle. A fresh empty thread has native status zero and Lua status
dead. Adding an entry changes its Lua status to suspended. The scheduler accepts
Lua and native entries and preserves values below the entry, including their
collector roots and returned prefix. A successfully completed thread can accept
another entry; adding values to a failed thread does not reset its error status.
Values staged above a yielded thread's empty API window remain rooted, while the
next resume supplies the explicit new arguments as in official Lua.

NativeThreadStack provides non-executing access to another live thread's native
window. It supports ordinary and applicable pseudo-indexes, nil-filled growth,
push/replace/insert/remove, scalar and string coercion, raw length, pointer and
thread conversion, and shared registry/thread globals. Borrowing it does not
activate the thread. Transfers preserve order and identities, including nils;
validation and destination reservation precede mutation. A same-thread move
returns immediately, including the C reference's negative/oversized-count cases.
Invalid handles, insufficient source values and destination limits leave the
live stacks intact. C lua_checkstack with a negative request now preserves the
reference's successful no-op behavior.

The collector traces visible inactive windows in addition to live execution
frames and open cells. Popped values stop owning roots; thread self-cycles remain
collectible. Stable string storage survives arena growth and collection while
the value stays on a reachable thread stack. Waiting auxresume frames retain
only their remaining thread argument after transferring the resume arguments.
Return windows place remaining arguments and results together, so return hooks
can inspect and change the actual results through collection. Five tested
failed-thread layouts retain their raw/located diagnostics and object identity.
Other private diagnostic layouts remain outside this milestone's proof.

BoundaryValue carries the target opaque state key. Dispatch continues through
the active C-only error frame, with all Rust borrows and drop guards released
before a jump. Read/write primitives can therefore target an inactive state
without switching execution contexts. C states remain non-owning thread tokens;
the pointer lookup does not root a thread or its stack contents.

The same Windows/MSVC C module and thread-stacks.lua run in both engines. They
compare native/Lua entry calls, reuse after completion, prefixes, nil/object
transfer order, same-thread no-ops, normal/yielded/completed/failed windows,
string coercion and stable payloads, registry/globals identity, creator globals
versus function environments, inherited hooks, weak lifetimes, 8,000-slot bounds
and a thousand transfer cycles. Explicitly named diagnostic chunks allow direct
comparison of the diagnostic strings. All 904 stdout bytes match. The Rust run
balances 70,609 dispatch/drop guards and releases all 2,023 C handles, including
2,022 collected before shutdown.

Six portable Rust groups cover entry/prefix/reuse behavior, transfer roots and
self-cycles, yielded/normal windows, primitive conversions/pseudo-indexes,
atomic rejections/stale handles and failed-thread status. A seventh differential
group checks resume/wrapper return-hook windows and mutations through collection.
Fresh verification passes 692 tests on Windows x64 and 693 on each of Linux x64
and Linux i686 SSE2: 78 unit, four documentation and 610/611 integration tests.
Rustfmt, strict Clippy and the zero normal/build dependency tree pass.

The retained main C fixture still matches 61,019 stdout bytes and balances
141,654 guards, releasing 183 handles (182 collected before shutdown). The
parser-root fixture still matches 470,384 bytes across 1,745 scenarios and 7,772
reader observations. The independent loader stack probe still matches 1,3.
Eighteen imported-name chunks match reference/direct/cache-store/fresh-process
cache-hit modes on every platform. Unchanged stock main.lua and all.lua pass
in both CLIs on Linux i686 SSE2; internal T/testC and suite native modules remain
absent.

Raw [C-boundary](2026-10-27-rust-lua-native-transfer-boundary.json),
[Windows](2026-10-27-rust-lua-native-transfer-windows.json),
[Linux](2026-10-27-rust-lua-native-transfer-linux.json),
[i686](2026-10-27-rust-lua-native-transfer-i686.json) and
[stock-suite](2026-10-27-rust-lua-native-transfer-suite.json) reports retain commands
and observations. [Validation metadata](2026-10-27-rust-lua-native-transfer-validation.json)
binds source snapshots, workspace inputs, logs, ABI headers and private executables.
Final build targets start absent and build from source.

The boundary declares 63 of 80 core APIs; declarations do not establish full
semantics. Inactive operations that execute Lua, native C resume/yield, state
lifecycle, allocator/OOM behavior, broader diagnostic/parser scratch behavior,
remaining C APIs and complete standalone Lua 5.1 parity remain unfinished.
Later dialects and integrations remain open. Optimization and benchmarks stay
deferred.
