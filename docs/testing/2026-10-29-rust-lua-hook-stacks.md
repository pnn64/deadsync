# Lua 5.1 physical stack slots in C hooks

A C hook can lower the visible stack top while its interrupted function still
owns the physical slots. Lua 5.1 restores the saved top after the hook returns;
values popped without a replacement remain in those slots. Previously the Rust
engine truncated their storage and restored nil. For example, a hook that calls
`lua_settop(L, 0)` while entering `function(x,y,z)return x,y,z end` changed
`17,29,42` into three nil values.

The VM now retains the physical slot contents during an external hook. Register
window shrink operations preserve the latest contents, including writes that
are subsequently popped. Hook return restores missing physical slots while
keeping writes in the live window. These saved copies are excluded from traced
roots. Thread traversal clears discarded slots, including scalars, so an
explicit collection still changes their later observable values to nil. Hook
unwind clears the saved values, and heap estimates include their capacity.

The [preserved baseline](2026-10-29-rust-lua-hook-stacks-before.json) runs the
previous committed release binary, `b2eec5b8c`, whose hash matches the preceding
milestone's library audit. It differs from both PUC references on 101 of 172
observations. The reference release and assertion-enabled builds agree.

The public-header fixture covers call, return, line and count hooks on Lua
activations, plus call and return hooks on a native echo function. Cases combine
top changes, pushes, remove/replace, explicit nil extension, GC collection and
steps, and collector controls that do not traverse a thread. Strings and tables
exercise object values as well as numeric slots. The expected transcript comes
from the reference build and is also checked by a normal integration test. A
Rust callback test checks the same physical-slot and collector distinction
without requiring the C feature.

All 172 observations match on
[Windows debug](2026-10-29-rust-lua-hook-stacks-focused.json),
[Windows release](2026-10-29-rust-lua-hook-stacks-current.json) and
[Linux i686 SSE2 release](2026-10-29-rust-lua-hook-stacks-i686-current.json),
for 516 observations across the three profiles. Each uses unmodified Lua 5.1.5
sources, with assertions force-enabled in the checked reference. The
[i686 verifier](2026-10-29-rust-lua-hook-stacks-i686-verification.json) checks
ELF32 width, source and artifact hashes, checked-reference settings and the
native-enabled stock suite.

[Full validation](2026-10-29-rust-lua-hook-stacks-validation.json) passes 817
Windows tests and 818 tests on each Linux target, strict Clippy, formatting and
all-target checks without default features. Normal/build dependencies remain
empty. The [portable run](2026-10-29-rust-lua-hook-stacks-portable.json) passes
25 tests in an isolated no-default-feature target directory. The
[library audit](2026-10-29-rust-lua-hook-stacks-stdlib.json) matches 5,507 cases,
and the [native audit](2026-10-29-rust-lua-hook-stacks-boundary-current.json)
preserves balanced dispatch, state and library lifetimes. Both CLIs pass
unchanged `main.lua` and `all.lua` in the native-enabled
[i686 stock suite](2026-10-29-rust-lua-hook-stacks-i686-suite.json), with T/testC
still absent. [Windows](2026-10-29-rust-lua-hook-stacks-windows-suite.json)
retains 17/18 passes in all four modes and the common `files.lua:59` failure.
The [evidence manifest](2026-10-29-rust-lua-hook-stacks-evidence.json) binds the
sources, references, binaries, reports and logs and embeds the verification tools.

A separate [pop-then-yield probe](2026-10-29-rust-lua-hook-stacks-yield-current.json)
remains unequal. Release Lua exposes the restored slots above the base chosen
by `lua_yield`, and resume arguments can overwrite those slots. The Rust engine
currently reports no yielded values and preserves the old parameters. The
checked reference aborts on its VM top invariant. This probe is retained as
unresolved evidence and is excluded from the passing observation count. The
earlier [I/O hook-yield case](2026-10-28-rust-lua-io-closers-hook-yield.json) also
remains open.

Full Lua 5.1 parity is unfinished: lifecycle/allocator APIs, exact heap and GC
accounting, broader strict continuations, diagnostics and resource/OOM behavior
still require implementation and evidence.
