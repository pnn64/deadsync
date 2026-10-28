# Collector checks at native allocation boundaries

The C adapter now performs the collector checks required by Lua 5.1 string,
table, userdata and C-closure allocation, formatted messages, concatenation of
two or more operands and numeric `lua_tolstring` conversion. Formatted-message
components use internal pushes so they do not introduce additional checks.
Numeric conversion checks collection after replacing the value, then reacquires
the indexed string. Full Lua 5.1 parity remains the goal.

The [preceding release](2026-10-28-rust-lua-native-alloc-gc-before.json)
finishes the initial 26-case probe but misses required checks or runs collection
inside a later lookup. Its CLI and SDK are preserved with digests matching the
preceding selected-thread file checkpoint. The original C and Lua fixtures are
also preserved; the broader final corpus adds target states and assertions.

Two implementation differences mattered. Primitive host get/set/compare
operations entered the interpreter driver even after producing a synchronous
result. Its unrelated collector check could run while the C function was still
active. Completed synchronous actions now pop their invisible continuation
directly. Explicit C API allocation checks provide the intended safe points.

Collection on another thread uses the shared native window. A completed GC tag
method restores the current CallInfo base, which can precede a yielded C suffix.
A tag-call epoch distinguishes actual finalizer calls from a collector step
that invokes none. The
[first target-state comparison](2026-10-28-rust-lua-native-alloc-gc-before-base.json)
records six incorrect positive-result stack sizes before this restoration was
added. Nonallocating zero/one-operand concatenation now also works on these
stacks without running collection.

The [native boundary comparison](2026-10-28-rust-lua-native-alloc-gc-boundary-current.json)
and [public-header release comparison](2026-10-28-rust-lua-native-alloc-gc-current.json)
match all 126 observations. Twenty-six cover the active C activation; one
hundred cover fresh, completed, yielded, failed and suspended C targets. Checks
observe finalizer timing, caller identity, visible stack size, result type,
original error identity and C-local panic recovery. Controls distinguish
operations that require collection from zero/one-operand concatenation,
numeric object length and nil pushes, which must defer it.

Each case starts from a completed collection and explicitly reserves target
stack capacity after restart. This supplies allocation debt without introducing
an earlier collector check. The assertion-enabled
[reference build](2026-10-28-rust-lua-native-alloc-gc-checked.json) also completes
the corpus with identical bytes. A portable internal regression verifies that
a selected collector check needs no spare public result slot at the 8,000-slot
stack bound. The public-header integration corpus checks its observations
against the reference-derived expectations as well as printing them.

The supplementary [operand audit](2026-10-28-rust-lua-native-alloc-gc-operands.json)
matches eighteen active-frame snapshots before the allocating operation.
A finalizer also replaces a string concatenation operand with a table. Both
engines then report the same concatenation error, proving that the operation
reads the changed operand after collection. Release and checked references
both complete these controls.

The [validation report](2026-10-28-rust-lua-native-alloc-gc-validation.json),
[argument audit](2026-10-28-rust-lua-native-alloc-gc-stdlib.json),
[native-enabled i686 suite](2026-10-28-rust-lua-native-alloc-gc-i686-suite.json) and
[Windows independent probes](2026-10-28-rust-lua-native-alloc-gc-windows-suite.json)
record the broader gates. The [evidence manifest](2026-10-28-rust-lua-native-alloc-gc-evidence.json)
binds current source, binary, report, artifact and log digests.
Final validation passes 778 Rust tests on Windows x64 and 779 on Linux x64 and
i686 SSE2, strict Clippy, formatting, builds without default features and the
empty normal/build dependency tree. All 5,507 library argument cases still match.
Both native-enabled i686 CLIs pass unchanged `main.lua` and `all.lua`, with
T/testC absent. Windows retains 17/18 independent passes in all four modes and
the shared `files.lua:59` rename failure. Thirty successful Rust boundary
processes balance 813,567 dispatch returns, destroy 6,545 C states and close
40 loader references. All shared groups are asserted released. The five expected
exit-1 processes remain outside those cleanup totals.

An unrelated workspace lockfile update overlapped the first validation pass.
Linux x64's locked test invocation could not start; Windows and i686 passed,
but the workspace hash guard correctly rejected that mixed snapshot. The
[original validation](2026-10-28-rust-lua-native-alloc-gc-validation-workspace-before.json)
and logs are preserved. The complete validation was repeated successfully
against the current lockfile with all captured source hashes unchanged.

The earlier automatic-GC timing mismatch remains open. Required safe points do
not by themselves make the two engines' collector phases and allocation charges
identical. The unsettled file-panic probe still produces different behavior;
the original mismatch remains recorded. The earlier Windows rename failure,
strict reader-resume failures and checked-continuation invariant failures also
remain separate from the passing corpora.
The [current unsettled audit](2026-10-28-rust-lua-native-alloc-gc-unsettled-current.json)
retains both mismatches using the final release binary and the preceding
public module. Both programs receive the same relative executable name, so
different CLI path prefixes do not account for their unequal bytes. Its strict
checker exits 1.

Core declaration coverage remains 76/80. Lifecycle/allocator entry points,
protected errors into waiting targets, other auxiliary allocation checks,
selected-thread formatted or multi-operand concatenation and thread creation,
complete resource/OOM behavior and broader GC timing remain unfinished. This
checkpoint does not establish full standalone Lua 5.1 parity.
