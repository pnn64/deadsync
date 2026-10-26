# Independent Lua engine

The full objective is a standalone Rust Lua 5.1â€“5.5 implementation, followed by
song and noteskin integrations sharing the compiler, VM, heap and semantics.
The first completion target remains arbitrary standalone Lua 5.1 programs with
correct libraries, GC, closures, metatables, coroutines and errors. The current
implementation is an early functional stage of that target.

Current work follows correctness and Lua 5.1 compatibility first. Close known
failures and expand differential coverage against official Lua before resuming
performance tuning. Existing performance reports remain historical baselines;
new compatibility milestones require correctness checks, not benchmark campaigns.
Full standalone Lua 5.1 parity is still an open gate.

## Preservation and current scope

The mlua fork lives in `crates/deadlib-lua-legacy`, including tests, benchmarks,
examples, macros, FFI and existing optimizations. Its three Cargo packages use
the `deadlib-lua-legacy` prefix. Rust imports remain `mlua`, `mlua_sys` and
`mlua_derive`. All production consumers select the legacy workspace alias.
Current benchmark scripts and Markdown commands follow the move; historical
performance JSON keeps its recorded measurement provenance.

| Requirement | Evidence and remaining work |
| --- | --- |
| Preserve legacy and ongoing changes | Move and references updated. Full legacy feature tests pass; song Lua: 843 passed, 41 ignored; noteskin: 298 passed, 10 ignored; theme check and 3 integer-width regressions pass. Original runtime/test/example source contents were compared across the move. |
| Independent compiler/interpreter | Resumable scanner/parser, flat AST arenas and grammar continuations, continuation-based register compiler, verifier, literal folding, VM and CLI exist. Compiler nesting uses flat task/function vectors and iterative ancestor captures. Fixed-source, reader-based and file-backed text loading share the frontend without replaying prefixes. File loads preserve stdin lookahead, source names, initial executable headers and thread environments. Native Lua 5.1 chunks are decoded, checked and translated into the shared VM with per-site captures and native debug/hook metadata. Lua 5.1 function dumps lower to native chunks, while imported functions retain native words for re-export. Broader export/resource limits and complete reference-suite coverage remain open. |
| Exact language semantics | Differential cases cover scalar bits, strings, assignments, loops, closures, varargs, methods, tail calls, multiple results, metatables, protected calls, arbitrary error identity, named locations, environments and coroutine transitions. Local operand references preserve Lua 5.1 reads after right-hand callbacks; indexed assignment captures only conflicting local addresses. Verified metadata supports function/stack/local/upvalue inspection and mutation. Source diagnostics retain raw bytes, token context, opening lines and tested local/assignment bounds. Full runtime/compiler diagnostics, debug behavior and broader numeric/resource edge cases remain open. |
| Object model and memory | Copyable tags and generation-bearing numeric IDs; flat reusable arenas; vector registers, frames, prototypes and upvalue cells; interned strings; contiguous table arrays and indexed hash entries. Deleted keys retain traversal positions until rebuild. Host roots check VM ownership; raw values must stay in their originating VM. Capacity-based heap estimates and object counts are exposed. |
| Garbage collection | Iterative tracing, cyclic reclamation, Lua 5.1 weak tables, compiler register maps, host roots, pressure checks and collector controls exist. Collected threads close surviving cells; cached pattern/program bindings are reclaimed. Full userdata has environments, metatables, byte storage and Lua 5.1 proxies. Finalizers support creation order, resurrection, weak-reference rules, nested collection, error identity and shutdown. Marking advances a persistent frontier; mutator barriers and final stack/open-cell/metatable refreshes preserve new references. Table propagation advances in 64-slot physical slices, restarting after compaction, hash-to-array moves and metatable changes. Sweeping advances in bounded arena-slot batches. Other object scans, final root/metatable/weak processing, cache rebuilding and callback batches still contain atomic work; fully bounded pauses and broader resource tests remain open. Full collection, explicit steps and host collection now resume automatic collection; successful finalizers restore saved controls while errors retain changes. Pending finalization resumes before a new mark/sweep; the retained finalizer-error/large-pause regression now matches the reference. Exact PUC work units and fully incremental behavior remain unfinished. |
| Coroutines | Shared-heap threads have separate register/frame vectors and environments, explicit resume/yield scheduling, wrappers/status, and cross-thread upvalue access. Lua 5.1 native/metamethod yield restrictions, inherited C-call depth, error identity, quota recovery and reclamation with escaping cells are tested. Debug inspection, local/upvalue mutation, tracebacks and per-thread hooks reach other thread stacks and shared cells. New threads inherit hook controls without inheriting the Lua callback. Hook/traceback callbacks support child resumes, collection and quota recovery. Full hook/diagnostic parity and broader resource tests remain open. |
| Standard libraries | Expanded base library includes radix conversion, iteration, metatables, protected calls, output, environments, `loadstring` and protected reader-based text `load`. String byte operations, patterns, replacement callbacks, formatting, raw table operations/traversal/sorting, coroutine functions and the 5.1 math function set exist. Initial I/O includes file userdata, temporary/named files, defaults, pipes, reads/writes/seeks, buffering, iterators and GC closure. A shared registry stores file tags and builtin `_LOADED` entries. Package loading provides preloaded/Lua-file/custom loaders, import caching and 5.1 module environments. Light userdata supplies unmanaged identities and the observable import sentinel. OS support includes calendar conversion/formatting, clocks, time differences, environment queries, file removal/rename, temporary names, shell execution and process exit. Windows/MSVC native locales connect numeric conversion/formatting, ctype, patterns, collation and calendar formatting. Native locales on other targets, remaining base functions, native C-library loading, broader debug and I/O/device/encoding behavior and numeric/diagnostic compatibility remain open. |
| Persistent compiled cache | Opt-in ProgramCache persists the shared bytecode, constants, collector/debug metadata and native re-export words. Entries bind the format/dialect/target, Cargo compiler build and lexer character classes; full source/name equality guards filename collisions. Bounded decoding, integrity checks, Program verification and native tree/range checks reject incompatible entries. Temporary-file replacement and compile fallback preserve availability. Fresh-process source/native reloads and concurrent writers are tested. Directory eviction, a memory cache and package/CLI integration remain open; tiny/native inputs may reload more slowly than recompilation. |
| Native host calls | Builtins and registered Rust function pointers use numeric native IDs and register argument ranges. HostCall borrows arguments, writes separate results and accesses up to 255 traced/mutable Lua captures. Private owned binding data stays separate from Lua upvalues. Protected nested Lua/native calls preserve outer frames, share the instruction quota and return arbitrary error values to Rust. Unprotected stack calls retain failing child frames for outer error handlers. Hooks, coroutine callers, resumed children, finalizers, preload modules and per-VM cached-program bindings are tested. Callback and nested-call measurements exist. Suspending Rust callback stacks, richer Rust host objects and broader error/overhead work remain open. |
| Lua 5.2â€“5.5 | Unimplemented. Investigated differences below must extend shared infrastructure with per-version oracle gates. |
| Machine code | Optional; defer until compatibility baselines and profiles identify profitable paths. |
| Song integration | Deferred until engine gates pass. Implement live callbacks, independent baking, auto by default, dynamic fallback, and shared state/identity across strategies. |
| Noteskin integration | Deferred. Create `deadsync-noteskin-lua` with the shared engine, reusable programs, static actor/asset caches and necessary dynamic callbacks. |
| Correctness testing | Differential cases and deterministic generation exist. A checksum-pinned official 5.1 runner records seventeen reference and Rust passes among eighteen probes, including `attrib.lua` and `verybig.lua`. `files.lua` is retained with the Windows open-file rename failure at line 59 in both engines. A separate standalone CLI audit records remaining mismatches against a checksum-pinned official executable. Full suite passage, broader source/bytecode fuzzing and per-dialect CI remain open. |
| Performance | Initial reference harness exists. Expand throughput, startup, compile cost, allocations, live/peak memory, scaling, representative workloads and other Rust engines. Preserve samples and machine/build metadata. |
| Independent stages | The literal-folding pass is separate internally. Public compile/optimization and scene-baking stages, cached stage identities and equivalence gates remain open. |

