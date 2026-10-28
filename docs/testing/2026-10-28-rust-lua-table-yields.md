# Yielded table metamethod continuations

Selected-thread table/field access and comparisons now retain a permitted
metamethod yield. Previously the adapter rejected these yields through the
target's panic callback. A Lua 5.1 API call can instead return while the target
remains yielded, and then finish its own physical stack operations. Lookup and
comparison APIs consume the last physical value; assignments pop their inputs
from the new top. The resulting offset can be negative.

The runtime retains the unfinished frames and their yielded base. A tag call
that already returned through C no longer contributes a C-call charge or an
escaped yield barrier. Resuming a setter leaves its original operands below
the tag-function slot. Collection now clears open local slots above the physical
top, matching Lua's thread traversal even when a returned API has lowered that
top past an open local.

The [Windows release comparison](2026-10-28-rust-lua-table-yields-current.json),
[Windows debug comparison](2026-10-28-rust-lua-table-yields-focused.json) and
[i686 SSE2 comparison](2026-10-28-rust-lua-table-yields-i686-current.json) match
all 1,078 observations against release and assertion-enabled Lua 5.1.5:

- 150 stack/status cases cover all six APIs, five target contexts and zero to
  four yielded values. Fresh threads reject the yield; previously activated
  threads retain it. Collection and mutable surviving upvalues are observed.
- 750 value cases add numbers, false, nil, object/function identities, binary
  strings, negative zero, userdata and threads. The C fixture copies only values
  within the reported positive stack range and records comparison results.
- 90 direct C metamethod cases yield all arguments, the last argument or zero
  arguments. They check the C boundary and collection of the target state.
- 88 successful resumption cases cover completed targets, zero to three resume
  arguments, surviving mutable cells, repeated yields and retained result or
  operand prefixes. Each case also passes the checked reference.

The [Windows baseline](2026-10-28-rust-lua-table-yields-before.json) and
[i686 baseline](2026-10-28-rust-lua-table-yields-i686-before.json) use preserved
executables from commit `8838a4314`. Both references complete the final corpus;
the old Rust executable fails its assertions after 300 observations. Earlier
unchecked rows also retain the rejected-yield differences.

The [first fixture](2026-10-28-rust-lua-table-yields-fixture-first.json) assumed
an open local always retained its table. The reference clears that local after
a zero-value setter yield and collection, so the corrected fixture observes
its type before and after collection. The
[four remaining GC mismatches](2026-10-28-rust-lua-table-yields-gc-before.json)
record the implementation failure before the physical-top fix. The
[first value fixture](2026-10-28-rust-lua-table-yields-values-fixture-first.json)
also retains an incorrect false-valued `and`/`or` expectation, corrected with
an explicit conditional.

The isolated [480-case resumption audit](2026-10-28-rust-lua-table-yields-resume-current.json)
retains every process result, including reference failures and unnormalized
address-bearing output. Forty-four cases complete normally and identically in
both references, and all forty-four match Rust. The checked reference rejects
436 cases, and the release reference fails in 284 cases. These cases are not
counted as successful continuation comparisons;
all 480 Rust processes still exit zero and reach the usability check. The
[initial resumption fixture](2026-10-28-rust-lua-table-yields-resume-first.json)
used `==` while formatting values and could invoke another `__eq` handler.
The corrected audit uses `rawequal`; its
[focused record](2026-10-28-rust-lua-table-yields-resume-focused.json) also verifies
the setter operand fix before the final release run. Reference crashes and
checked invariants remain explicit gaps in the broader strict audit.

The [portable regression](2026-10-28-rust-lua-table-yields-portable.json) runs
760 observations through Rust host callbacks with default features disabled.
It includes direct host-function yields and all 88 successful resumptions.
[Full validation](2026-10-28-rust-lua-table-yields-validation.json) passes 805
tests on Windows x64 and 806 on Linux x64 and i686 SSE2, strict Clippy,
formatting, builds without default features and an empty normal/build
dependency tree. The [library audit](2026-10-28-rust-lua-table-yields-stdlib.json)
matches all 5,507 cases. Both native-enabled i686 CLIs pass unchanged `main.lua`
and `all.lua`; T/testC remains absent. Windows retains 17/18 independent passes
in all four modes and the shared `files.lua:59` rename failure.

The [combined native audit](2026-10-28-rust-lua-table-yields-boundary-current.json)
rechecks the preceding native corpora. Forty-one successful Rust processes
balance 1,221,220 operations, 19,209 state lifetimes and 51 library lifetimes;
25 expected terminating processes are counted separately. The
[evidence manifest](2026-10-28-rust-lua-table-yields-evidence.json) binds current
sources, reports, binaries, public modules and logs. The separate
[i686 verification](2026-10-28-rust-lua-table-yields-i686-verification.json)
checks Linux artifacts, reference sources and the assertion-enabled build.

Full Lua 5.1 parity remains unfinished. Arbitrary reader/body/API interleavings,
the broader strict resumption failures, exact heap counts, collector work and
allocation charges, timing, the retained unrooted package-name mismatch,
diagnostics and resource/OOM behavior remain open. Core declaration coverage
stays 76/80: `lua_close`, `lua_getallocf`, `lua_newstate` and `lua_setallocf`
remain missing.
