# Native C-call count transfer and recovery

NativeStack::copy_thread_call_level implements the counter transfer used by
Lua 5.1's lua_setlevel. NativeStack::thread_call_level reads a live thread's
counter for host inspection. Copying validates both identities before mutation,
preserves stack contents and statuses, and reaches current, waiting, suspended,
completed and failed threads. The development C boundary adds lua_setlevel;
its fixture reads the source-built reference's lua_State.nCcalls directly.
This is compatibility work. Production still uses deadlib-lua-legacy.

The counter retains Lua 5.1's unsigned 16-bit wrapping behavior. Each thread
stores its inherited count and a counter offset; active parser/decoder bases
follow offset changes. Protected frames save the entry offset and restore it
on errors. Successful calls retain counter changes. Native text syntax failures
restore the loader's entry count, including nested parser scopes. Tail calls
preserve the enclosing execution's error checkpoint. Each call frame stores a
new u16 checkpoint; its x64 Rust layout changes from 64 to 72 bytes. Collector
storage accounting uses the new layout. No performance measurements ran.

Resume keeps the final inherited count, including uncaught nested calls and
wraps, and returns native error-handler status 5 when appropriate. Fresh native
resume preserves the target's inherited count; Lua coroutine.resume copies its
caller's count before entry. The 199/200 boundary, overflow rejection at 201,
unchanged status on a rejected resume, and recovery after a valid replacement
entry are checked. Raising an active count above its resume boundary rejects a
yield. Native-triggered finalization now counts its Lua call once, correcting a
double count in the previous frame-derived model. Call and return hooks exclude
completed native calls from the counter.

The new C counter trace matches 7,239 stdout bytes, including text reader
changes, syntax-error recovery, twelve fragmented binary-reader cases, nested
loaders, waiting parser owners, active counter wrapping, protected success and
failure, raw error-handler failures, hooks, native/Lua finalization and 1,000
mixed native/Lua resume cycles. All 94,234 Rust dispatch guards return before a
C jump. All 1,076 states are destroyed; 1,075 are collected during execution.
The earlier main boundary, parser-root, transfer and resume traces remain
independent matching checks.

A separate strict audit retains a mismatch. After lua_setlevel lowers the
active count, a protected call that attempts to yield reports these results:

| Observation | Official Lua 5.1.5 | Rust engine |
| --- | --- | --- |
| lua_resume result / lua_status | 1 / 1 | 0 / 0 |
| Public stack value types | number, number | boolean, string |

The Rust engine still enforces its structural C-call boundary in this case.
The [strict yield audit](2026-10-27-rust-lua-native-level-yield.json) exits 1;
there is no semantic allowlist turning that difference into a pass. Source,
artifact and state/drop checks remain successful. Counter transfer is not a
claim of complete lua_setlevel/coroutine parity. This remains compatibility
work before any performance work resumes.

Five added portable groups check stack preservation, active/waiting counters,
wrapping, protected and tail-call recovery, the resume limit and error-handler
status, text/binary parser restoration, GC/hook counts and stale identities.
Fresh isolated runs pass 702 Rust tests on Windows x64 and 703 on Linux x64 and
i686 SSE2. Formatting, strict Clippy and the empty normal/build dependency tree
pass. Eighteen imported-name chunks match reference/direct/cache-store/cache-hit
modes on each platform. Unchanged stock main.lua and all.lua pass in both CLIs
on i686 SSE2. The separate lowered-count yield audit remains a failure.

Raw [boundary](2026-10-27-rust-lua-native-level-boundary.json),
[Windows](2026-10-27-rust-lua-native-level-windows.json),
[Linux](2026-10-27-rust-lua-native-level-linux.json),
[i686](2026-10-27-rust-lua-native-level-i686.json),
[stock-suite](2026-10-27-rust-lua-native-level-suite.json) and
[validation](2026-10-27-rust-lua-native-level-validation.json) reports bind the
observations, logs, source-built artifacts, ABI headers and compiler inputs.
The build snapshot uses committed workspace inputs and excludes unrelated
uncommitted dependency updates; final target directories start absent.
Removing an extra final blank line from the C-only Lua fixture left every Rust
source byte unchanged. The boundary proof, strict yield audit and Windows
reference/cache audit were rerun and repinned; metadata records both fixture hashes.

The boundary declares 66 of 80 core APIs. That count does not establish full
semantics. The lowered-count yield mismatch, remaining native hook/debug APIs,
state lifecycle, allocator/OOM behavior, native module loading, the retained
fourteen package.loadlib differences, broader GC/resource and diagnostic behavior,
and full standalone Lua 5.1 parity remain open. T/testC and suite native modules
remain absent. Later dialects and gameplay integration remain unfinished.
Optimization, profiling and benchmarks stay deferred.