Next work should finish the standalone engine: register allocation/resource limits, incremental collection,
remaining standard libraries, complete diagnostics/debug behavior, native export/resource edges and
cache integration/eviction. Production migration requires the complete compatibility and
performance gates; the current regression corpus does not establish them.

Validation also includes strict Clippy, a dependency tree containing only the new
crate, 520 differential/adversarial integration tests, seventy-three unit tests and
four runnable documentation examples. Generated cases include 1,000 numeric/logical
expressions, 325 metamethod-handler combinations, 5,120 arbitrary byte inputs,
4,000 byte-pattern cases, 2,000 binary replacements, 2,000 float formats, 2,000 binary-scaling cases and 2,000
radix conversion cases. Sort tests compare comparator
traces across all permutations through length six, callback mutations, nested
sorting, errors and metatables. Coroutine generation exercises 300 resume sequences
and recursive generators, alongside nested resumers, native callbacks, environments,
failed/suspended thread cells, result arity, C-call limits and quota recovery.
A separate [library argument audit](testing/2026-10-26-rust-lua-stdlib-arguments.md)
checks 5,507 bounded cases against the standalone Windows/MSVC reference. It
matches 5,493 cases after fixing `math.ldexp` validation order and retains
fourteen native-module loading failures. The audit exits unsuccessfully until
those mismatches are resolved; full standard-library parity remains open.
A [native-call boundary proof](testing/2026-10-26-rust-lua-native-boundary.md)
now compares an unchanged C module against official Lua and Rust under the
release `panic=abort` policy. A small development C shim returns Rust dispatch
errors before jumping within C, preserving Rust stack cleanup and Lua error
identity. Production native-module loading still requires full C API
stack/pseudo-index semantics, inactive/cross-thread state access, full C API
coverage, and production library ownership and loader integration. The proof
does not close that compatibility gate.
The [VM-owned native stack](testing/2026-10-26-rust-lua-native-stack.md) now
replaces capture-based temporary roots with the native frame's register window.
It supports mutable slots, an 8,000-slot bound, popping argument roots and
in-place return-hook result mutation. The
[unprotected-call path](testing/2026-10-26-rust-lua-unprotected.md) now retains
failing child frames, locals and open upvalues until an outer Lua handler runs.
The development bridge uses it for `lua_call`; recoverable Rust calls retain
their protected boundary. Pending unprotected errors reject subsequent stack
writes and escape the callback even if Rust ignores the return value. Protected
loaders inherit the enclosing error function through an unprotected boundary,
while an intervening protected call stops inheritance. C fixtures cover nested
native calls, local mutation, collection, reader errors, hooks, finalizers and
diagnostic bytes. This does not supply the remaining C API or production loader.
The [persistent thread-state proof](testing/2026-10-26-rust-lua-thread-states.md)
adds typed host resources owned by each Lua thread. Their immutable Lua roots
participate in tracing and incremental barriers; suspension preserves state and
unreachable thread cycles release it. Main-thread state survives shutdown Lua
finalizers. The C fixture retains stable state pointers across callbacks,
reentry, coroutine suspension and a VM move. It creates and destroys 68 states,
collecting 67 before VM shutdown, while DLL owners outlive state destruction.
Only the currently executing thread's C stack is accessible in this fixture;
production native loading and the complete C API remain open.
The [C closure proof](testing/2026-10-26-rust-lua-c-closures.md) now moves adapter
pointers and DLL ownership into private typed host data. Initial callbacks have
zero Lua upvalues; `lua_pushcclosure` consumes actual captures and supports the
Lua 5.1 C limit of 255. Registry, closure environment, thread globals and upvalue
pseudo-indexes support reads and valid table/upvalue replacement with collector
barriers. C fixtures exercise independent mutable counters, cyclic capture
collection, coroutine calls, inherited environments, temporary registry
replacement and exact negative-index argument errors. The runner releases its
external DLL owner before callbacks and verifies that all owners are released
after VM destruction. Complete native-module loading remains unfinished.
The [native table API checks](testing/2026-10-26-rust-lua-native-tables.md) add
creation hints, raw access, field/integer operations, metamethod-aware reads and
writes, actual metatables, traversal and stack movement. Native table actions
reuse the shared lookup and child-call scheduler, preserve fault frames and
inputs, and keep field-name temporaries outside the visible C stack. Table
metamethod continuations share their caller's C boundary instead of counting it
twice. The C runner now enters the standalone main context used by its reference;
the outer main is counted once. Both engines reach 196 ordinary recursive table
calls and 219 while handling overflow. Ten new Rust groups also cover embedded
versus standalone depth, collection, field privacy, coroutine scheduling,
metatable protection, capacity failures and quota recovery. Production native
loading and the remaining C API remain open.
GC coverage includes cyclic and weak graphs, collection inside native callbacks,
collected suspended/failed threads with escaping cells, 20,000-link graphs,
150 live locals, automatic concatenation/array pressure, stale host handles and
12,000 dynamically compiled chunks with bounded reusable storage.
Userdata coverage adds proxy validation, byte storage, environments, metamethods,
finalization order across 100 reuse batches, resurrection, weak tables, nested
collection, error handlers, C-call depth, instruction aborts and VM shutdown.
Nested collection after automatic interruption also tests completed-opcode
register maps, 500 open results and host return roots.
The [official suite report](testing/2026-10-26-rust-lua-boolean-hooks.md) identifies the
current failing files without changing or excluding their assertions. The initial
[performance baseline](performance/2026-10-24-rust-lua-baseline.md) records
execution ratios of 2.38â€“13.51 times the reference duration. Subsequent
[compatibility and dispatch measurements](performance/2026-10-24-rust-lua-dispatch.md)
recorded ratios of 2.57â€“6.63 with expanded semantics. The latest
[string/table measurements](performance/2026-10-24-rust-lua-stdlib.md) add six
workloads and confirm zero warmed allocation events for pattern search and
replacement workloads. The [coroutine measurements](performance/2026-10-24-rust-lua-coroutines.md)
add resume, wrap and deep-yield scripts. Substantial speed work and broader measurements remain
after the language/compatibility gates.

The [GC measurements](performance/2026-10-25-rust-lua-gc.md) add cyclic and weak
churn workloads, before/after collection heap estimates and reclaimed-object
counts. Automatic collection is enabled during timing. These partial workloads
do not establish complete Lua compatibility or performance parity.

The [userdata measurements](performance/2026-10-25-rust-lua-userdata.md) add proxy
construction and finalizer dispatch, bringing the harness to twenty workloads.
Host full collection now returns a result so Lua finalizer errors propagate.

The [scanner validation](testing/2026-10-25-rust-lua-stream-lexer.md) covers all
two-way splits of representative lexical input, one-byte input, 2,000 generated
binary inputs and long partial tokens. The [frontend measurements](performance/2026-10-25-rust-lua-stream-lexer.md)
add fixed-source compilation scaling from 10 KB to 1 MB. The subsequent
[reader validation](testing/2026-10-25-rust-lua-reader.md) adds parser resumption,
protected text `load`, reader/error ordering, environment changes, collection,
nested calls and quota recovery. It also checks declaration-time local limits.
The [reader measurements](performance/2026-10-25-rust-lua-reader.md) add single-chunk
and one-byte loading workloads. Binary loading and broader compilation resource
limits remain unfinished.

