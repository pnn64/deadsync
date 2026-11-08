# Objects and raw access on inactive native stacks

`NativeThreadStack` now creates tables, userdata and host closures; supports raw
table lookup/assignment/traversal and metatables; and reads/writes object
environments and indexed closure upvalues. The runtime C adapter routes the
corresponding APIs to the selected thread, preserving its stack prefix and
execution status. No new core declarations are added: coverage remains 75/80.

These operations follow the corresponding [Lua 5.1 C API paths](https://www.lua.org/source/5.1/lapi.c.html).
Closures and userdata inherit the target thread's current calling environment,
or its globals when there is no call. They do not inherit the currently executing
helper's environment. Environment changes and raw table/capture writes use the
ordinary VM collector barriers. Active and inactive closure APIs now share
capture lookup, names, cell reads and writes. Open Lua cells can belong to any
thread; writes reach the existing cell rather than a copied value.

The [preserved previous CLI comparison](2026-10-28-rust-lua-inactive-objects-before.json)
uses the preceding source-built binary, with its digest checked against that
milestone's argument audit. It fails at the first inactive table creation with
`inactive operation requires execution support`. The same public-header module
and Lua fixture complete under the official reference.

The [current source-built comparison](2026-10-28-rust-lua-inactive-objects-boundary-first.json)
adds the shared C fixture to the existing strict boundary corpus. Its 17
observations cover fresh, completed, yielded, failed and waiting threads across
four allocation/collection rounds. Both engines match all 1,071 stdout bytes.
The new Rust process balances 1,991 dispatches, destroys all fifteen C states and
closes its one native library reference under `panic=abort`.

The fixture exercises raw string/integer keys, traversal, throwing metatables
that must not execute, metatable removal/replacement, userdata payloads and
environment identity, C function identity, C and open Lua capture reads/writes,
invalid capture numbers and unsupported environment targets. Fresh threads use
distinct globals, and a waiting C function has a distinct environment, checking
allocation context independently of the helper. Two collections run while the
new table, userdata and C closure are rooted only on the inactive stack. After
moving them back, they remain callable/readable across further collections;
yielded and fresh threads can still resume normally.

The public-header module integration runs this same fixture through the default
adapter on Windows x64, Linux x64 and Linux i686 SSE2. A portable Rust group
checks rejected table/userdata capacities, capture underflow/overflow, nil keys,
invalid environments/metatables and invalid capture numbers without consuming
the stack inputs. It also collects while a closure is rooted only through an
inactive thread, then reads the updated capture.

The [platform validation](2026-10-28-rust-lua-inactive-objects-validation.json)
passes 760 tests on Windows and 761 on each Linux target, plus strict Clippy and
all-targets builds with default features disabled. Formatting and the empty
normal/build dependency tree pass. Source and log hashes are retained. All
twenty-one Rust boundary processes pass their comparisons and balance cleanup:
723,400 dispatches return, 5,853 C states are destroyed and 31 loader references
close.

The [complete library argument matrix](2026-10-28-rust-lua-inactive-objects-stdlib.json)
still matches all 5,507 cases, with successful reference/Rust processes and empty
stderr. The [native-enabled i686 SSE2 stock suite](2026-10-28-rust-lua-inactive-objects-i686-suite.json)
passes unchanged `main.lua` and `all.lua` in both actual CLIs, with verified
32-bit ABIs and no native-library skip. T/testC remains absent. The
[Windows independent probes](2026-10-28-rust-lua-inactive-objects-windows-suite.json)
retain the shared `files.lua:59` open-file rename failure; their strict runner
still fails, with 17/18 passes in reference, Rust and both cache modes. The
[evidence manifest](2026-10-28-rust-lua-inactive-objects-evidence.json)
binds these reports and their cleanup counters to current source and log hashes.

This expands non-executing access to other threads. Metamethod operations,
calls, loading and other executing APIs on inactive threads remain unsupported.
Complete unprotected error routing and panic behavior also remain open. The five
lifecycle/allocator functions, foreign payload/FILE pointer behavior, native
resource/OOM windows, parser-internal mutation, broader debug/hook/scratch-stack
behavior and the lowered-call-count yield mismatch remain unfinished. The
earlier [unrooted search-name mismatch](2026-10-27-rust-lua-library-loading-first.json)
is preserved; a current match does not establish stable behavior for that input.
Full standalone Lua 5.1 parity remains the goal. Performance work stays deferred.
