# Native collector controls and finalization

`NativeStack::gc` applies typed `GcOperation` commands inside a synchronous
native callback. The development C boundary exposes Lua 5.1's `lua_gc` opcode
interface, including stop, restart, full collection, whole-kibibyte and remainder
counts, steps, pause and step multiplier. Invalid C opcodes return -1 without
changing the stack. Setters return their previous signed control value.

Native and Lua-library controls share the same collector operation path.
Collections and completed steps invoke pending finalizers through the existing
host child-call scheduler. The host continuation is hidden from Lua stack
inspection: the finalizer sees its actual C caller, with its original argument
window. A finalizer can inspect or change those slots, call native functions,
resume coroutines and collect recursively. Successful API calls preserve the
native stack top and return the collector result separately.

Finalizer failures retain the original Lua error value and fault frames for an
outer handler. Subsequent fallible native operations reject that pending error,
and returning success from a Rust callback cannot suppress it. Finalizers use
the enclosing instruction quota; cancellation unwinds their scheduler state and
permits a later VM entry. This path does not reset the instruction budget or
clear the caller's frames as an idle host collection would.

The identical C fixture is built against official Lua 5.1.5 and the development
boundary on Windows/MSVC x64. It checks all opcodes, invalid opcodes, signed
control extremes, stable count parts, collection and full-step completion,
reentrant finalizers, C argument inspection/mutation, arbitrary error identity,
stopped automatic collection after a failing finalizer, coroutine resumes and
forbidden yields. Memory values are checked for consistency within each engine;
absolute memory sizes are not compared across different runtime layouts.

Six new Rust groups check control results and stack preservation; finalizers
that inspect and mutate the true caller; error-frame retention and sticky
errors; instruction-limit recovery; collection of popped argument roots before
finalization; and full-step finalizer execution. All earlier native upvalue,
buffer, userdata and C-only error-boundary cases remain enabled.

The verification covers 646 Rust tests per platform: 77 unit, 565 integration
and 4 documentation tests on Windows x64, Linux x64 and Linux i686 SSE2.
Rustfmt and strict Clippy pass, and the normal/build dependency tree contains
only `deadlib-lua`. Imported-name audits match eighteen native chunks in C,
Rust direct, cache-store and fresh-process cache-hit modes on each platform.
The unchanged stock `main.lua` and `all.lua` pass in both engines on Linux
i686 SSE2. Internal T/testC and native suite modules remain absent.

Raw [C-boundary](2026-10-27-rust-lua-native-gc-boundary.json),
[Windows](2026-10-27-rust-lua-native-gc-windows.json),
[Linux](2026-10-27-rust-lua-native-gc-linux.json),
[i686](2026-10-27-rust-lua-native-gc-i686.json) and
[stock-suite](2026-10-27-rust-lua-native-gc-suite.json) observations are retained.
[Validation metadata](2026-10-27-rust-lua-native-gc-validation.json) binds exact
source snapshots, workspace inputs, commands, logs, ABI headers and private
executable artifacts. Each target starts absent and final binaries are built
from source. Rust dispatch/drop guards and native state lifetimes must balance.

Counts use the existing `HeapStats` live-storage estimate, split into whole
kibibytes and a remainder modulo 1024. Allocator headers and exact bucket
layouts remain excluded. Collector work units, automatic timing and completely
bounded pauses are not equivalent to PUC Lua. The API runs the engine's current
incremental collector; it does not close those resource/GC parity gaps.
Complete C APIs, production native loading, full standalone Lua 5.1 parity,
later dialects and integrations remain unfinished. Performance work stays
deferred, and the legacy engine remains the production runtime.