The [debug validation](testing/2026-10-25-rust-lua-debug.md) adds function/stack
metadata, caller-name inference and shared upvalue access. It compares active
line sets across 512 generated layouts in main/nested functions, checks 30,000
lost tail calls and retains debug results through collection. Subsequent work
adds local inspection, tracebacks and initial hooks; broader hook/source/register
parity remains open. The
[debug measurements](performance/2026-10-25-rust-lua-debug.md) add inspection to
the 23-workload runtime harness and account for prototype metadata storage.

The [local inspection validation](testing/2026-10-25-rust-lua-debug-locals.md) adds
`getlocal`/`setlocal`, compact temporary maps, native argument slots, shared-cell
mutation and GC roots. Its 512 generated assignment combinations also cover
callback-driven local changes and address conflicts. The
[measurements](performance/2026-10-25-rust-lua-debug-locals.md) expand to 24 runtime
workloads and eight compilation cases, including 1,000/10,000 short scopes with
nested calls. Private native/parser scratch slots remain outside the current
completed work; subsequent work adds hook delivery.

The [traceback validation](testing/2026-10-25-rust-lua-traceback.md) adds current and
other-thread formatting, message/level coercions, deep-stack truncation and
callback-visible concatenation scratch slots. Formatting uses reusable flat jobs
and native register roots, preserving arbitrary error objects and nested calls.
Error/quota cleanup releases discarded stacks while preserving host error payloads.
Twenty thousand traces test reclamation and bounded storage. The
[measurements](performance/2026-10-25-rust-lua-traceback.md) add a 25th execution
workload with checked complete traceback bytes; eight compilation cases remain.

The [hook validation](testing/2026-10-25-rust-lua-hooks.md) adds per-thread hook
controls and non-recursive call/return/tail-return/line/count delivery. Compiler
hook maps omit synthetic copies and combined branches. Explicit comparison-result
steps preserve mutations between comparison and materialization, with numeric
fusion when instruction hooks are off. Single-local returns retain their actual
register; relocatable single assignments avoid an extra copy. Initially nil
markers preserve call-hook changes, and hook-visible register writes remain GC
roots during entry callbacks. The [measurements](performance/2026-10-25-rust-lua-hooks.md)
add a 26th runtime workload. Full instruction-event parity, exact PUC scratch-slot
bounds and broader hook resource/fuzz coverage remain open.

Finalizers suppress hooks on their own thread, preserve pending call events during
automatic collection, and leave resumed children's hook controls independent.

The [I/O validation](testing/2026-10-25-rust-lua-io.md) adds buffered file records,
native userdata/iterator roots, default environments, process pipes and the shared
registry. Twenty-two new groups include 330 numeric stream cases, Windows newline
and seek-cookie checks, real CLI streams and 1,000 temporary files with bounded
reusable storage. The [measurements](performance/2026-10-25-rust-lua-io.md) expand
to 27 runtime workloads plus eight compilation cases. The unchanged `db.lua`
probe now passes. Path encoding, exact standard-stream buffering, device/error
coverage and blocking-I/O cancellation remain open. Closed stream records follow
userdata lifetime; reclaiming their storage earlier and bounding large write
buffers remain performance work. The registry's C-private entries and broader
numeric-string conversion parity also need further coverage.

The [file-loading validation](testing/2026-10-25-rust-lua-file-load.md) adds text
`loadfile`/`dofile`, named-reader roots and cleanup, shared underlying stdin and
initial executable headers. Fifteen groups cover extra arguments, environments,
hooks, errors, nested C-call limits, six isolated stdin cases, thirty-one quota
aborts and 1,000 successful reloads. The parser deducts active native-call depth
from its 200-level syntax budget; lexer diagnostics use their 80-byte source-ID
buffer separately from debug/runtime descriptions. The
[measurements](performance/2026-10-25-rust-lua-file-load.md) add a 28th runtime
workload. Host errors also expose the actual raised file-error string. Binary
chunks, complete syntax diagnostics and compiler depth/resource
parity remain open, along with the broader standalone and integration gates.

The [package validation](testing/2026-10-25-rust-lua-package.md) adds import
caching, preloaded/Lua-file/custom searchers, path defaults and 5.1 module
initialization/options. Retained native slots preserve callback mutations without
aliasing captured names or duplicated module tables. Light userdata adds opaque
unmanaged identities, weak-table behavior and independent per-VM metatables.
Twenty-five groups cover these semantics, nested native-call limits, environment
expansion in child processes, instruction aborts and 2,000 transient imports.
The unchanged `math.lua` probe now passes. The
[measurements](performance/2026-10-25-rust-lua-package.md) add cached and preloaded
imports, bringing the harness to thirty execution workloads and eight frontend
cases. Cached imports allocate nothing after warmup but remain slower than C;
removing dispatch work remains a profiling target. Native C modules, exact native
scratch footprints and broader package/resource diagnostics remain open.

The [source-diagnostic validation](testing/2026-10-25-rust-lua-syntax.md) adds
sixteen groups with 13,439 reference observations: expected/near token text,
opening lines, raw byte diagnostics, lookahead, local/assignment bounds, reader
request ordering, file inputs and the reference's embedded-NUL lexical behavior.
Cold token-span descriptors avoid copying diagnostic context on successful
scans. Safe eight-byte checks scan long tokens, with byte lookup only in marked
chunks or the tail. The [measurements](performance/2026-10-25-rust-lua-syntax.md)
add a syntax-error workload, bringing the harness to 31 execution workloads and
eight frontend cases. Broader runtime operand provenance, remaining compiler bounds,
binary chunks and the broader standalone-engine gates remain unfinished.

The [runtime-error validation](testing/2026-10-25-rust-lua-runtime-errors.md)
adds ten groups covering operand names, anonymous/redirected values, logical
trees, scoped locals, callbacks, coroutines, raw names and comparison wording.
Names are reconstructed from verified producers and existing debug metadata on
the cold error path, without new per-instruction provenance storage or changes
to successful dispatch. The [measurements](performance/2026-10-25-rust-lua-runtime-errors.md)
add a protected runtime-error workload, bringing the harness to 32 execution
workloads and eight frontend cases. Native argument diagnostics and wider
compiler/runtime/debug compatibility remain open.

The [native argument validation](testing/2026-10-25-rust-lua-native-errors.md)
adds seven groups with 362 reference observations across forty native functions,
aliases, method/bad-self numbering, missing versus nil arguments, validation
order, options, file errors, callbacks, coroutines and hook-mutated arguments.
Shared cold formatters use existing call metadata and preserve raw byte names.
Base table/metatable checks follow the reference's failure order. The
[measurements](performance/2026-10-25-rust-lua-native-errors.md) add a protected
native argument-error workload, bringing the harness to 33 execution workloads
and eight frontend cases. The official error probe reaches the Lua call-stack
limit; catchable overflow behavior, depth/reserve parity and broader resource
limits remain open with the other standalone-engine requirements.

