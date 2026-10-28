# Direct I/O close callbacks

File close operations now look up `__close` with normal table semantics and
execute the selected native body inside the caller's existing activation.
Previously only three numeric built-in close handlers were recognized. This
prevented fresh C closures from serving as ordinary file close handlers.

The shared path handles `io.close`, file methods, file finalizers and the
automatic close at the end of `io.lines`. A callback uses the caller's Lua
environment, upvalues, stack and debug identity. It receives its own private
host binding data. That data can outlive the selected closure during a call
without retaining the closure's Lua environment or captures.

The full modified C window is retained before selecting its result suffix.
Finalizers and line iterators discard callback results while preserving those
temporaries for return-hook inspection. A yielded delegated body now retains
the hidden C prefix below its exposed yielded suffix. Hook bodies can also pop
below their scratch control window without indexing discarded storage.

The [Windows debug](2026-10-28-rust-lua-io-closers-focused.json),
[Windows release](2026-10-28-rust-lua-io-closers-current.json) and
[i686 SSE2](2026-10-28-rust-lua-io-closers-i686-current.json) comparisons match
231 observations against release and assertion-enabled Lua 5.1.5:

- 210 cases cover five caller forms, direct/metamethod lookup, five callback
  bodies and zero to four extra arguments. They observe caller identity,
  environments, captures, original C arguments, arbitrary results and errors.
- Six return-hook cases inspect complete windows after appending results or
  replacing the stack. The Lua debug dispatcher contributes its own hook-table
  temporary after that window.
- Two collection cases remove the selected callback from the file environment,
  pop it and collect its Lua environment or capture while its C body runs.
- Three delegated-body cases check the calling closure's environment and captures.
- Six coroutine cases cover direct and delegated bodies, suffix/zero/all-value
  yields, resumption arguments and the retained C prefix in return hooks.
- Four hook cases execute a close body inside an interrupted Lua activation.

The [preserved CLI](2026-10-28-rust-lua-io-closers-baseline.json) comes from
`fa1aaed3c`. Its [comparison](2026-10-28-rust-lua-io-closers-before.json) stops
after the first callback case; both references complete the corpus. The
[prefix regression](2026-10-28-rust-lua-io-closers-prefix-before.json) retains the
initial delegated-yield implementation's missing C prefix. The
[hook regression](2026-10-28-rust-lua-io-closers-hook-first.json) retains the
discarded-control-window panic before its correction.

Two fixture corrections are explicit: the
[first fixture](2026-10-28-rust-lua-io-closers-fixture-first.json) tried to read
C captures through Lua's debug library, which deliberately hides them. The
fixture now uses public `lua_getupvalue`. The
[return-hook fixture](2026-10-28-rust-lua-io-closers-fixture-hook.json) initially
omitted the debug dispatcher's table temporary. Two
[development-header](2026-10-28-rust-lua-io-closers-boundary-build-first.json)
[build failures](2026-10-28-rust-lua-io-closers-boundary-build-second.json)
also remain recorded; the shared helper now uses exported API functions.

The separate [hook-yield audit](2026-10-28-rust-lua-io-closers-hook-yield.json)
remains a strict gap. The release reference first returns false plus its own
thread object, then completes a second resume. The checked reference produces
that first result and rejects the second resume with a `lua_xmove` assertion.
Rust yields zero results and completes its second resume. These unequal and
checked-invariant observations are retained and are not counted among the 231
completed comparisons. Broader hook/body/API interleavings remain unfinished.

[Full validation](2026-10-28-rust-lua-io-closers-validation.json) passes 812
tests on Windows x64 and 813 on Linux x64 and i686 SSE2, strict Clippy,
formatting, checks without default features and an empty normal/build dependency
tree. The [portable regression](2026-10-28-rust-lua-io-closers-portable.json)
executes ten tests with the default C feature disabled. The
[library audit](2026-10-28-rust-lua-io-closers-stdlib.json) matches 5,507 cases.
The [combined native audit](2026-10-28-rust-lua-io-closers-boundary-current.json)
includes the new close-callback corpus and cleanup guards.

Both native-enabled i686 CLIs pass unchanged `main.lua` and `all.lua` in the
[stock suite](2026-10-28-rust-lua-io-closers-i686-suite.json); T/testC remains
absent. [Windows](2026-10-28-rust-lua-io-closers-windows-suite.json) retains 17/18
independent passes in all four modes and the shared `files.lua:59` failure.
The [i686 verification](2026-10-28-rust-lua-io-closers-i686-verification.json)
and [evidence manifest](2026-10-28-rust-lua-io-closers-evidence.json) bind current
sources, references, binaries, public modules and logs.

Full Lua 5.1 parity remains open. `luaopen_io`, `luaL_openlibs`, the four core
lifecycle/allocator APIs, exact allocation/heap and collector accounting,
broader strict continuations, diagnostics and resource/OOM behavior are still
unfinished. This milestone supplies the callback mechanism needed by I/O
initialization; it does not complete that initializer or the standalone gate.
