# Native file loading through another thread

`NativeStack::load_file_thread` and the C adapter's `luaL_loadfile` now load
through a selected thread's native stack. Named files, shared stdin, file
ownership, source names and parser/error statuses use the existing file loader.
Loading preserves the target's suspended computation and retained status; new
functions receive that thread's globals. Full Lua 5.1 parity remains the goal.

The [preceding release comparison](2026-10-28-rust-lua-thread-files-before.json)
finishes all 53 reference observations but fails on the first Rust call with
`inactive operation requires execution support`. The saved CLI and SDK digests
match the preceding native-panic checkpoint.

Reader and file loading now share thread selection and restoration. The loader
itself owns the final visible base. A file-open failure never enters the parser,
so it retains a yielded C result suffix. A parser error restores the current
CallInfo base instead. The
[first comparison](2026-10-28-rust-lua-thread-files-focused-first.json) records
two extra values incorrectly exposed on the open-error path before this fix.

An error in filename-push collection occurs before the parser's protection.
For idle targets, the adapter now retains that original error on the target,
returns to C, reads the latest panic callback, dispatches the required reset and
returns again before invoking C. Finalizers can replace or clear the callback
while loading runs. Both dispatches complete their Rust cleanup before any C
jump. Errors caught by the parser remain ordinary returned loader statuses.
Protected routing into a waiting target remains unfinished.

The [file comparison](2026-10-28-rust-lua-thread-files-current.json) checks 53
observations across fresh, completed, yielded, failed and suspended C targets.
Inputs include empty/text/shebang/binary/shebang-binary files, malformed text
and binary chunks, missing files, directories and NUL-terminated paths. It
compares both stack sizes, value types, retained statuses, environments, source
names and diagnostics. Three cases load through a waiting parent thread.
Nine further [resumption controls](2026-10-28-rust-lua-thread-files-resumes.json)
resume suspended C activations after every file outcome. They match the release
reference and also pass its internal assertions.

The [stdin comparison](2026-10-28-rust-lua-thread-file-stdin-current.json) checks
five targets after a Lua read has consumed the first byte. File loading shares
the buffered input, retains the `=stdin` source name, leaves stdin open and
handles subsequent EOF loads. A CLI integration test exercises the same path
through piped process input.

The [finalizer-panic comparison](2026-10-28-rust-lua-thread-file-panic-current.json)
checks another 53 observations. Fifty cover five target states, two callbacks
and table/string/number/boolean/nil errors. Three replace or install the panic
callback inside the finalizer. Callbacks observe the reset stack, original
error, target identity and status; recovery leaves the caller usable. Locals
captured by the faulting finalizer remain valid after collection and can be
changed through `debug.setupvalue`. A
[preserved CLI without this routing](2026-10-28-rust-lua-thread-file-panic-unrouted-current.json)
still propagates the first error to the wrong caller with the current fixture.

The assertion-enabled reference passes the
[file corpus](2026-10-28-rust-lua-thread-files-checked.json) and
[finalizer corpus](2026-10-28-rust-lua-thread-file-panic-checked.json).
The initial finalizer fixture allocated a pressure string on the caller, causing
collection before target selection, and used `error`'s default location prefix
while asserting string identity. Corrected tests reserve target stack capacity
after restarting collection and raise errors at level zero. A retained
[allocation-control diagnostic](2026-10-28-rust-lua-thread-file-panic-no-allocation.json)
shows why a post-restart allocation is needed to trigger collection in both
engines. These are fixture corrections, separate from the routing fix.

The permanent [boundary harness](2026-10-28-rust-lua-thread-files-boundary-current.json)
also checks two processes whose finalizers clear the panic callback or replace
it with a returning callback. Each must exit 1 with exact expected bytes. They
are separate from successful recovery and cleanup processes. Portable Rust
tests check target capacity rejection, restoration of both stacks, same-thread
fallback and named-file release.

The [release CLI controls](2026-10-28-rust-lua-thread-file-exits-current.json)
also match after explicitly finishing collection before creating the finalizer.
The [unsettled CLI audit](2026-10-28-rust-lua-thread-file-exits-unsettled.json)
remains a strict failure: with the collector state left by the smaller public
module bootstrap, the reference postpones finalization past the unprotected
filename push, while Rust runs the finalizer there. The reference returns from
the C helper and reaches the fixture's `panic returned` guard; Rust instead
exits silently or prints the returning panic callback's line. Both processes
exit 1, but their bytes differ. This observable automatic-GC timing mismatch is
retained rather than counted as a passing panic comparison.

The [validation report](2026-10-28-rust-lua-thread-files-validation.json),
[argument audit](2026-10-28-rust-lua-thread-files-stdlib.json),
[native-enabled i686 suite](2026-10-28-rust-lua-thread-files-i686-suite.json) and
[Windows independent probes](2026-10-28-rust-lua-thread-files-windows-suite.json)
record the broader checks. The [evidence manifest](2026-10-28-rust-lua-thread-files-evidence.json)
binds current sources, reports, logs and binaries.
The final checks pass 776 Rust tests on Windows x64 and 777 on Linux x64 and
i686 SSE2, strict Clippy, formatting, builds without default features and the
empty normal/build dependency tree. All 5,507 argument cases still match.
Unchanged `main.lua` and `all.lua` pass in both native-enabled i686 CLIs, with
T/testC absent. Windows retains 17/18 independent passes in all four modes,
the shared `files.lua:59` rename failure and the strict checker's failing exit.
All 29 successful Rust boundary processes balance 752,102 dispatch returns,
destroy 6,317 C states and close 39 native loader references. Every runner
asserts shared group cleanup. The five expected terminating processes remain
outside those totals.

The first full boundary build failed because its development header lacked two
macro aliases used by the new C fixture. The fixture now calls the underlying
exported functions directly. Pre-fix sources, build diagnostics and passing
platform reports are retained separately; final validation was rerun with the
corrected fixture and frozen source hashes.

Core declarations remain 76/80. The four lifecycle/allocator APIs, arbitrary
unprotected calls and inactive metamethod execution, protected errors into
waiting threads, complete resource/OOM behavior and prior strict failures
remain open, including the automatic-GC timing mismatch above. Full standalone
Lua 5.1 parity is not established.