The [call-limit validation](testing/2026-10-25-rust-lua-call-limits.md) adds sixteen
groups and 47 paired programs covering exact stopped-GC recursion counts,
repeated recovery, error-handler reserve/re-entry, C-call emergency callbacks,
near-capacity calls, explicit GC shrinking, coroutine state, surviving cells,
deep tracebacks, call hooks, tail calls and instruction-abort recovery. A per-thread
`u16` records logical call-info capacity; physical vector length bounds the common
check and visible frames are counted only near a limit. Native callbacks perform
their C-boundary check separately from ordinary Lua calls. Fatal handling errors
bypass handler re-entry; the scheduler retains faulting frames until recovery.
The [measurements](performance/2026-10-25-rust-lua-call-limits.md) add a protected
overflow workload, bringing the harness to 34 execution workloads and eight
frontend cases. Backing vectors retain reusable capacity independently of the
logical model. Wider compiler/resource, debug, allocation-failure and incremental
GC parity remain open.

The [capture-limit validation](testing/2026-10-25-rust-lua-captures.md) adds ten
groups with 398 paired loader observations, including 256 generated lexical
layouts. Identifier spans are interned in flat storage; scopes share a local
stack and capture nodes are reused after function exit. Checks propagate captures
from the owner through descendants, before a complete AST is required, while
waiting for the reference's next-token lookahead. Repeat-body locals remain in
scope through the condition. The unchanged official `errors.lua` now passes.
The [measurements](performance/2026-10-25-rust-lua-captures.md) add a capture-limit
workload and closure-heavy compilation scaling: 35 execution workloads and ten
frontend cases. This milestone exposed host-stack overflow on deeply nested valid
code generation, addressed by the continuation-based compiler below.

The [flat compiler validation](testing/2026-10-25-rust-lua-flat-codegen.md) adds five
groups with 273 paired execution programs on 512 KB Rust thread stacks. Blocks,
expressions, argument/return lists, table fields and nested function compilation
use one flat continuation stack; ancestor capture propagation is iterative.
Tested sources reach 190 nested functions/scopes and 20,000-term logical chains.
The artificial recursive-compiler depth guard is removed; source syntax limits
remain enforced by the parser. The earlier capture corpus now also compiles its
complete 59- and 60-capture nested fixtures. The
[measurements](performance/2026-10-25-rust-lua-flat-codegen.md) include nested-function
and long-expression frontend cases, bringing the harness to 35 execution and
twelve frontend workloads. Exact register allocation/resource limits and broader
diagnostics remain required compiler work.

The [register-reuse validation](testing/2026-10-25-rust-lua-register-reuse.md)
adds seven groups with 92 paired execution/error observations. Arithmetic and
index expressions reuse anonymous result slots and allocate right-hand scratch
storage after the left finishes. Named destinations remain unchanged during
callbacks and metamethods, including unary assignment. Ten-thousand-term
arithmetic and index chains now compile with bounded entry register counts;
190-term right-associated concatenation also fits. Tests preserve number bits,
late local reads, captured intermediates, GC roots and complete diagnostics.
The [measurements](performance/2026-10-25-rust-lua-register-reuse.md) add an operand
workload and two long-chain frontend cases, bringing the harness to 36 execution
and fourteen frontend workloads. The reports also retain instruction counts and
entry register counts. Exact compiler resource limits and other expression
layouts still require work. This milestone exposed excess temporary use in deep
constructors and chained calls, addressed by the following change.

The [call/constructor validation](testing/2026-10-25-rust-lua-call-table-slots.md)
adds seven groups with 182 paired observations. Top anonymous result slots are
reused; arguments reserve storage after the callee, one at a time. Methods retain
their receiver before lookup/arguments. Compact literal record keys retain pool
insertion order but materialize after field values. List/record/key constructors
reach 190 levels, and ordinary/method chains reach 10,000 calls. Tests cover
open results, snapshots, GC roots, errors, large constant pools and fifty-item
list flushing. The earlier 190-constructor and 300-call failure probes now load.
The [measurements](performance/2026-10-25-rust-lua-call-table-slots.md) expand to
37 execution and seventeen frontend workloads. Exact reference register budgets,
remaining resource diagnostics and broader scratch/debug parity remain open. A
249-argument `print` loader probe was still accepted here and rejected by C;
both accepted 248. The following milestone closes that logical-budget gap.

The [register-budget validation](testing/2026-10-25-rust-lua-register-budget.md)
adds eight groups with 3,624 protected loader observations and five executed
programs. Flat 16-byte descriptors track Lua 5.1 logical register use while
parsing, independently of physical bytecode storage. Constant identities, list
flushes, open arguments, conditionals and assignment conflicts preserve the
reference's error token and reader consumption. A request reaching 250 logical
slots fails during parsing. Physical result slots grow as expressions finish,
allowing 199-result assignments with 247 call arguments and deep first results.
The [measurements](performance/2026-10-25-rust-lua-register-budget.md) cover
37 runtime and eighteen frontend workloads, with parent comparison and explicit
accounting costs. Full physical allocation parity remains open: 150 literal-key
indexed assignment targets still exhaust Rust storage in a reference-valid
program. Broader resource/debug/scratch behavior remains unfinished.

The [indexed-target validation](testing/2026-10-25-rust-lua-index-targets.md)
adds three groups with 155 paired programs. Local table/key references reserve
no unused slots; compact literal keys retain pool insertion order and materialize
in one scratch slot immediately before each reverse-ordered store. Tests cover
up to 200 targets, snapshots/conflicts, pending objects during GC, coroutine
returns, mixed generated targets, large pools and multiline error locations.
The original 150-target probe now executes. A prefixed variant whose backend
constant pool no longer has compact keys still exhausts physical storage, so
complete allocation parity remains open. The
[measurements](performance/2026-10-25-rust-lua-index-targets.md) cover
38 runtime and nineteen frontend workloads.

The [reference-constant validation](testing/2026-10-25-rust-lua-reference-constants.md)
adds four groups with 159 executed programs and 360 loader observations. Sparse
AST metadata carries reference constant operands for deferred assignment and
constructor keys, including folded logical/boolean keys, without copying string
bytes. Read-only index chains retain no key metadata. Per-function zero signs
prevent extra backend constants from changing reference signed-zero pooling.
Logical pool accounting now includes equal-length local assignment stores. The
prefixed 150-target failure now executes in both engines. A source with 100
locals and 190 right-associated literal powers remains reference-valid and
exhausts backend storage. The
[measurements](performance/2026-10-25-rust-lua-reference-constants.md) cover
38 execution and twenty frontend workloads, including metadata overhead.

The [deferred-operand validation](testing/2026-10-25-rust-lua-deferred-operands.md)
adds six groups with 421 executed programs and 512 loader observations. Numeric
arithmetic left operands and reference compact constants for arithmetic and
comparisons materialize after the right result. Constant-pool insertion remains
before right-hand code generation, preserving backend pool ordering and the
existing reference zero-sign metadata. Named destinations retain their old
values through callbacks; dynamic/noncompact operands retain ordinary evaluation.
Small numeric/variable trees retain grouped loads when a cached 32-node bound
fits the remaining register file, avoiding interleaved-load overhead. The hint
uses existing AST span padding on 64-bit targets.
The 190-power numeric and string probes now execute, and numeric chains fit with
200 active locals. Generated pressure cases compare acceptance and diagnostics
with large pools and mixed expression forms. The
[measurements](performance/2026-10-25-rust-lua-deferred-operands.md) cover
39 execution and twenty-two frontend workloads. Complete resource/debug/scratch
parity and broader standalone compatibility remain open.

Pattern programs use compact instructions and class bitmaps. Backtracking,
replacement callbacks and sort comparisons resume through explicit VM state.
Scratch buffers and bounded sort partitions avoid per-match/partition allocation.
Native jobs trace their retained values; pattern caches are evicted when their
source string dies. Library error text, numeric conversions, resource behavior and
non-C locales still need broader validation; current coverage is not a complete
standard-library compatibility claim.

