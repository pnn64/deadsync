# Independent Lua engine

The full objective is a standalone Rust Lua 5.1â€“5.5 implementation, followed by
song and noteskin integrations sharing the compiler, VM, heap and semantics.
The first completion target remains arbitrary standalone Lua 5.1 programs with
correct libraries, GC, closures, metatables, coroutines and errors. The current
implementation is an early functional stage of that target.

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
| Independent compiler/interpreter | Resumable scanner/parser, flat AST arenas and grammar continuations, register compiler, verifier, literal folding, VM and CLI exist. Fixed-source, reader-based and file-backed text loading share the frontend without replaying prefixes. File loads preserve stdin lookahead, source names, initial executable headers and thread environments. Complete reference-suite coverage remains open. |
| Exact language semantics | Differential cases cover scalar bits, strings, assignments, loops, closures, varargs, methods, tail calls, multiple results, metatables, protected calls, arbitrary error identity, named locations, environments and coroutine transitions. Local operand references preserve Lua 5.1 reads after right-hand callbacks; indexed assignment captures only conflicting local addresses. Verified metadata supports function/stack/local/upvalue inspection and mutation. Full diagnostics/debug behavior and broader numeric/resource edge cases remain open. |
| Object model and memory | Copyable tags and generation-bearing numeric IDs; flat reusable arenas; vector registers, frames, prototypes and upvalue cells; interned strings; contiguous table arrays and indexed hash entries. Deleted keys retain traversal positions until rebuild. Host roots check VM ownership; raw values must stay in their originating VM. Capacity-based heap estimates and object counts are exposed. |
| Garbage collection | Iterative tracing, cyclic reclamation, Lua 5.1 weak tables, compiler register maps, host roots, pressure checks and collector controls exist. Collected threads close surviving cells; cached pattern/program bindings are reclaimed. Full userdata has environments, metatables, byte storage and Lua 5.1 proxies. Finalizers support creation order, resurrection, weak-reference rules, nested collection, error identity and shutdown. Cycles complete atomically; `step` budgets whole cycles. Incremental pauses and broader resource tests remain open. |
| Coroutines | Shared-heap threads have separate register/frame vectors and environments, explicit resume/yield scheduling, wrappers/status, and cross-thread upvalue access. Lua 5.1 native/metamethod yield restrictions, inherited C-call depth, error identity, quota recovery and reclamation with escaping cells are tested. Debug inspection, local/upvalue mutation, tracebacks and per-thread hooks reach other thread stacks and shared cells. New threads inherit hook controls without inheriting the Lua callback. Hook/traceback callbacks support child resumes, collection and quota recovery. Full hook/diagnostic parity and broader resource tests remain open. |
| Standard libraries | Expanded base library includes radix conversion, iteration, metatables, protected calls, output, environments, `loadstring` and protected reader-based text `load`. String byte operations, patterns, replacement callbacks, formatting, raw table operations/traversal/sorting, coroutine functions and the 5.1 math function set exist. Initial I/O includes file userdata, temporary/named files, defaults, pipes, reads/writes/seeks, buffering, iterators and GC closure. A shared registry stores file tags and builtin `_LOADED` entries. Package loading provides preloaded/Lua-file/custom loaders, import caching and 5.1 module environments. Light userdata supplies unmanaged identities and the observable import sentinel. Remaining base/OS functions, native C-library loading, debug, binary dumps, broader I/O/device/encoding behavior and numeric/diagnostic compatibility remain open. |
| Persistent compiled cache | Unimplemented. Requires bounded decoding, bytecode verification, format/dialect/platform identity and collision-safe source checks. |
| Native host calls | Builtins use numeric IDs and register argument ranges. Public callback registration, host objects, error/yield handling and overhead measurements remain open. |
| Lua 5.2â€“5.5 | Unimplemented. Investigated differences below must extend shared infrastructure with per-version oracle gates. |
| Machine code | Optional; defer until compatibility baselines and profiles identify profitable paths. |
| Song integration | Deferred until engine gates pass. Implement live callbacks, independent baking, auto by default, dynamic fallback, and shared state/identity across strategies. |
| Noteskin integration | Deferred. Create `deadsync-noteskin-lua` with the shared engine, reusable programs, static actor/asset caches and necessary dynamic callbacks. |
| Correctness testing | Differential cases and deterministic generation exist. A checksum-pinned official 5.1 runner records fifteen reference passes among sixteen probes and ten Rust passes (`constructs.lua`, `locals.lua`, `nextvar.lua`, `vararg.lua`, `pm.lua`, `events.lua`, `gc.lua`, `closure.lua`, `db.lua`, `math.lua`). `files.lua` is retained with a reference failure at Windows open-file rename line 59; Rust reaches missing `os.tmpname` line 19. Calls reach binary dumps, literals/strings reach locales, sort reaches `os.clock`, and errors expose incomplete syntax diagnostics. Full suite passage, broader source/bytecode fuzzing and per-dialect CI remain open. |
| Performance | Initial reference harness exists. Expand throughput, startup, compile cost, allocations, live/peak memory, scaling, representative workloads and other Rust engines. Preserve samples and machine/build metadata. |
| Independent stages | The literal-folding pass is separate internally. Public compile/optimization and scene-baking stages, cached stage identities and equivalence gates remain open. |

Next work should finish the standalone engine: incremental collection,
remaining standard libraries, complete diagnostics/debug behavior, binary loading and
caches. Production migration requires the complete compatibility and
performance gates; the current regression corpus does not establish them.

Validation also includes strict Clippy, a dependency tree containing only the new
crate, 196 differential/adversarial integration tests, thirteen parser/scanner/arena/verifier tests and
a runnable documentation example. Generated cases include 1,000 numeric/logical
expressions, 325 metamethod-handler combinations, 5,120 arbitrary byte inputs,
4,000 byte-pattern cases, 2,000 float formats, 2,000 binary-scaling cases and 2,000
radix conversion cases. Sort tests compare comparator
traces across all permutations through length six, callback mutations, nested
sorting, errors and metatables. Coroutine generation exercises 300 resume sequences
and recursive generators, alongside nested resumers, native callbacks, environments,
failed/suspended thread cells, result arity, C-call limits and quota recovery.
GC coverage includes cyclic and weak graphs, collection inside native callbacks,
collected suspended/failed threads with escaping cells, 20,000-link graphs,
150 live locals, automatic concatenation/array pressure, stale host handles and
12,000 dynamically compiled chunks with bounded reusable storage.
Userdata coverage adds proxy validation, byte storage, environments, metamethods,
finalization order across 100 reuse batches, resurrection, weak tables, nested
collection, error handlers, C-call depth, instruction aborts and VM shutdown.
Nested collection after automatic interruption also tests completed-opcode
register maps, 500 open results and host return roots.
The [official suite report](testing/2026-10-25-rust-lua-package.md) identifies the
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
All engine code remains safe Rust with no normal/build dependencies. Commit each
verified milestone while keeping unrelated edits separate.

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
