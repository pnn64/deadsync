# External native hook callbacks

`NativeStack::set_hook` and `hook` install/query per-thread native hooks.
`HostCall::hook_event` supplies Lua 5.1 event numbers, lines and activation
records. Delivery does not introduce a visible function frame. The development
C boundary adds `lua_sethook`, `lua_gethook`, `lua_gethookmask` and
`lua_gethookcount`, with a C-only error boundary around external hook calls.

Hook controls retain signed counts, reset the instruction counter and narrow
nonzero masks to a byte after testing for a disabled callback. External hooks
inherit across new threads. Lua debug-library callbacks remain in their registry
table; replacing a native hook preserves that entry, and a saved Lua hook pointer
can be restored. `debug.gethook` identifies installed external hooks.

Native hook stacks alias the interrupted activation, including pushes read or
changed through debug locals. Hidden interpreter storage remains separate and
rooted while nested calls reuse scratch registers. Native bindings and captures
remain rooted during a callback even if it disables itself and collects.
Failures preserve Lua error identity and live fault frames. Reentry shares the
instruction quota and accounts for its actual C-call boundary.

Line/count hooks can yield zero results without retaining Rust/C call stacks.
Pending count/line delivery finishes before suspension, and resumption executes
the interrupted instruction. Additional resume arguments retain their visible
top and collector roots until an instruction normalizes the stack. Math functions,
`assert`, `unpack`, pairs/ipairs and iterator return hooks now expose their native
argument/result windows; terminal ipairs retains its pushed index/nil pair.

The [source-built C comparison](2026-10-27-rust-lua-native-hooks-boundary.json)
runs the same module against official Lua 5.1.5 and the Rust engine under
`panic=abort`. It compares callback identities, controls, events, activation
records, lines, names, stack tops, temporary aliases and actual C-call counters.
It includes callback replacement, restored Lua hooks, error tokens, inheritance,
line/count/mixed yields, reentry with collection, iterator termination and a math
result changed from a native hook. Eleven portable Rust groups additionally
cover stale handles, binding reclamation, interrupted native results, collector
roots and quota-abort recovery.

On Windows x64, all 719 Rust tests pass, with strict Clippy and formatting.
The final C hook probe matches all 7,880 stdout bytes; its 7,998 dispatch/drop
guards balance, and all seven native states are destroyed (six before shutdown).
The earlier native-boundary, parser-root, thread-stack, coroutine, reader-depth
and debug probes also pass in that source-built run. Normal/build dependencies
remain empty.

The unchanged [independent official files](2026-10-27-rust-lua-native-hooks-suite.json)
pass 17/18 in reference, Rust, cold-cache and cache-hit modes on Windows.
`files.lua:59` retains the open-file rename failure in all four modes. The
[stock standalone entry points](2026-10-27-rust-lua-native-hooks-full-suite.json)
also retain unsuccessful exits in both CLIs at `main.lua:53`, through the
`main.lua:42` child-process assertion. These strict runners still exit with
failure; no official file or assertion is changed.

The expanded [first](2026-10-27-rust-lua-native-hooks-extended.json) and
[intermediate](2026-10-27-rust-lua-native-hooks-fixed.json) strict runs retain
temporary-alias and resumed-top mismatches. Their observations directed the
fixes; later success does not erase them. The
[earlier limited probe](2026-10-27-rust-lua-native-hooks-probe.json) covered
controls and event records before these additional observations were included.

The [Rust validation report](2026-10-27-rust-lua-native-hooks-validation.json)
records the full crate test command, strict Clippy, formatting and the empty
normal/build dependency tree, with source hashes and log digests. This milestone
adds four development C declarations (74/80). Full hook semantics, remaining
builtin scratch stacks, state/allocator/OOM APIs, native module loading and full
standalone Lua 5.1 parity remain open. The separate lowered-count yield mismatch
also remains open. Performance work stays deferred.