The math library uses bit-based `frexp`/`ldexp`, including subnormal tie rounding,
and preserves `modf` result arity and signed zero. Radix conversions follow the
platform unsigned-long width and saturation. The pinned MSVC random stream is
shared by Lua states on each OS thread, as in the Windows CRT; erroring random
calls still advance it. Other CRT random streams are not emulated. Differential
checks pin the Windows MSVC/C-locale reference and do not establish cross-libm
transcendental equivalence.

Coroutine switches transfer ownership of vectors without copying stack contents.
Open cells include their owning thread ID. Active registers remain direct VM
fields, and shared-heap references do not adjust reference counts. Yield-boundary
depth is inherited in call frames; fixed result counts use 16 bits and retain an
open-result sentinel. A compile-time assertion keeps frames at 64 bytes on 64-bit
targets; values remain 16 bytes in measurements. The global instruction budget
covers coroutine execution and aborts active resumer chains before restoring main.

The collector traces active/normal/suspended stacks and native continuation
state. Cold register maps distinguish live locals/results from dead temporaries;
the interpreter does not inspect these maps outside collection. Surviving cells
in an unreachable thread are closed before its storage is freed. Cell-owner IDs
do not retain entire threads. Failed thread frames remain available for later
debug inspection while their thread remains reachable.

The host must pin retained values. A live host `Program` keeps its per-VM mutable
entry binding; a separate weak lifetime token lets dynamically loaded chunks and
unused entries die while escaped Lua closures retain their immutable code.
Generation exhaustion retires an arena slot rather than reusing an old identity.
Compiler/VM storage uses safe Rust; OS operations have scoped Windows and POSIX
clock/calendar system-API bridges. There are no normal/build dependencies. Commit each
verified milestone while keeping unrelated edits separate.

The [OS validation](testing/2026-10-25-rust-lua-os-basics.md) covers
`difftime`, `getenv`, `remove` and `tmpname`. The partial library shares the rooted
builtin-module registry and existing native error/environment machinery.
Windows time differences retain integer conversion, second-argument-first
validation and negative-time rejection. I/O and OS paths share borrowed path
conversion. Windows environment/path checks currently cover ASCII; broader
calendar/time-zone, locale, rename and further process behavior remain open. The
[measurements](performance/2026-10-25-rust-lua-os-basics.md) add two workloads
and retain 41 runtime and twenty-two frontend measurements.

The [process validation](testing/2026-10-25-rust-lua-process.md) adds Lua 5.1
shell execution and process exit. Seventy-four isolated cases compare process
statuses, output bytes and generated files across the reference, Rust runner and
CLI. Shared stdout buffering preserves ordering around child execution.
The [measurements](performance/2026-10-25-rust-lua-process.md) retain the safe
standard-library process baseline, the Windows bridge and 43 runtime workloads.
Remaining OS, encoding, terminal and host-process resource behavior stays open.

The [clock/rename validation](testing/2026-10-25-rust-lua-clock-rename.md)
retains native clock epoch/units, bracketed readings and Windows rename behavior
that preserves existing destinations. Native path conversion uses initialized
stack storage for common names and complete spill storage for longer ones.
The official sort probe now passes. Nine new regression groups, 128 controlled
rename/create races and 76 isolated process cases pass; other target builds are
compile checks. The [measurements](performance/2026-10-25-rust-lua-clock-rename.md)
retain 45 runtime and twenty-two frontend workloads.

The [calendar validation](testing/2026-10-25-rust-lua-calendar.md) adds `os.time`
and `os.date`, ordered metamethod field reads, native normalization without input
table changes, local/UTC output, native format handling and DST process probes.
Thirteen regression groups include field-value finalization, callback C-call
limits and 512 generated calendars. All 340 tests and 93 isolated process probes
pass; the official result remains twelve of sixteen because locale support and
binary dumps still stop the remaining files. `os.setlocale` is the missing OS
function. Windows calendar calls have runtime comparisons; six additional target
builds are compile checks only. The [measurements](performance/2026-10-25-rust-lua-calendar.md)
retain 48 runtime and twenty-two frontend workloads. Broader locale,
OS/device/encoding behavior and the remaining engine gates stay open.

The [calendar table validation](testing/2026-10-25-rust-lua-calendar-records.md)
adds exact record capacity with initial GC accounting and a mutation/traversal
regression. All 341 tests and 93 process probes pass. In two
[paired comparisons](performance/2026-10-25-rust-lua-calendar-records.md),
ordinary date-table construction takes 27–31% less time than the committed parent,
eliminates three reallocations per table and reduces requested bytes by 64.5%.
Adding a field still improves in the measured fixture but uses more retained
entry capacity. The official result remains twelve of sixteen; standalone
compatibility and incremental collection remain unfinished.

The [GC control validation](testing/2026-10-25-rust-lua-gc-controls.md) fixes
explicit collection/step resumption, restart thresholds, deferred pause changes
and finalizer control restoration across successful returns and errors. Nine new
regression groups include 64 generated control sequences, coroutine suspension,
hooks and the Rust host API. All 350 tests and 105 process probes pass. The
retained parent fails eight of twelve new process cases; current Rust matches
all twelve. The [measurements](performance/2026-10-25-rust-lua-gc-controls.md)
retain 49 runtime and twenty-two frontend workloads. The phase gap discovered
here is covered by the following sweep milestone. Official-suite results remain
twelve Rust passes and fifteen reference passes among sixteen files.

The [GC sweep validation](testing/2026-10-25-rust-lua-gc-sweep.md) introduces
explicit pause/sweep/finalize phases, bounded arena-slot reclamation, protection
for new allocations and string resurrection, and cluster repair during string
removal. The finalizer-error phase regression and zero/negative step-budget
cases now match the reference. Six new test groups cover budget bounds, slot
reuse, wrapped hash clusters, mutable graphs, builtin pins and nested finalizers.
All 356 tests and 109 process cases pass; the retained parent fails three new
process cases. The [measurements](performance/2026-10-25-rust-lua-gc-sweep.md)
retain 50 runtime and twenty-two frontend workloads. Marking, weak processing,
cache compaction and callback batches still need incremental work. The complete
standalone engine and integration gates remain open.

The [incremental marking validation](testing/2026-10-25-rust-lua-gc-mark.md)
adds a persistent marking frontier and barriers for table edges, environments,
metatables, closures and shared cells. Final root, stack, open-cell and metatable
refreshes preserve mutations before weak processing. Full collection restarts
partial marking so floating garbage can be reclaimed. Seven new unit groups
exercise the same active cycle; a differential group combines mutation with
coroutines and finalizers. All 364 tests and 111 process probes pass. The
[measurements](performance/2026-10-25-rust-lua-gc-mark.md) retain 51 runtime and
twenty-two frontend workloads. Individual object scans, final root/weak work,
cache rebuilding and callback batches still contain atomic work. Exact PUC work
units and fully bounded pauses remain open, as do the complete standalone
engine and integration gates.

The [locale validation](testing/2026-10-25-rust-lua-locale.md) supplies the last
missing Lua 5.1 OS function on Windows/MSVC and connects numeric conversion,
formatting, stream scanning, character classes, casing, identifiers, patterns
and collation. Native locale storage is copied under the CRT locale lock;
ordinary byte work and number formatting retain Rust fast paths. Pattern caches
track shared ctype changes, including callbacks and existing iterators. Two new
integration groups and fifteen isolated locale process cases pass, bringing
the totals to 366 tests and 126 process probes. Unchanged `literals.lua` and
`strings.lua` now pass, making fourteen of sixteen official Rust probes; the
reference retains fifteen. Both fail the Windows file rename assertion at line
59, and Rust still lacks binary dumps. The [measurements](performance/2026-10-25-rust-lua-locale.md)
retain 53 runtime and twenty-two frontend workloads. Non-Windows native locales,
broader encodings, external host locale synchronization, binary chunks/modules
and the remaining engine/integration gates are unfinished.

