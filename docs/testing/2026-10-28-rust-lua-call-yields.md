# Selected calls across yields and interleaved API calls

A pending Lua function now regains its register window when its yielding native
call resumes. A synchronous C API call made on the suspended state can shrink
that storage. The native return path previously kept the shorter vector, so a
later Lua instruction could index beyond its end. Restoring the saved caller
window matches the `ci->top` restoration in Lua 5.1's `luaD_poscall`.

The [baseline](2026-10-28-rust-lua-call-yields-before.json) preserves the preceding
release's bounds abort against the final corpus. Its executable and SDK match
the selected unprotected-call checkpoint. A phase diagnostic identifies the
failure after an intervening synchronous call and before the pending function
finishes resuming. The smaller 640-case baseline and its first passing comparison
are retained alongside the final expanded corpus.

The [public-header release comparison](2026-10-28-rust-lua-call-yields-current.json),
[assertion-enabled reference](2026-10-28-rust-lua-call-yields-checked.json) and
[native-boundary run](2026-10-28-rust-lua-call-yields-boundary-current.json) match
1,280 observations with both Lua and C resumers:

- 768 successful yield/resume sequences cover zero through three yielded values,
  six result counts including padding, protected/unprotected calls, repeated
  yields, and four interleavings. These include a synchronous call on the same
  state, a protected error/handler on that state, and nested calls through another
  thread back to the waiting state.
- 512 sequences leave two API calls yielded on the same state. The newer call
  raises on resumption, preserving eight error-value types and bypassing both
  expired API handlers. The older computation does not continue. Failed frames,
  mutable open upvalues and later weak collection match the reference.

The C fixture keeps pending calls and yielded values on the target stack. Its
resume wrapper uses public `lua_setlevel`, `lua_xmove` and `lua_resume`, matching
the argument/result transfer used by the coroutine library. The checked
reference accepts the complete corpus. This is separate from the retained broad
resume probe whose invalid continuations trigger reference invariant failures.

The [C-call count audit](2026-10-28-rust-lua-call-yields-level-current.json)
matches release/checked references at the API yields, after interleavings,
between repeated yields, and after completion or failure. It adds 4,224 count
copies, zero-count writes, selected collector activations and restorations.
Module/report hashes bind the preceding boundary module's reference `nCcalls`
observation and corresponding Rust query.

A portable Rust regression reproduces an intervening call between yield and
resume and validates the pending function's later temporaries, results, open
cells and reclamation. It passes in the default suite and in a separate
[build without default features](2026-10-28-rust-lua-call-yields-portable-no-default.json).
The source change restores storage in the shared native-return path.

The [full validation](2026-10-28-rust-lua-call-yields-validation.json) passes
796 Rust tests on Windows x64 and 797 on Linux x64 and i686 SSE2, strict Clippy,
formatting, builds without default features and the empty normal/build
dependency tree. The [library argument audit](2026-10-28-rust-lua-call-yields-stdlib.json)
matches all 5,507 cases. Both native-enabled i686 stock entry points pass with
T/testC absent. Windows retains 17/18 independent passes in all four modes and
the shared `files.lua:59` rename failure.

The preceding 960 call/panic and 736 waiting-error observations, their 1,120 and
800 count controls, twenty terminating stdout/file-write scenarios, selected GC,
execution, concatenation and allocation corpora are rechecked with this release.
The [evidence manifest](2026-10-28-rust-lua-call-yields-evidence.json) binds current
source, executable, module, report and log digests.

The 38 successful native-boundary processes balance 983,952 dispatches and
returns, 11,255 state creations and destructions, and 48 library opens and closes.
The 25 expected terminating processes are recorded separately from those cleanup
totals.

Full Lua 5.1 parity remains unfinished. Arbitrary interleaved reader/body/API
execution is not established by this corpus. The strict broad resume audit
remains 6/20 and the strict reader audit 19/24; reference assertions/crashes and
all Rust outcomes are retained. Collector timing, unequal raw heap counts, the
unrooted package-name mismatch, complete lifecycle/allocator APIs and
resource/OOM behavior also remain open. Core declaration coverage stays 76/80,
with `lua_close`, `lua_getallocf`, `lua_newstate` and `lua_setallocf` missing.
