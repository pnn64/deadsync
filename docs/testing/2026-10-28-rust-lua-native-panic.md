# Native panic callbacks and idle thread errors

The C adapter now exports `lua_atpanic`. Threads in one VM share the callback,
registration returns the preceding function pointer, and separate VMs retain
independent callbacks. An unprotected `lua_error` on an idle other thread now
sets its error status and invokes the registered panic callback. This closes
one missing API declaration and one error path; full Lua 5.1 parity remains
the goal.

The [preserved preceding release](2026-10-28-rust-lua-native-panic-missing-before.json)
cannot resolve `lua_atpanic` from its bundled C image. Its saved CLI and image
digests match the preceding reader-continuation checkpoint. The new adapter
declares 76 of the 80 core APIs. `lua_close`, `lua_getallocf`, `lua_newstate` and
`lua_setallocf` remain absent, and declaration coverage does not establish
complete semantics for the other APIs.

The panic callback belongs to a C metadata group shared by the VM's opaque
thread tokens. The tokens keep the VM's existing pointer identities. Groups
and member records are synchronized across independent host threads, counted
in native heap accounting and released when the final member is destroyed.
Public-header tests exercise callback isolation in eight parallel VMs and
repeat panic recovery and cleanup across three separate VMs.

For an idle target with a callback, error handling closes open upvalues,
discards pending frames, restores the base stack with the original error at
index 1, resets transient execution state and retains the installed hook.
Without a callback, Lua 5.1 changes the status and exits without resetting the
stack; a Rust unit test checks this distinction before process termination.
Ordinary errors on the current protected activation still reach `pcall`.

The Rust dispatcher returns before C invokes the panic callback. A callback
can recover through a C-local `longjmp` below the enclosing Rust invocation.
The fixture's jump crosses only C frames. Returning from the callback, or
having no callback, terminates the process with exit code 1. The adapter's
initial callback matches the auxiliary library's standard panic message for
the tested standalone VM setup; raw `lua_newstate` lifecycle behavior remains
unimplemented.

The [native boundary comparison](2026-10-28-rust-lua-native-panic-boundary-current.json)
and [public-header release comparison](2026-10-28-rust-lua-native-panic-current.json)
match all 54 observations against Lua 5.1.5. They cover shared registration,
five target states, two callbacks, table/string/number/boolean/userdata errors,
an explicit nil error, callback replacement, caller/target status, stack reset,
thread identity, closed upvalues and retained hooks. The targets include fresh,
completed, yielded, failed and suspended C activations. The C fixture reserves
its required stack space. The
[assertion-enabled reference](2026-10-28-rust-lua-native-panic-checked.json)
also completes all 54 observations with identical output and no assertions.

Three additional subprocess comparisons in the boundary harness and
[release CLI audit](2026-10-28-rust-lua-native-panic-exits-current.json) require
exact exit codes and output bytes. The default callback prints the standard
panic diagnostic; a cleared
callback exits silently; a returning callback observes status 2 and a one-value
stack before the process exits. These expected exit-1 processes are separate
from the successful recovery processes and their cleanup totals.

The [validation report](2026-10-28-rust-lua-native-panic-validation.json) passes
772 tests on Windows x64 and 773 on Linux x64 and i686 SSE2, strict Clippy,
formatting, builds without default features and an empty normal/build dependency
tree. The [argument audit](2026-10-28-rust-lua-native-panic-stdlib.json),
[native-enabled i686 suite](2026-10-28-rust-lua-native-panic-i686-suite.json) and
[Windows independent probes](2026-10-28-rust-lua-native-panic-windows-suite.json)
retain the broader compatibility checks. Source, binary, artifact and log
digests are bound in the [evidence manifest](2026-10-28-rust-lua-native-panic-evidence.json).
The unchanged `main.lua` and `all.lua` pass in both native-enabled i686 CLIs,
with T/testC absent. Windows retains 17/18 passes in all four independent modes,
the shared `files.lua:59` rename failure and the strict checker's exit code 1.
All 26 successful Rust boundary processes balance 748,521 dispatch returns,
destroy 6,205 C states and close 36 native loader references. Each runner also
asserts that its shared metadata groups are released. These totals exclude the
three deliberately terminating panic processes.

Protected error routing into a waiting or live other thread, arbitrary
unprotected calls, other inactive operations that execute Lua, allocator/OOM
behavior and complete embedding lifecycle remain open. The earlier strict
reader-resume failures, checked-continuation invariant failures, lowered C-call
count yield mismatch and unrooted search-name reference mismatch remain
recorded. This checkpoint does not establish full standalone Lua 5.1 parity.
