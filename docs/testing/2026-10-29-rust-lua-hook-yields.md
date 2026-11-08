# Lua 5.1 hook yield bases

A native hook's `lua_yield` selects a new visible base immediately. Hook return
then restores the old physical top. A lower selected base exposes the intervening
slots as yielded values, even when the hook requested zero results. After those
values are consumed, resume arguments can overwrite the exposed parameters.
The engine previously replaced the selected base with the saved top, so it
reported no values and preserved those parameters.

The VM now tracks the selected base and restored top separately, retains the
physical slots through suspension, and reports the resulting value range.
Further C API operations inside the hook observe the selected base and yielded
status immediately. Hook return retains that base after subsequent writes.
Primitive table access, including table-valued metamethod chains, now uses the
shared lookup/assignment resolver directly. It no longer writes interpreter
call temporaries over discarded physical slots. Function-valued metamethods
continue through the existing call scheduler.

The [preserved previous binary](2026-10-29-rust-lua-hook-yields-before.json),
`9bb30fc1c`, differs from both PUC references on 222 of 360 observations. Its
binary hash matches the preceding milestone's library audit.

The new public-header corpus covers line and count hooks, combined masks,
lowered bases, writes and lookups after `lua_yield`, immediate status queries,
table and userdata access, and collection while suspended. Each scenario resumes
through ordinary Lua, through C while retaining the yielded public stack, or
through C after clearing and collecting that stack. These cases resume with a
physical top accepted by the checked reference. Their expected transcript is
also asserted by an ordinary integration test. A Rust callback regression
checks exposed values and parameter overwrites without the C feature.

All 360 observations match release and assertion-enabled Lua 5.1.5 on
[Windows debug](2026-10-29-rust-lua-hook-yields-focused.json),
[Windows release](2026-10-29-rust-lua-hook-yields-current.json) and
[Linux i686 SSE2 release](2026-10-29-rust-lua-hook-yields-i686-current.json),
for 1,080 observations across the three profiles. Reference sources are unchanged.
The [i686 verifier](2026-10-29-rust-lua-hook-yields-i686-verification.json)
checks source/artifact hashes, ELF32 width, assertions and the native stock suite.

The earlier eight pop-then-yield cases now also match release Lua on
[Windows debug](2026-10-29-rust-lua-hook-yields-strict-focused.json),
[Windows release](2026-10-29-rust-lua-hook-yields-strict-current.json) and
[i686 release](2026-10-29-rust-lua-hook-yields-i686-strict-current.json).
Their checked references still abort at the VM top invariant when the caller
resumes with fewer values than the original frame top. Those 24 release-only
comparisons are recorded separately from the passing three-engine corpus.

[Full validation](2026-10-29-rust-lua-hook-yields-validation.json) passes 819
Windows tests and 820 tests on each Linux target, strict Clippy, formatting and
all-target checks without default features. Normal/build dependencies remain
empty. The [portable run](2026-10-29-rust-lua-hook-yields-portable.json) passes
26 tests in its isolated target directory. The
[library audit](2026-10-29-rust-lua-hook-yields-stdlib.json) matches 5,507 cases;
the [native audit](2026-10-29-rust-lua-hook-yields-boundary-current.json)
preserves balanced dispatch, state and library lifetimes. Both native-enabled
i686 CLIs pass unchanged `main.lua` and `all.lua` in the
[stock suite](2026-10-29-rust-lua-hook-yields-i686-suite.json), with T/testC still
absent. [Windows](2026-10-29-rust-lua-hook-yields-windows-suite.json) retains
17/18 passes in all four modes and the common `files.lua:59` failure.

The [first validation run](2026-10-29-rust-lua-hook-yields-validation-first.json)
retains an i686 calendar comparison failure. The oracle returned fields from
different dates while parallel tests used libc's shared `gmtime`/`localtime`
buffer. Calendar oracle comparisons are now serialized; production calendar
code is unchanged. The rerun passes all 14 calendar tests on every target.
The [evidence manifest](2026-10-29-rust-lua-hook-yields-evidence.json) binds the
current sources, binaries, references, reports and logs and embeds the proof tools.

Full Lua 5.1 parity remains unfinished. The separate
[I/O closer hook-yield audit](2026-10-29-rust-lua-hook-yields-io-current.json)
is still unequal with current binaries: release Lua returns `false, thread`
on the first resume, while the engine returns `true`; checked Lua aborts on its
second resume. Lifecycle/allocator APIs, exact heap and GC accounting, broader
strict continuations, diagnostics and resource/OOM behavior remain open.