The [binary codec validation](testing/2026-10-25-rust-lua-chunk-codec.md) adds
resumable native Lua 5.1 decoding and structural checking to fixed-buffer,
reader and file loading. Flat prototype/continuation vectors avoid recursive
decoding and input replay. Counts do not reserve untrusted declared sizes;
SETLIST payload discovery is linear. Official dumps cover all 38 native opcodes
and round-trip their bytes; split/truncated/corrupt input, reader stopping points
and binary file reopening are tested. All 375 tests and 129 process probes pass.
The [measurements](performance/2026-10-25-rust-lua-chunk-codec.md) retain 54 runtime
and twenty-two frontend workloads. Valid native chunks still report unsupported
execution: instruction translation and dump emission remain required. Official
suite results stay fourteen Rust passes and fifteen reference passes among
sixteen files. This is a decoding/verification stage, not binary execution,
native-module loading or the persistent compiled cache.

The [native execution validation](testing/2026-10-25-rust-lua-chunk-exec.md)
adds translation of all 38 native opcodes to the existing VM. Closures use
per-site bindings; imported functions retain native source names, local ranges,
upvalues, lines and dispatch hook counts. Range concatenation preserves numeric
coercions and callback order. Generic-for calls retain native yield and C-depth
boundaries for imported chunks and ordinary source loops. Collector maps exclude dead translation scratch slots and stale
caller argument copies. Fixed buffers, Lua loaders, binary files and the CLI
share execution. Four additional unit groups include 256 generated programs,
stripped chunks, native metadata and a 25,551-element constructor. A source
regression checks iterator yield/depth boundaries in both thread contexts. The
[measurements](performance/2026-10-25-rust-lua-chunk-exec.md) add three imported
chunk workloads. This remains incremental coverage; dump emission, broader
binary diagnostics/collector edges and the full standalone-engine gates remain
open. Production consumers still use the legacy crate.

The [function dump validation](testing/2026-10-25-rust-lua-chunk-dump.md)
adds native Lua 5.1 export for source-compiled functions and retains original
native words/debug offsets for imported-function re-export. A flat traversal
emits prototype trees without recursive Rust calls. Lowering reconstructs native
comparison/method/iterator forms, capture bindings and compacted iterator scopes.
Dumped roots omit captured values and environments. Extra arguments are ignored;
return hooks see the function, result and live registry hook table. That table
now backs hook lookup/delivery and accepts observable mutation through debug.
All 388 tests and 136 process probes pass. Unchanged `calls.lua` now passes,
giving fifteen of sixteen Rust and reference probes; both retain the Windows
open-input rename failure. The [measurements](performance/2026-10-25-rust-lua-chunk-dump.md)
retain sixty runtime and twenty-two frontend workloads, adding source dumping,
native re-export and dump/load execution. Native encoding limits, broader debug,
collector/resource cases and the remaining standalone/integration gates remain open.

The [long-branch export validation](testing/2026-10-25-rust-lua-chunk-jumps.md)
adds guarded native jump relays after source lowering expands branches beyond
Lua 5.1's signed encoding range. Batched relocation keeps closure bindings,
extended SETLIST payloads, skip pairs and open-result consumers intact. Relays
reserve no registers; their debug lines and named locals describe the originating
branch. Export also omits CLOSE operations without a live captured local.
All 393 tests and 138 process probes pass. The sixteen unchanged official files
retain fifteen passes in each engine and the shared Windows rename failure.
Two [measurement runs](performance/2026-10-25-rust-lua-chunk-jumps.md) cover
sixty-two runtime and twenty-two frontend workloads. Register/operand limits,
broader debug/collector cases and the full standalone-engine gates remain open.

The [source concatenation validation](testing/2026-10-26-rust-lua-concat.md)
extends eligible right-associated chains into the shared native-style range
operation. Buffered string groups avoid repeated pairwise intermediate copies;
metamethods preserve operand order and callback roots. Parenthesized left groups
and logical paths retain their evaluation boundaries. Copied operands report
their operator's source line, and known conditional edges bypass an opposite
test of the same register. All 398 tests and 140 process probes pass, including
256 generated concatenation/logical contexts and exact line/count hook probes.
The official file results remain fifteen of sixteen in both engines. The
[measurements](performance/2026-10-26-rust-lua-concat.md) retain a compiler
baseline and two final runs over sixty-four runtime and twenty-two frontend
workloads. Full standalone compatibility, broader resource/collector/debug
coverage and later dialects remain unfinished.

The [standalone entry validation](testing/2026-10-26-rust-lua-cli.md) adds host
calls and argument-bearing chunk entry through the shared driver, raw table
construction and direct protected file loading. The CLI supports chained command
and module actions, startup code/files and Lua 5.1 script argument indexing.
Argument roots survive initialization callbacks and loading without retaining a
replaced argument table during script execution. All 407 crate tests and 140
existing process probes pass. The official file corpus now includes unchanged
`attrib.lua` and `verybig.lua`, bringing both engines to seventeen of eighteen.
The new standalone CLI audit deliberately retains interactive, traceback,
outer C-frame hook, banner and usage differences. It exits unsuccessfully;
complete standalone compatibility remains a required gate. Two
[measurement runs](performance/2026-10-26-rust-lua-cli.md) cover sixty-five runtime
and twenty-two frontend workloads, including typed argument entry. The full
engine, later dialects and integrations remain unfinished.

The [standalone message-handler validation](testing/2026-10-26-rust-lua-cli-traces.md)
keeps an observable parked C main frame across CLI actions and runs the current
debug.traceback before unwinding failures. Managed native closures preserve
main/handler identity, environments and GC ownership. Nil handler results stay
silent; failed handlers, loader-reader errors and explicit post-error collection
match the expanded reference probes. load_program lets the CLI release its host
Program before calling, so failed chunk environments can be collected at the
reference point. All 414 crate tests and 140 process probes pass. The direct CLI
audit now matches sixty of sixty-six cases; usage text, project branding and
interactive mode remain different, and its command still fails. The official
file results remain seventeen of eighteen in both engines. Two
[measurement runs](performance/2026-10-26-rust-lua-cli-traces.md) cover sixty-six
runtime and twenty-two frontend workloads, including standalone entry overhead.
Complete standalone compatibility and the full engine/integration goal remain open.

The [console validation](testing/2026-10-26-rust-lua-console.md) adds `-i` and
automatic terminal entry, expression results, continued chunks, prompt callbacks,
shared stdin and recovery after chunk/print failures. Host global lookup honors
`__index`; previous console results remain rooted during print lookup callbacks.
The message-handler-free print call reuses the protected entry driver. Usage
diagnostics match the reference. All 415 crate tests and 140 process probes pass;
both engines still pass seventeen of eighteen official files. The expanded CLI
audit retains 72/110 raw matches and 110/110 after removing one exact leading
version banner; all thirty-eight raw differences are project branding. This is
not full terminal/signal or standalone compatibility. Complete process timing
and two [engine measurement runs](performance/2026-10-26-rust-lua-console.md)
retain source hashes, outputs and remaining limitations. The full goal stays open.

