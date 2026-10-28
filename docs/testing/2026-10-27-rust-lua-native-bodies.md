# Native function implementation pointers

The development C boundary now implements `lua_tocfunction`. Foreign C closures
return their real function pointer. Builtins return stable C adapter functions,
including iterator implementations. Shared implementations preserve pointer
identity: `next` and the `pairs` iterator, and `io.close` and the file `close`
method, compare equal even when their closure environments differ. Lua functions
and non-functions return null.

`NativeBuiltinBody` represents an implementation independently of its source
closure. Calling it through `NativeStack::call_builtin_body` executes in the
current activation, with the current captures, environment and mutable native
stack. Nested Lua calls see that activation's original function. No additional
visible frame or call/return hook is introduced. Yielding returns through the
native callback before suspending the thread. Body handles do not retain the
original closure or its environment.

Ordinary native body calls use an owned argument window. This keeps library
scratch separate from the caller's named stack slots, and moved native error
function indexes follow their values. External hook calls retain their interrupted
stack aliases and current Lua or native function identity. The string dumper now
uses its own scratch instead of writing below its input window.

The shared close implementation exposed another library correction. With no
argument, a free file `close` method fetches its environment's implicit default
output slot. That slot is nil in the method's ordinary environment, so it reports
`FILE* expected, got nil`, matching the reference. The copied body follows the
same rule.

The [source-built C comparison](2026-10-27-rust-lua-native-bodies-boundary-current.json)
builds the same module against official Lua 5.1.5 and the Rust boundary. It checks
pointer aliases, null results, metadata reads on an inactive thread, direct calls,
copied captures and environments, protected errors, coroutine resume/wrap/yield,
native and Lua hook contexts, and collection of an original closure's environment
while its former body is executing. All 543 copied-body observations from the
base/string and table/debug/I/O/OS fixtures match, including first/last-slot
writes and collection. Earlier native-boundary probes also pass. The
[evidence manifest](2026-10-27-rust-lua-native-bodies-evidence.json) binds reports
to digests and source hashes. The copied-body fixtures match all 82,913 stdout
bytes; together with the focused body fixture, all 31,755 dispatch/drop guards
balance under `panic=abort` and all 555 states are destroyed. The manifest also
verifies all sixteen Rust runs' dispatch and state counters.

The [first comparison](2026-10-27-rust-lua-native-bodies-first.json) retains the
initial copied-body failures. A later
[development comparison](2026-10-27-rust-lua-native-bodies-development.json)
matches program observations but intentionally fails the release guard because
its prebuilt debug runner uses `panic=unwind`. It is not the release proof.

Eight portable regression groups cover complete stacks, result counts, caller
identity, hook events, copied captures/environments, coroutine suspension,
instruction-abort recovery, collector lifetime, external hooks and implicit close
arguments. The [platform validation](2026-10-27-rust-lua-native-bodies-validation.json)
records Windows, Linux x64 and i686 SSE2 tests, strict Clippy, formatting and the
normal/build dependency tree, with unchanged source hashes. There are 742 passing
tests on Windows and 743 on each Linux target; normal/build dependencies remain
empty.

The [i686 SSE2 stock suite](2026-10-27-rust-lua-native-bodies-i686-suite.json)
runs unchanged `main.lua` and `all.lua` through both actual CLIs, including ABI
checks. Stock mode still omits T/testC and native suite libraries. The
[Windows probes](2026-10-27-rust-lua-native-bodies-windows-suite.json)
retain 17/18 passes and the shared `files.lua:59` open-file rename failure in all
four modes. Both stock entry points pass in both i686 CLIs.

The boundary declares 75 of 80 core APIs. The remaining declarations are
`lua_atpanic`, `lua_close`, `lua_getallocf`, `lua_newstate` and `lua_setallocf`.
Declaration counts and these comparisons do not establish complete C API
semantics. Arbitrary Rust host callbacks have no foreign C binding to export;
direct builtin execution on an inactive state, broader error-time windows,
native file/resource/allocator/OOM behavior, parser-internal mutation, production
`package.loadlib`/`require` integration and the retained lowered-call-count yield
mismatch remain open. Full Lua 5.1 parity remains the goal; performance work stays
deferred.
