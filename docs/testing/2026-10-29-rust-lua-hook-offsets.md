# Lua 5.1 signed hook stack offsets

A hook can select a yield base above the top restored on hook return. The
resulting `lua_gettop` is negative. Previously the scheduler converted that
offset to an unsigned zero-result window, losing both the raw C API state and
the coroutine library's subsequent control flow.

Hook suspension now carries its base and top independently. Native resume and
selected-thread API calls retain signed windows, and raw inspection reports
their negative offsets. A negative C `lua_xmove` count advances the source top
without copying values; its newly exposed physical slots retain their contents.
A zero-count move preserves the source window. Both active and inactive thread
paths retain collector visibility when physical slots become exposed.

Release Lua's coroutine library passes a negative result count through
`lua_xmove` and then interprets it as a failure flag. `coroutine.resume` returns
`false, thread` while the child remains suspended. The wrapper propagates the
closure value below its empty C activation as the error object. The engine now
matches those observed identities and subsequent resumptions.

The [preserved previous binary](2026-10-29-rust-lua-hook-offsets-before.json),
`689245041`, differs on 66 of 108 observations. Its hash matches the previous
milestone's library audit. Reference release and checked builds agree on this
corpus, which covers signed raw windows, raw resumptions, negative C moves,
active hook slot restoration and destination stacks.

All 108 observations match unmodified release and assertion-enabled Lua 5.1.5 on
[Windows debug](2026-10-29-rust-lua-hook-offsets-focused.json),
[Windows release](2026-10-29-rust-lua-hook-offsets-current.json) and
[Linux i686 SSE2 release](2026-10-29-rust-lua-hook-offsets-i686-current.json),
for 324 three-engine observations.

Two additional corpora match release Lua and retain their checked-reference
aborts separately:

- Wrapped-coroutine error identity and recovery: 27 observations per profile on
  [Windows debug](2026-10-29-rust-lua-hook-offsets-wrapped-focused.json),
  [Windows release](2026-10-29-rust-lua-hook-offsets-wrapped-current.json) and
  [i686 release](2026-10-29-rust-lua-hook-offsets-i686-wrapped-current.json).
  Checked Lua aborts when the wrapper indexes its empty C window.
- Zero-count C moves from a negative source window: 36 observations per profile
  on [Windows debug](2026-10-29-rust-lua-hook-offsets-zero-focused.json),
  [Windows release](2026-10-29-rust-lua-hook-offsets-zero-current.json) and
  [i686 release](2026-10-29-rust-lua-hook-offsets-i686-zero-current.json).
  Checked Lua rejects the source-element precondition.

These total 189 release-only observations across the three profiles. The older
I/O closer hook-yield probe now also matches release Lua on
[Windows debug](2026-10-29-rust-lua-hook-offsets-io-focused.json),
[Windows release](2026-10-29-rust-lua-hook-offsets-io-current.json) and
[i686 release](2026-10-29-rust-lua-hook-offsets-io-i686-current.json).
Its first result is `false, thread`; its second resume completes with `true, 17`.
The checked references still abort on the second resume's move-capacity check.

A normal integration test checks the corpus transcripts and the I/O recovery.
A portable Rust callback regression checks signed raw resume windows,
zero-count moves, ordinary resume failure identity and wrapper identity.
[Full validation](2026-10-29-rust-lua-hook-offsets-validation.json) passes 821
Windows tests and 822 tests on each Linux target, strict Clippy, formatting and
all-target checks without default features. Normal/build dependencies remain
empty. The isolated [portable run](2026-10-29-rust-lua-hook-offsets-portable.json)
passes 27 tests. The [library audit](2026-10-29-rust-lua-hook-offsets-stdlib.json)
matches 5,507 cases, and the
[native audit](2026-10-29-rust-lua-hook-offsets-boundary-current.json) retains
balanced dispatch, state and library lifetimes.

Both native-enabled i686 CLIs pass unchanged `main.lua` and `all.lua` in the
[stock suite](2026-10-29-rust-lua-hook-offsets-i686-suite.json), with T/testC still
absent. [Windows](2026-10-29-rust-lua-hook-offsets-windows-suite.json) retains
17/18 independent passes in all four modes and the common `files.lua:59` failure.
The [i686 verifier](2026-10-29-rust-lua-hook-offsets-i686-verification.json)
checks reference settings, ELF32 binaries, current sources and artifacts.
The [evidence manifest](2026-10-29-rust-lua-hook-offsets-evidence.json) binds the
reports, sources, binaries and logs and embeds the proof tools.

Full Lua 5.1 parity remains unfinished. Lifecycle/allocator APIs, exact heap and
GC accounting, arbitrary reader/body/API interleavings, diagnostics and
resource/OOM behavior remain open. The bounded cases above do not establish
every continuation or stack-limit behavior.