The [native integer/result validation](testing/2026-10-26-rust-lua-native-bounds.md)
narrows select/unpack/character/repetition arguments through the reference's
64-bit Lua integer and 32-bit native int conversions. Unpack and byte results
honor the 8,000-slot native limit, counting unused arguments and preserving
catchable errors. Seven groups cover numeric strings, non-finite/platform edge
values, 126 generated arguments across four functions, result boundaries,
error precedence and collection with 7,999 unpacked objects. All 422 tests and
140 process probes pass. The direct CLI audit retains 88/126 raw matches;
all thirty-eight differences are the project banner, and all 126 match after
only that leading banner is removed. Both engines still pass seventeen of
eighteen official files. Two [measurement runs](performance/2026-10-26-rust-lua-native-bounds.md)
cover 72 runtime and 22 frontend workloads, including result-width scaling and
failure paths. Complete standalone compatibility and the full goal remain open.

The [native work validation](testing/2026-10-26-rust-lua-work-limits.md) removes
synthetic ten-million-step limits from pattern searches and table concat/insert.
Finite backtracking and late malformed-pattern errors remain reachable. Sort now
checks its 40-slot native reserve, normalizes the comparator slot and releases
unused argument roots before comparison; host-entry padding stays separate from
scratch storage. All 430 tests and 140 process probes pass. The CLI audit retains
112/150 raw matches; all thirty-eight differences are the project banner, with
150/150 after only its leading banner is removed. Official isolated files remain
17/18 in both engines. Two [measurement runs](performance/2026-10-26-rust-lua-work-limits.md)
cover 76 runtime and 22 frontend workloads. The measured x64 VM struct shrinks
from 4,440 to 4,432 bytes. Native loops still run between execution-budget checks;
full resource, standalone and integration gates remain open.

The [live native argument validation](testing/2026-10-26-rust-lua-native-args.md)
reloads sort table/comparator slots after callbacks and removes their stale job
roots. Traversal returns before rechecking unused mutated arguments and keeps
iteration state below callable/wide-result callback storage. Native operand
errors preserve C-frame origin; Lua errors keep source locations. Missing
protected-call slots retain reference argument diagnostics. All 441 tests and
140 process probes pass. The CLI audit retains 124/162 raw matches, with all
thirty-eight differences limited to the version banner; 162/162 match after only
that leading banner is removed. Official files remain 17/18 in both engines.
Two [measurement runs](performance/2026-10-26-rust-lua-native-args.md) cover 78 runtime
and 22 frontend workloads. Full native/debug, resource, standalone and integration
gates remain open.

The [debug command console](testing/2026-10-26-rust-lua-debug-console.md) adds
`debug.debug` through the existing native continuation driver. Commands inspect
and change the suspended caller, share original stdin, use stderr prompts and
diagnostics, and recover under a handler-free protected boundary. Fixed input
fragments, exact exit input, argument/root cleanup, nested sessions,
thread environments, hooks and coroutine restrictions have independent CLI
probes. All 449 tests and 140 process probes pass. The CLI audit has 142/180 raw
matches and 180/180 after only the known version banner is removed. Official
files stay 17/18 in both engines. Two [pairs of measurements](performance/2026-10-26-rust-lua-debug-console.md)
cover ten complete-process workloads and 78 runtime/22 frontend cases; VM
metadata remains 4,432 B. Complete standalone compatibility remains open.

The [native conversion and substitution milestone](testing/2026-10-26-rust-lua-native-coercions.md)
coerces numeric string arguments in place and separates inspected arguments from
native result writes. Gsub observes changed replacement slots, releases obsolete
roots and models its partial-buffer stack. Buffer concatenations can suspend
through Lua metamethods using flat pending actions, including nested calls,
collection, errors and instruction aborts. All 461 tests and 140 process probes
pass. The CLI audit retains 154/192 raw matches and 192/192 after only the known
version banner is removed; official files remain 17/18 in both engines. Two
[measurement runs](performance/2026-10-26-rust-lua-native-coercions.md) cover 81
runtime and 22 frontend workloads. VM metadata stays at 4,432 B; complete
compatibility and the full engine/integration gates remain open.

The [interning hash milestone](testing/2026-10-26-rust-lua-intern-hash.md) processes
long strings through four independent word lanes with safe loads. Every byte
participates and full byte equality resolves collisions. No string/VM field is
added. New regressions cover binary identity, unaligned slices, table growth,
sweeping and reused object slots. All 465 tests pass. Paired comparisons against
the committed parent and [reference measurements](performance/2026-10-26-rust-lua-intern-hash.md)
record about 2.2 times faster wide callback execution and 13 times faster 4 KiB
intern hits, with identical measured allocation/heap footprints. All 140 process
probes pass. The CLI audit has 158/196 raw matches and 196/196 after only its
known banner is removed; official files remain 17/18 in both engines. The full
standalone engine and integration gates remain open.

The [table propagation milestone](testing/2026-10-26-rust-lua-table-slices.md)
limits each table work item to 64 physical array/hash slots, including holes and
tombstones. Continuations resume after child edges. Compaction, hash-to-array
moves and metatable changes restart active scans; final metatable refreshes
observe weak-mode changes. The table header remains 88 B, VM 4,432 B and Value
16 B. [Paired measurements](performance/2026-10-26-rust-lua-table-slices.md)
report step latency, complete-cycle cost, retained frontier capacity, allocation
events and process CPU counters. Final root/metatable/weak work and other object
scans still contain atomic work, so collector pauses are not fully bounded.
The full standalone-engine and integration gates remain open.

The [persistent-program milestone](testing/2026-10-26-rust-lua-program-cache.md)
adds a disk cache for the complete shared compiler representation, including
native chunk metadata. Exact source/name checks, compiler/target/locale identity,
bounded decoding, structural verification and temporary-file replacement govern
reuse. Cache failures retain ordinary compilation and source errors. Programs
reload in fresh processes and retain independent VM bindings. [Measurements](performance/2026-10-26-rust-lua-program-cache.md)
compare compilation, stores and reloads with allocation/heap and process CPU
counters; tiny programs and native chunks are not assumed to benefit. Cache
eviction and package/CLI integration remain open alongside the full engine gates.

The [cache checksum milestone](testing/2026-10-26-rust-lua-cache-crc.md) advances
CRC32C through eight-byte blocks with safe unaligned loads and an 8 KiB static
lookup table. An independent bitwise calculation covers binary inputs, tails
and offsets. [Comparisons with the committed parent](performance/2026-10-26-rust-lua-cache-crc.md)
measure checksum and full cache calls, allocation/heap footprints and process
CPU; both engines also cross-read byte-identical artifacts under a shared
benchmark-only build identity. Normal Cargo build invalidation remains in place.
The full standalone-engine and integration gates remain open.

The [compiler constant-pool milestone](testing/2026-10-26-rust-lua-constant-pool.md)
adds a temporary flat index once a function reaches 64 constants. Borrowed
string lookups copy only new values. Append order, full-byte equality, numeric
zero pooling and distinct NaNs preserve the existing compiler output. The index
is dropped at function finalization. [Paired measurements](performance/2026-10-26-rust-lua-constant-pool.md)
cover scaling, allocations, tracked live layout sizes and process CPU. Complete
compiled artifacts remain byte-identical to the parent under the benchmark's
shared build ID. Runtime layouts and the full standalone-engine gates remain
unchanged.

The [format diagnostic validation](testing/2026-10-26-rust-lua-format-diagnostics.md)
checks 544 binary/malformed format cases, seven native error contexts and four
new standalone CLI probes. A terminal or NUL conversion now omits the diagnostic
NUL as Lua 5.1 does. An isolated comparison reproduces four mismatches in the
committed parent and six current/reference matches, including two unchanged
controls. Literal NULs in format text remain in successful output. This advances
standard library error compatibility; the complete standalone-engine gates remain open.

The [Rust callback milestone](testing/2026-10-26-rust-lua-host-functions.md)
registers native function pointers with borrowed argument ranges, separate result
writes and traced captures. Fifteen integration groups cover collector barriers,
error identity, hooks, coroutine callers, finalizers, preload modules and cached
programs with independent VM bindings. Rust unwinds bypass Lua handlers and
disable Lua finalization on the interrupted VM. [Measurements](performance/2026-10-26-rust-lua-host-functions.md)
compare warmed callbacks with the development C bridge, including allocations,
heap observations and process CPU.

The [nested callback milestone](testing/2026-10-26-rust-lua-host-reentry.md)
adds HostCall::call and call_and_return over a hidden protected continuation.
The live Rust callback and its original arguments stay on the VM stack. Runtime
errors return to Rust with their identities; child execution shares one outer
instruction counter, and resource failures cannot be swallowed by Rust or Lua
handlers. Cleanup returns to the owning coroutine and preserves parent locals,
upvalues and result roots. C API differential probes cover native stack slots,
hooks, handlers, finalizers and child coroutine behavior. A shared remaining-count
field adds eight bytes to Vm; Value and call-frame sizes stay unchanged.
[Measurements](performance/2026-10-26-rust-lua-host-reentry.md) include protected
Rust-to-Lua calls and their C lua_pcall comparison. Suspending Rust callback stacks,
richer host objects and complete standalone compatibility remain open.

The [unary hook milestone](testing/2026-10-26-rust-lua-unary-hooks.md) removes
redundant local-operand copies, folds constant logical negation and combines
comparison negation with its final boolean materialization. Count/line hooks
retain the comparison's original operation, metamethod order and NaN behavior;
callbacks between comparison and materialization cannot change the completed
predicate. Exported Lua 5.1 chunks and fresh-process private-cache reloads
preserve these results. Three new CLI cases reproduce failures in the preserved
parent and pass against the official reference. [Measurements](performance/2026-10-26-rust-lua-unary-hooks.md)
compare current, parent and C CLIs with process CPU counters, alongside the
broader runtime, frontend, callback and cache harnesses. Complete hook parity and
the other standalone gates remain open.

The [boolean value milestone](testing/2026-10-26-rust-lua-boolean-hooks.md)
preserves short-circuit hook events under repeated logical negation. A compact
compiler descriptor carries the final operand and pending true/false edges;
flat jobs append/join edge chains in instruction slots and patch them once.
Local conditions read after their callbacks, and final boolean writes preserve
named destinations. Verified comparison branches fuse when instruction hooks
are disabled. Native export preserves comparison/JMP and boolean skip adjacency,
including long-branch relay insertion. Expanded source/native/cache probes and
[parent/current/C process measurements](performance/2026-10-26-rust-lua-boolean-hooks.md)
cover both branches, local mutation, GC, metamethods and long chains. Complete
hook parity and the broader standalone-engine gates remain unfinished.

The [predicate milestone](testing/2026-10-26-rust-lua-predicates.md) extends
pending boolean edges to `if`, `while` and `repeat`. Conditions use local
registers and comparison branches directly, and short-circuit expressions avoid
materializing a boolean value. The parser distinguishes an absent `else` from
an explicit empty block. Compiler capture tracking removes unnecessary scope
closes and preserves the two closing paths of a captured repeat scope. Shared
unconditional jump targets resolve in linear time; value-chain test sharing
remains distinct from statement callback boundaries. Source, native export and
persistent-cache regressions cover the changed paths. Native export still has
broader hook differences where extra operand loads or copies remain; complete
hook parity and the other standalone gates remain open.

The [register/constant operand milestone](testing/2026-10-26-rust-lua-rk-windows.md)
removes hidden constant loads from arithmetic, comparisons and table access.
Native imports retain RK operands directly and exports emit native RK fields.
Source metadata carries the parser's logical stack bound so hooks can inspect
temporary slots without exposing physical scratch registers. Hook windows
include their registry table, use the actual return/result top, and keep
temporary writes rooted during callbacks. Reclaimed temporary handles read as
nil. Single nonlocal stores read a local RHS at their own instruction boundary.
The persistent representation advances to schema 2 for logical debug bounds.
Both previously recorded predicate/debug/export probes now match the reference;
broader stack-layout and hook parity remain open.

## Reference contract

The oracle is `vendor/lua-src/lua-5.1.5`, linked only for development through the
legacy binding; its registry-capacity patch remains. Record its source digest
with measurements. Current validation uses x86-64 Windows MSVC, `+crt-static`,
binary64 numbers and the C locale. No cross-platform transcendental-bit guarantee
has been established.

Byte-exact claims must pin source, version/build options, numeric widths, locale,
CRT/libm, OS behavior, random seeds, paths, newlines and output encoding. Compare
bits and bytes directly and identity through predicates rather than addresses.
NaN payloads, unspecified traversal order and hole borders require explicit
comparison policies. Arbitrary Lua errors must eventually retain value identity.

Differential tests cover several observable PUC compiler details: literal folding
avoids division by zero and NaN constants; numeric constant pooling merges signed
zeros; numeric loops subtract before their initial increment; table list fields
flush in groups of 50, affecting duplicate keys mixed with record fields.

## Dialect investigation

These are semantic seams to implement and verify, not available dialect flags.
Official manuals are authoritative:

| Dialect | Required investigation and regression tests |
| --- | --- |
| [5.1](https://www.lua.org/manual/5.1/manual.html) | Function/thread environments, float-only numbers, old vararg `arg`, coercions, weak tables and coroutine boundaries. |
| [5.2](https://www.lua.org/manual/5.2/manual.html#8) | Lexical `_ENV`, goto, escapes/literals, ephemeron keys, length/iteration metamethods and library/yield changes. |
| [5.3](https://www.lua.org/manual/5.3/manual.html#8) | Integer/float subtypes, overflow, integer division, bitwise operations, formatting, UTF-8, packing and numeric keys. |
| [5.4](https://www.lua.org/manual/5.4/manual.html#8) | Const/closing locals, unwinding, integer-loop overflow, coercion/metamethod changes, GC modes, RNG and coroutine close. |
| [5.5](https://www.lua.org/manual/5.5/manual.html#8) | Global declarations, named varargs, read-only loop controls, call-chain limits, nil errors and collector parameters. |

The internal ISA can share infrastructure without reproducing each reference
bytecode format. Official chunk import/export requires separate validation and
must remain distinct from our persistent cache format.

## Architecture and testing research

Studied [Luna architecture](https://github.com/goliajp/luna/blob/cd4ecfe3ee2ec4c58f8cc741bdf0463d1bf5d771/docs/architecture.md)
and [compatibility testing](https://github.com/goliajp/luna/blob/cd4ecfe3ee2ec4c58f8cc741bdf0463d1bf5d771/docs/compatibility.md).
Useful ideas include a dependency-free interpreter core, an optional native-code
backend and separate frontend/binary-reader/runtime fuzzing against official Lua.
This engine currently uses safe indexed storage; no intrusive collector or JIT
interface was adopted.

Studied [OmniLua version analysis](https://github.com/ianm199/omnilua/blob/90dcf85a8cfb95305b77056af00a114fd0df30a1/specs/MULTIVERSION_ARCHITECTURE_DECISION.md)
and [benchmark methodology](https://github.com/ianm199/omnilua/blob/90dcf85a8cfb95305b77056af00a114fd0df30a1/harness/bench/README.md).
Its analysis identifies number models, environments and libraries as substantive
version boundaries. Its multiple-backend proposal is not a requirement here.
Useful practices include pinned per-version oracles, checksums before timing,
interleaved samples, separate compile/run costs and scaling workloads that expose
nonlinear behavior. No implementation code was imported from either project;
neither is a dependency.
