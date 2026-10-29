# Independent Lua engine

The full objective is a standalone Rust Lua 5.1â€“5.5 implementation, followed by
song and noteskin integrations sharing the compiler, VM, heap and semantics.
The first completion target remains arbitrary standalone Lua 5.1 programs with
correct libraries, GC, closures, metatables, coroutines and errors. The current
implementation is an early functional stage of that target.

Current work follows correctness and Lua 5.1 compatibility first. Performance
tuning stays deferred until the full standalone Lua 5.1 parity gate is met.
Close missing behavior and failures, run the full official suite, and expand
differential coverage against official Lua. Existing performance reports remain
historical baselines; compatibility milestones require correctness checks.
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
| Standard libraries | Expanded base library includes radix conversion, iteration, metatables, protected calls, output, environments, `loadstring` and protected reader-based text `load`. String byte operations, patterns, replacement callbacks, formatting, raw table operations/traversal/sorting, coroutine functions and the 5.1 math function set exist. Initial I/O includes file userdata, temporary/named files, defaults, pipes, reads/writes/seeks, buffering, iterators and GC closure. A shared registry stores file tags and builtin `_LOADED` entries. Package loading provides preloaded/Lua-file/custom loaders, import caching and 5.1 module environments. The default Windows/Linux C ABI adapter enables `package.loadlib` and C/C-root searchers; custom host resolvers remain supported. Light userdata supplies unmanaged identities and the observable import sentinel. OS support includes calendar conversion/formatting, clocks, time differences, environment queries, file removal/rename, temporary names, shell execution and process exit. Windows/MSVC and Linux/glibc native locales connect numeric conversion/formatting, ctype, patterns, collation and calendar formatting. Other libc ABIs, remaining base functions, full native C API semantics, broader debug and I/O/device/encoding behavior and numeric/diagnostic compatibility remain open. |
| Persistent compiled cache | Opt-in ProgramCache persists the shared bytecode, constants, collector/debug metadata and native re-export words. Entries bind the format/dialect/target, Cargo compiler build and lexer character classes; full source/name equality guards filename collisions. Bounded decoding, integrity checks, Program verification and native tree/range checks reject incompatible entries. Temporary-file replacement and compile fallback preserve availability. Fresh-process source/native reloads and concurrent writers are tested. Directory eviction, a memory cache and package/CLI integration remain open; tiny/native inputs may reload more slowly than recompilation. |
| Native host calls | Builtins and registered Rust function pointers use numeric native IDs and register argument ranges. HostCall borrows arguments, writes separate results and accesses up to 255 traced/mutable Lua captures. Private owned binding data stays separate from Lua upvalues. Protected nested Lua/native calls preserve outer frames, share the instruction quota and return arbitrary error values to Rust. Unprotected stack calls retain failing child frames for outer error handlers. Hooks, coroutine callers, resumed children, finalizers, preload modules and per-VM cached-program bindings are tested. Callback and nested-call measurements exist. Suspending Rust callback stacks, richer Rust host objects and broader error/overhead work remain open. |
| Lua 5.2â€“5.5 | Unimplemented. Investigated differences below must extend shared infrastructure with per-version oracle gates. |
| Machine code | Optional; defer until full standalone Lua 5.1 parity, then use profiles to identify profitable paths. |
| Song integration | Deferred until engine gates pass. Implement live callbacks, independent baking, auto by default, dynamic fallback, and shared state/identity across strategies. |
| Noteskin integration | Deferred. Create `deadsync-noteskin-lua` with the shared engine, reusable programs, static actor/asset caches and necessary dynamic callbacks. |
| Correctness testing | Differential cases and deterministic generation exist. The unchanged stock `main.lua` and `all.lua` pass in both standalone CLIs on Linux i686 SSE2 with the stock native libraries enabled. T/testC remains absent. All eighteen independent files pass on Linux x64 in reference, Rust and both cache modes; Windows retains seventeen passes and the common `files.lua:59` rename failure. Native x64 full-suite failures, remaining standalone mismatches, broader source/bytecode fuzzing and per-dialect CI remain open. |
| Performance | Deferred until full standalone Lua 5.1 parity. Preserve existing reference harnesses and historical measurements; later work covers throughput, startup, compile cost, allocations, live/peak memory and representative scaling. |
| Independent stages | The literal-folding pass is separate internally. Public compile/optimization and scene-baking stages, cached stage identities and equivalence gates remain open. |

Unchanged `all.lua` and `main.lua` run against both standalone CLIs, with retained
failures directing compatibility fixes. Linux `main.lua` passes in both engines.
Linux x64 `all.lua` reaches the same `big.lua:11` overflow assertion in both
engines. [Linux i686 SSE2](testing/2026-10-27-rust-lua-i686-suite.md) passes
both complete stock entry points, including that native-width overflow check.
The assertions remain unchanged. Windows stops in `main.lua:53` in both
engines. The [GNU numeric and I/O fixes](testing/2026-10-27-rust-lua-gnu-numbers.md)
resolve the Linux I/O and native-error targets. The expanded stream audit
closes the retained Windows byte-consumption mismatches and matches 15,883
stream inputs on both platforms. [Portable regression checks](testing/2026-10-27-rust-lua-portable-tests.md)
passed 629 Rust tests on each platform after fixing the remaining fixture
assumptions. Crashing GNU C `unpack` inputs run in a separate strict audit;
five native crashes remain recorded, with signed-overflow diagnostics and an
independent wrapping control. The default reference flags remain unchanged.
The current runs pass 812 Rust tests on Windows x64 and 813 on Linux x64 and
Linux i686 SSE2. The default x87 arithmetic baseline and five bounded traceback
requests retain failed/incomplete observations. Complete C/auxiliary APIs and
remaining language, library, debug, GC and resource behavior must also be
completed. Selected passing probes and declaration counts do not close these
gates. Production migration also requires the subsequent performance gates.

Validation also includes strict Clippy, a dependency tree containing only the new
crate, 717 differential/adversarial integration tests, eighty-four unit tests and
four runnable documentation examples. Generated cases include 1,000 numeric/logical
expressions, 325 metamethod-handler combinations, 5,120 arbitrary byte inputs,
4,000 byte-pattern cases, 2,000 binary replacements, 2,000 float formats, 2,000 binary-scaling cases and 2,000
radix conversion cases. Sort tests compare comparator
traces across all permutations through length six, callback mutations, nested
sorting, errors and metatables. Coroutine generation exercises 300 resume sequences
and recursive generators, alongside nested resumers, native callbacks, environments,
failed/suspended thread cells, result arity, C-call limits and quota recovery.
A separate [library argument audit](testing/2026-10-28-rust-lua-table-yields-stdlib.json)
matches all 5,507 bounded cases against the standalone Windows/MSVC reference.
The default native adapter closes the fourteen previously retained native-loading
argument mismatches; full standard-library parity remains open.
A [native-call boundary proof](testing/2026-10-26-rust-lua-native-boundary.md)
now compares an unchanged C module against official Lua and Rust under the
release `panic=abort` policy. A small development C shim returns Rust dispatch
errors before jumping within C, preserving Rust stack cleanup and Lua error
identity. The default Windows/Linux adapter now uses this shim and package
ownership integration. Full C API stack/pseudo-index semantics,
inactive/cross-thread state access and complete C API coverage remain open.
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
diagnostic bytes. This does not supply the remaining C API.
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

The [native userdata checks](testing/2026-10-26-rust-lua-native-userdata.md)
add stable, pointer/double-aligned full userdata payloads to the native stack,
function/userdata/thread environment access, raw object lengths and numeric
string conversion in the original slot or capture. Interned strings include
the C API's trailing NUL without changing their logical bytes. The identical C
fixture checks direct payload writes, pointer stability through collection,
finalization and resurrection, environment identity and binary strings against
official Lua. Eight new Rust groups cover payload sizes, VM moves, collection,
allocation rejection and pending-error guards. This is a compatibility
milestone; native loading, the rest of the C API, broader native GC/debug
behavior and full standalone Lua 5.1 parity remain open. No performance tuning
or benchmarks accompany this change.

The [protected native checks](testing/2026-10-27-rust-lua-native-protected.md)
add `NativeStack::protected_call` and distinct success/runtime/handler-failure
statuses. A hidden continuation roots the error function, invokes the existing
handler path before closing faulting cells, and returns status separately from
Lua error bytes. The development bridge compares `lua_pcall` and `lua_cpcall`
against official Lua, including negative error-function indexes, result counts,
reader-error inheritance, protected-boundary isolation, coroutine yield errors,
binary C errors and recursion reserve exhaustion. Error functions retain mutable
stack-slot references through Lua/native argument-window moves and vararg entry.
Nine Rust groups cover protected execution, slot aliases and fatal instruction
limits. That revision's development shim declared 37 of the pinned Lua header's
80 core C functions, plus one auxiliary function; declarations do not establish full
semantic coverage. Production native loading and full standalone Lua 5.1
compatibility remain open; performance work stays deferred.

The [native value checks](testing/2026-10-27-rust-lua-native-values.md) add
nonmutating scalar conversions, pointer-width integer pushes, shared-handler
equality and less-than, and concatenation of the original C operand window.
Numeric coercions and partial reductions remain visible while its top and
operand roots persist until the operation completes. Private comparison and
metamethod argument copies stop retaining cleared operands after Lua entry.
The same C fixture verifies conversion bytes, stack effects, collector roots,
diagnostics and matching recursion limits. Nine new Rust groups accompany
thirteen additional C API declarations; the shim now declares 50 of 80 core
functions plus one auxiliary function. Validation lists the remaining 30 core
names. Declarations are not proof of complete semantics or production C API
availability. Full standalone Lua 5.1 and native loading remain open gates;
performance tuning remains deferred.

The [native auxiliary checks](testing/2026-10-27-rust-lua-native-auxiliary.md)
add C formatted messages, source prefixes, argument/type/default/option checks,
tagged userdata, raw metafields and metamethod calls, module registration,
dotted table creation and reference handles. Formatting uses primitive VM
concatenation without an extra collector check, matching Lua 5.1's observable
finalizer timing. C error handlers exclude the error slot from the failing
native frame's locals. Shared-source C cases cover bytes, stack effects,
method/self names, environments, captured roots and reference reuse. Four new
Rust groups accompany 24 additional declarations. The development header now
declares 52 of 80 core and 23 of 36 auxiliary functions; validation lists missing
names. Complete semantics, production native loading, the full official suite
and standalone Lua 5.1 parity remain open. Performance work stays deferred.

The [auxiliary buffer checks](testing/2026-10-27-rust-lua-auxbuffer.md)
add the Lua 5.1 `luaL_Buffer` layout, buffer operations and literal `luaL_gsub`
replacement to the development C boundary. The unchanged shared C fixture
compares 99 buffer scenarios, chunk-stack traces and 40 collection callbacks
with official Lua, including binary strings, numeric coercion, reentry, error
identity and forbidden yields. The core Rust runtime remains unchanged.
Production native loading, the remaining C API and full standalone Lua 5.1
parity remain open; performance work stays deferred.

The [imported upvalue checks](testing/2026-10-27-rust-lua-upvalue-names.md)
correct debug access to stripped and partially named native captures. Only
retained names permit inspection/mutation; actual cells, execution and reported
capture counts remain intact. Missing names use Lua 5.1's `?` diagnostic
fallback. Eighteen native chunks match official Lua on Windows x64, Linux x64
and Linux i686 SSE2 in direct, cache-store and fresh-process cache-hit modes,
including open/closed/suspended cells and redumped functions. These regressions
do not establish complete C API coverage or full standalone Lua 5.1 parity.

The [native upvalue checks](testing/2026-10-27-rust-lua-native-upvalues.md)
add `lua_getupvalue` and `lua_setupvalue` to the development boundary. Lua
capture cells and native captures share the existing VM stack and collector
barriers. Builtin iterators, wrappers, `pairs`, `ipairs` and `newproxy` now use
ordinary traced Lua capture values; native writes change execution immediately.
Program-owned interned names keep returned C pointers stable while their
functions remain reachable. Shared-source C cases cover all builtin capture
families and nine native-chunk name variants. Six Rust groups and two forced
collector groups cover mutation, lifetime, collection, stack bounds and error
recovery. The unchanged stock i686 SSE2 suite still passes in both engines.
Full C API coverage, production native loading and standalone Lua 5.1 parity
remain open. Performance work stays deferred.

The [native collector checks](testing/2026-10-27-rust-lua-native-gc.md) add
`NativeStack::gc` and `lua_gc` in the development boundary. Native operations
share the standard library's collector controls. Full collections and completed
steps run finalizers through a hidden host continuation, preserving the actual
C caller, argument slots, error identity, fault frames and instruction quota.
The shared C fixture checks controls, count-part consistency, reentry, caller
mutation, stopped collection after errors and yield restrictions. Six new Rust
groups cover stack preservation, discarded roots, sticky errors and recovery.
Storage counts describe this engine's estimated live bytes; PUC work units and
complete incremental/resource behavior remain open. Full C API coverage,
production native loading and standalone Lua 5.1 parity remain unfinished.

The [native loader checks](testing/2026-10-27-rust-lua-native-load.md) add
`NativeStack::load_reader`/`load_buffer`, loader statuses and development C
`lua_load`, `luaL_loadbuffer` and `luaL_loadstring` entry points. A protected
continuation shares the existing incremental source/binary loader, preserves
caller slots and permits reader reentry without a visible Lua loader frame.
Reader errors inherit the enclosing error function; syntax errors bypass it.
Initial empty probes, binary upvalue initialization, coroutine environments,
reader consumption and parser/native depth are covered. The shared C fixture
uses identical official binary chunks for both engines. A separate executed
probe retains an observable mismatch: C readers see PUC's private parser roots
through `lua_gettop`, whereas this engine exposes the caller stack. Consequently
native loader parity is explicitly unproven. File-loading auxiliary APIs,
allocator/OOM statuses, full C API coverage and standalone Lua 5.1 parity remain
open. Performance work stays deferred.

The [native file checks](testing/2026-10-27-rust-lua-native-file.md) add
`NativeStack::load_file` and development `luaL_loadfile`. Native and Lua loaders
share stream ownership and executable-header/binary-reopen handling. The native
API reads the actual shared stdin stream, preserves buffered lookahead, returns
Lua 5.1 file status 6 and closes named streams on parser, collector and quota
failures. Active parser jobs trace borrowed file handles while their native scope
owns cleanup. Filename pushes and file-error formatting check collection outside
parser protection. Windows empty paths now return MSVC's EINVAL diagnostic.
The shared C fixture covers text/binary files, long headers, source names,
missing/empty paths, shared stdin, coroutine environments and mutable global
loader isolation. Rust groups cover nested finalizer loads and collection,
pre-open collector errors, file-error precedence and cancellation recovery.
Parser scratch slots, allocator/OOM statuses, remaining C APIs, production
native loading and full standalone Lua 5.1 parity remain open; optimization stays
deferred.

The [native dump checks](testing/2026-10-27-rust-lua-native-dump.md) add
`NativeStack::dump` and development `lua_dump` writer callbacks. The shared
serializer optionally records Lua 5.1 field/vector/string block boundaries,
including empty debug vectors. Native writers use the existing caller window,
can reenter Lua or collect, stop on their first nonzero signed status, and
propagate Lua errors unprotected with fault frames and instruction quotas.
Imported chunks retain byte-exact writer payloads; source exports use this
engine's native lowering. The C fixture checks eleven imported chunks, every
cancellation point of a constants fixture, signed statuses, NULL empty-vector
buffers, recursion, collection and caller mutation. Rust tests compare writer
blocks directly with official Lua and verify quota recovery. Native loader
scratch slots, allocator/OOM behavior, remaining C APIs, complete standalone
Lua 5.1 parity and later stages remain open. Optimization stays deferred.

The [native thread checks](testing/2026-10-27-rust-lua-native-thread.md) add
`lua_tothread`, `lua_pushthread` and readonly `lua_status` queries. The safe stack
API converts/pushes thread values, reports retained execution statuses and can
attach typed host resources to any live thread without activating it. Fresh,
running and successfully completed threads report zero; yielded threads report
one, and failed children report two. The C adapter lazily creates stable states
for never-resumed coroutines and removes its pointer lookup when a thread's
resource drops. This lookup does not retain Lua threads. The shared C fixture
checks identity before/after resume, parent/main flags, pseudo-index captures,
status transitions and weak lifetimes. Rust groups cover inactive resource roots,
self cycles, VM moves, collected IDs and quota cancellation. Inactive C stack
mutation, C coroutine creation/resume/yield, allocator/OOM statuses and the other
full Lua 5.1 gates remain unfinished. Optimization remains deferred.

The [native pointer checks](testing/2026-10-27-rust-lua-native-pointer.md) add
`lua_topointer` and shared stable object identities. Tables, Lua/native functions
and threads receive lazily owned pointer-aligned tokens; userdata returns its
actual aligned payload and light userdata its unmanaged address. Tokens do not
retain Lua objects and are pruned after collection. Default object strings use
the same native pointer formatting and address as the pointer APIs. C thread
handles now use the thread token; C-only error frames live in a TLS chain rather
than dereferencing an assumed state layout. The C fixture verifies identity,
thread/payload agreement, default-string relationships, weak lifetimes and a
thousand live objects. Rust tests cover VM moves, cross-VM uniqueness, stale
handles, resurrection, native formatting and token reclamation. Complete C API,
allocator/OOM, parser scratch and standalone Lua 5.1 gates remain open;
optimization stays deferred.

The [native parser-root checks](testing/2026-10-27-rust-lua-native-roots.md)
close the recorded `lua_gettop` loader mismatch: both engines now expose 1,3
slots in the original probe. Native text readers see constants-table/prototype
pairs for active functions; binary readers see unfinished prototypes after the
header. Incremental grammar/decoder events populate real collector objects and
constant tables, including reserved words, implicit locals and constant indices.
Field constants are installed after the following token is read, and function
roots remain until the closing token's lookahead completes. Compiled closures
retain their own prototype subtrees; parent/sibling prototypes and temporary
constant tables can be collected independently. Table iteration also preserves
the first inserted zero key's sign while both zero signs compare equal. The C
fixture compares 1,745 loading scenarios and 7,772 reader observations against
official Lua, including weak lifetimes. Portable Rust groups cover nested loads,
binary fragmentation/collection, function ordering and stack-error recovery.
Arbitrary mutation of private parser state, remaining C APIs, allocator/OOM
behavior and full standalone Lua 5.1 parity remain unfinished. Optimization and
benchmarks stay deferred.

The [native thread-stack checks](testing/2026-10-27-rust-lua-native-transfer.md)
add `lua_newthread` and `lua_xmove`. Empty threads share the creator's heap,
registry and thread globals, inherit hook controls, and hold their eventual entry
on the actual stack. A fresh empty thread reports dead to `coroutine.status` but
native status zero; adding an entry makes it suspended. Successfully completed
threads can accept another entry, while failed threads retain their error status.
`NativeThreadStack` borrows an inactive stack without executing Lua. Primitive
indexing, coercion, stack edits, pointer/thread conversion and cross-thread moves
preserve values and collector edges. The C adapter routes these operations by
opaque thread identity while retaining its active C-only error boundary. Waiting
resumers expose their remaining argument window; return hooks see arguments and
transferred results, with writes aliased to the actual return values. Supported
failed-thread windows retain their diagnostics. The Windows fixture checks a
thousand transfer cycles, weak lifetimes, stable string pointers, globals,
inherited controls, native/Lua entries, limits and five diagnostic layouts.
Inactive operations that execute Lua, remaining C coroutine execution APIs,
broader diagnostic scratch behavior, lifecycle/allocator APIs and full standalone
Lua 5.1 parity remain unfinished. Optimization stays deferred.

The [standalone suite checks](testing/2026-10-27-rust-lua-full-suite.md) invoke
the real CLIs with the official README's `LUA_PATH="?;./?.lua"` setup. Each run
gets unchanged suite files and a private `lua` executable on PATH, so child
commands use the same engine. No `_U` or other Lua bootstrap is injected.
POSIX temporary names now use `/tmp` and an extensionless basename, allowing
the suite's `require(os.tmpname())` usage. Linux `main.lua` changes from a Rust
failure to a pass; the reference passes throughout. Both `all.lua` failures and
the common Windows entry-point failure remain visible. The internal T module
and suite native libraries are not provided by this stock-mode runner, so
their automatically skipped tests are not established by these results.

The [POSIX locale checks](testing/2026-10-27-rust-lua-posix-locale.md) add
Linux/glibc category selection, queries, aliases, composite restoration and
environment defaults. Owned immutable native locale objects support ctype,
collation, calendar text and raw errno messages. Engine VMs share the selection;
the host's process-global C locale remains unchanged. External native bindings
and other libc ABIs remain open. Character classes/case maps publish as one
locked snapshot, fixing a threaded reader's exposure to cleared mappings.
Thirty-six standalone locale programs match the official C reference, and both
Linux full-suite runs now stop at `big.lua:11`. Three new Linux Rust groups pass.
The broad Linux audit remains red in the same six targets as its parent; it is
retained alongside the passing Windows corpus rather than counted as parity.

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
open-result sentinel. At that milestone a compile-time assertion kept frames at 64 bytes on 64-bit
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

The [native coroutine checks](testing/2026-10-27-rust-lua-native-resume.md)
add `NativeStack::resume_thread` and `yield_results`, plus development C
`lua_resume` and `lua_yield`. The shared scheduler preserves the C entry point's
results on the resumed thread and Lua's transfer-to-caller behavior. Yielding
returns normally through the Rust callback; resumption provides its return
values without calling it again. Suspended debug locals follow the public
stack, and return hooks can inspect and modify retained arguments and actual
return values. The C fixture matches five fault windows, invalid/callable entries,
nested/mixed resumes, parent mutations, collection, hook aliases and 1,000 cycles.
Portable groups cover Rust destruction at yield, argument validation and quota
recovery. The boundary declares 65 of 80 core APIs; full semantics, `lua_setlevel`,
remaining native hook/debug/lifecycle/allocator behavior and full standalone
Lua 5.1 parity remain unfinished. Optimization and benchmarks stay deferred.

The [C-call count checks](testing/2026-10-27-rust-lua-native-level.md) add
`NativeStack::copy_thread_call_level` and `thread_call_level`, plus development
`lua_setlevel`. Current/waiting thread counters, parser bases, protected error
checkpoints, unsigned wrapping, the resume limit and raw error-handler status 5
are checked directly against source-built Lua 5.1.5's counter. Native-triggered
GC no longer counts its finalizer call twice. A separate strict audit still
fails when lowering the active counter permits a reference yield across a
protected call; Rust retains its structural boundary. That mismatch remains
open, along with complete coroutine/C API and standalone Lua 5.1 parity. The
boundary declares 66 of 80 core APIs, which is not proof of full semantics.
Optimization and benchmarks stay deferred.

The [reader call-limit checks](testing/2026-10-27-rust-lua-native-reader.md)
fix stock `load(reader)` at wrapped/high C-call counts and generic-for entry
after a counter change. A checked build of unchanged official sources diagnoses
the retained lowered-count yield case at the resume counter invariant; its
ordinary control and the reader/iterator cases pass. The release mismatch still
exists and its strict audit still fails. This additional evidence does not close
complete coroutine/C API or standalone Lua 5.1 parity. Optimization stays deferred.

The [native stack-inspection checks](testing/2026-10-27-rust-lua-native-debug.md)
add typed stack records, function information and local reads/writes, with four
corresponding development C adapters. Records preserve visible call ordinals;
inspection works on active, suspended, failed and waiting parent threads. Native
stack effects, invalid options, tail records, field preservation, source-buffer
padding and interned-name lifetimes are checked against official Lua 5.1.5.
The boundary now declares 70 of 80 core APIs. This is declaration coverage,
not complete C API or standalone parity. Native hooks, state/allocator APIs,
the retained strict coroutine mismatch and broader compatibility gates remain
open. Performance tuning remains deferred until full standalone Lua 5.1 parity.

The [external native hook checks](testing/2026-10-27-rust-lua-native-hooks.md)
add thread-owned host hook bindings, control queries and event activation records,
with development adapters for `lua_sethook`, `lua_gethook`, `lua_gethookmask` and
`lua_gethookcount`. Native hooks share the interrupted stack without an extra
visible call frame. Temporary aliases, collector roots, callback replacement,
Lua hook pointer restoration, inheritance, zero-result instruction-hook yields,
resumed tops and C-call counters are compared against the same C module built
for official Lua. Math, unpack and iterator return windows now retain their
native arguments and pending results. Portable regressions cover lifetime,
reentry, errors and instruction-abort recovery. The development boundary declares
74 of 80 core APIs; declaration counts and these selected checks do not close
the full C API or standalone parity gates. State/allocator APIs, native loading,
broader scratch-stack/event behavior and the retained strict coroutine mismatch
remain open. Performance work stays deferred.

The [native calling-context checks](testing/2026-10-27-rust-lua-native-context.md)
fix external hook pseudo-indexes to read and mutate the interrupted function's
environment and C upvalues. Rust hook registration captures stay separate.
Native allocations inherit the real visible calling environment rather than an
internal continuation's environment. Waiting/hook-suspended thread APIs use that
same call identity, and a fresh thread rejects calling-environment replacement.
The identical C fixture checks native and Lua functions, separate thread globals,
child-coroutine reads/writes, userdata/closure environments and collector safety.
Five portable Rust groups cover the corresponding host API contexts. These are
semantic fixes to existing APIs; the remaining six core C declarations,
production loading and the full standalone Lua 5.1 gate remain open.

The [base/string return-stack checks](testing/2026-10-27-rust-lua-return-stacks.md)
replace exposed scheduler scratch with each library routine's logical C stack.
Retained arguments, consumed inputs, protected metatables, environment temporaries
and pending result suffixes now follow the reference. C and Lua hooks can mutate
the returned slots, while discarded arguments and scratch release their roots.
Lua reader-based `load` now shares incremental constants-table/prototype roots
with native readers and retains EOF nils and protected error layouts. The same C
module matches 252 observations with collection and first/last-slot writes; three
portable differential groups cover Lua hooks, root release and reader internals.
This leaves the full Lua 5.1 goal and remaining native API/loading/resource gaps
open.

The [return-hook protection checks](testing/2026-10-27-rust-lua-protection-hooks.md)
fix errors being caught by an already completed `pcall`, `xpcall` or loader.
Pending return continuations keep visible debug frames while ending the finished
operation's protection scope. Parent scopes still catch child hook errors, and
nested loaders inherit the active outer handler. All 39 C-module scenarios match
the reference, including direct errors, nested calls, handler counts and object
identity under collection. Three portable groups cover Lua and native hooks.
The full Lua 5.1 goal and remaining native API/loading/resource gaps remain open.

The [table/debug/I/O/OS stack checks](testing/2026-10-27-rust-lua-library-stacks.md)
extend native return windows to library temporaries, consumed inputs, implicit
files and iterator EOF/close states. Failed opens retain a genuine closed userdata
allocated before the OS operation; popped hook slots release it. Debug upvalue
writes consume the actual topmost argument, and nil/closed default line and close
arguments follow reference checks. The C corpus covers 291 observations with
first/last-slot writes and collection; four portable groups exercise Lua hooks,
behavioral fixes and object lifetime. Full Lua 5.1 parity and the remaining native
API/loading/resource gaps remain open.

The [native function-body checks](testing/2026-10-27-rust-lua-native-bodies.md)
add development-boundary `lua_tocfunction` pointers for foreign C functions and
builtins, including iterator bodies and shared implementation aliases. A body
executes in the current activation with that closure's captures and environment;
copying it does not retain the original closure. Direct invocation adds no visible
frame or hook event and returns through the native callback before suspension.
The C comparison includes 543 copied-body stack observations and focused pointer,
coroutine, hook-context and collector checks. Eight portable groups exercise the
same execution paths. The boundary now declares 75 of 80 core APIs. State and
allocator APIs, complete inactive-state execution, error-time/native resource
semantics and production native loading still keep the full Lua 5.1 gate open.

The [package C-stack checks](testing/2026-10-27-rust-lua-package-stacks.md)
replace exposed scheduling slots in `require`, `module`, `seeall` and the
preload/Lua/C-path searchers with their retained C windows. `require` now carries
that window through cache metamethods and loader calls, including mutations and
pending sentinel writes. `seeall` retains its original metatable and pending
globals during `__newindex`. Module initialization pops its `_NAME` lookup, and
moving arguments releases their old roots while relocating native error-function
indexes. Captured search names and paths stay separate from mutable C stack
slots. The identical C fixture covers ordinary and copied builtin bodies, live
callbacks and collector effects; seven portable groups cover these behaviors.
Production C-module loading, the remaining lifecycle/allocator APIs and the full
Lua 5.1 parity gate remain open.

The [native-library integration checks](testing/2026-10-27-rust-lua-library-loading.md)
connect package loading to an explicit host ABI resolver. `package.loadlib`, C
searchers and C-root fallback now share registry-cached library userdata, symbol
binding, retries, environment inheritance and close/reopen/GC ownership. Registry
metamethods execute in the actual loader/finalizer activation, and return hooks
share the retained handle and pending result slots. The Windows development
adapter loads a real DLL through the package routines and invokes its initializer
and exported functions, including copied builtin bodies. Six portable groups
exercise caching, callback reentry, child coroutines, errors, instruction-abort
recovery, environment inheritance and cleanup. The core remains independent of a
C Lua runtime and has no normal/build dependencies. This milestone supplied an
explicit adapter; the following change makes it the default on Windows/Linux.

The [default C ABI checks](testing/2026-10-28-rust-lua-default-native.md) move the
shared native dispatcher and C-only jump shim into the runtime. The default
`native-c-api` feature builds and embeds `lua51.dll` or `liblua5.1.so.0`, with lazy
loading from a byte-verified cache. Actual CLI tests compile a module against the
ordinary public Lua headers, and VM tests cover child-first calls, replacing the
provider, parallel VMs, userdata finalizers and native ownership. All twenty
Windows boundary runs have balanced dispatch/state/library counters under
`panic=abort`. The complete 5,507-case argument audit now matches. The unchanged
i686 SSE2 stock suite also passes with all four stock native libraries built,
with no native-library skip. Five lifecycle/allocator declarations, complete
inactive-state execution, broader native resource/error/OOM semantics and the
retained strict coroutine mismatch still keep full Lua 5.1 parity open.

The [inactive-object checks](testing/2026-10-28-rust-lua-inactive-objects.md)
extend other-thread stacks to raw tables, traversal, metatables, userdata,
closures, environments and indexed Lua/native captures. Allocations inherit
the target thread's calling environment, including a waiting C activation's
environment, without executing or resuming that thread. Open cells and native
capture writes share the ordinary collector barriers. The same C fixture matches
17 observations on fresh, completed, yielded, failed and waiting threads, with
collection while new objects are rooted only on the target stack. The preceding
CLI fails its first table operation. Safe Rust checks also preserve stack inputs
on rejected capacities, captures, keys and environments. Full inactive-state
execution, lifecycle/allocator APIs and the other standalone parity gates remain
open.

The [other-thread protected-call checks](testing/2026-10-28-rust-lua-inactive-calls.md)
add `NativeStack::protected_call_thread` and runtime dispatch for `lua_pcall`
and ordinary `lua_cpcall` bodies on a selected stack. The target keeps its
retained status and pending computation. Result placement, handler-local writes,
cell closure, reentry through a waiting caller, collection, hooks, C counters
and later resumption match 72 observations. Three portable Rust groups check
validation, quota cleanup, sticky errors, VM reuse and a completed-target crash
regression. That milestone retained three protected-yield mismatches on
completed, yielded and failed targets.

The [API-yield checks](testing/2026-10-28-rust-lua-api-yields.md) close those
three cases. A selected-thread C API invocation can return success at a yield
without a coroutine resumer, leaving the target suspended and its pending frames
rooted. A continuation finishes the pending call on later resumption without
reinstalling its expired error handler. Fifty-one public-header observations
match the reference, including Lua and C yields, `lua_cpcall`, fixed/open results,
fresh-target rejection, completed-target resumption, collection and shared open
cells. The earlier 72 call observations still match. These cases do not prove
arbitrary suspended C chains or complete inactive execution. Unprotected
error/panic routing and the full Lua 5.1 gate remain open.

The [other-thread loader checks](testing/2026-10-28-rust-lua-thread-loads.md)
add `NativeStack::load_reader_thread` and C adapter support for `lua_load` and
its buffer/string helpers on selected stacks. Reader callbacks use that thread's
stack and retained status; compiled functions inherit its globals. A temporary
window adds no visible Lua activation. Forty-seven observations cover fragmented
text and binary chunks, syntax and arbitrary reader errors, names, environments,
partial-yield stack bases, reentry through a waiting parent, nested readers and
permitted reader yields. The C-call counter and pending continuations follow
the selected invocation, preserving a reader yield even when a later reader
call fails. Two portable groups check capacity validation and fatal quota
cleanup. Arbitrary reader continuations, direct inactive
metamethod/unprotected calls and the remaining full parity gates stay open.

The [reader-continuation checks](testing/2026-10-28-rust-lua-reader-continuations.md)
extend this path past a permitted reader yield. Successful parsing retains the
actual physical top and pending Lua/C frames, with expired parser/window
continuations that add no visible activation. Parser-root pops use physical
positions while capacity checks still use the current window. Subsequent reader
calls restore the latest CallInfo base and environment. Ninety-six stack/status
snapshots, eighteen resumption observations and eight environment observations
match the release reference, including empty visible windows and exposed parser
anchors. A defensive guard rejects resumption when parser failure removed every
suspended frame. The separate 24-case resume audit remains a strict failure on
five reference crashes; an assertion-enabled reference also rejects two other
resume shapes. These outcomes remain recorded and do not establish arbitrary
reader continuation parity or complete Lua 5.1 behavior.

The [native panic checks](testing/2026-10-28-rust-lua-native-panic.md) add
`lua_atpanic` and unprotected `lua_error` on idle other threads. Per-VM C metadata
shares callback replacement across thread tokens without changing pointer
identity, and releases the group with its last member. Before invoking a panic
callback, the VM resets the target stack, closes upvalues and clears pending
frames; the original error and installed hook survive. A missing callback keeps
the stack intact until process exit. C invokes callbacks only after Rust
dispatch returns. Fifty-four public-header observations match the release and
assertion-enabled references, and three subprocess cases match exit status 1
and exact output bytes. Parallel VM isolation and state/group cleanup are
checked. Core declaration coverage is now 76/80; the four lifecycle/allocator
entry points, protected routing into waiting targets and broader native
error/resource/OOM semantics remain open.

The [selected-thread file checks](testing/2026-10-28-rust-lua-thread-files.md)
extend `luaL_loadfile` and the Rust `load_file_thread` API to other threads.
Reader and file loading share context restoration, while each loader retains
its actual final base. File-open errors keep a suspended C result suffix;
parser errors restore the CallInfo base. Fifty-three named-file observations
check stack values, environments, statuses, diagnostics and source names; five
stdin cases share buffered lookahead and leave the stream open. Another 53
finalizer cases exercise unprotected filename-push errors, original error
identity, closing captured locals and callback replacement. C rereads the panic
callback after the failing operation and completes a second Rust dispatch
before invoking it. Two additional subprocess cases verify callback clearing
and returning callbacks during finalization. Protected error routing into
waiting targets and the other full parity gates remain unfinished.
An additional public-module CLI audit exposes a retained automatic-GC timing
mismatch: the reference postpones finalization beyond the filename push while
Rust runs it there. Explicit collection before creating the finalizer makes
both terminating controls match, but the unsettled audit remains a strict
failure and does not establish complete GC/panic parity.

The [native allocation checks](testing/2026-10-28-rust-lua-native-alloc-gc.md)
place GC checks at the C API's required allocation boundaries. Internal formatted
parts avoid adding checks of their own, and numeric string conversion reacquires
its indexed result after collection. Synchronous host get/set/compare results
return directly without entering an unrelated driver safe point. When a tag
method actually runs on a selected thread, its window restores the CallInfo
base; a collector step without a callback keeps the yielded suffix. The corpus
matches 126 observations across six contexts, including error identity, caller
frames and C-local recovery. Supplementary snapshots inspect arguments before
allocation and mutate a concatenation operand inside a finalizer. The original
unsettled file-panic mismatch remains a strict failure with current binaries.

The [selected-thread execution checks](testing/2026-10-28-rust-lua-thread-execution.md)
add `NativeStack::new_thread_on` and `concatenate_thread`, direct `lua_newthread`
and `lua_concat` on other threads, and formatted-message construction there.
The shared execution window also serves loading and explicit collector checks.
A concatenation tag call restores the actual CallInfo base while later passes
retain relative operand offsets, matching unusual suspended-C results and
operand identity. Idle-target concatenation failures invoke the target's panic
callback after Rust dispatch returns and close surviving upvalues. Comparisons
match 54 normal observations, ten panic observations and the expanded 156-case
allocation corpus. Four yielding metamethod cases still differ: both release
and assertion-enabled Lua 5.1.5 return a negative visible stack size, while Rust
rejects the yield. These remain strict failures. Waiting-target errors, complete
continuations, general GC timing and the other full parity gates remain open.

The [concatenation-yield checks](testing/2026-10-28-rust-lua-concat-yields.md)
close the four initial concatenation-yield mismatches. A selected concatenation
keeps its pending Lua frames while the C API consumes yielded values and reduces
its physical top. Native stacks expose that signed offset through `top_offset`;
primitive pushes advance the physical top even below the base. Resumption removes
the escaped API invocation's stale yield boundary while retaining boundaries
introduced above it. Forty release/checked observations cover stack effects,
completed-target resumptions, repeated yields and upvalue mutation across GC.
Broader resume probes retain checked-reference invariant failures and release
crashes, with strict unequal results recorded separately. This does not establish
complete continuation or Lua 5.1 parity.

The [selected-thread collector checks](testing/2026-10-28-rust-lua-thread-gc.md)
add explicit `lua_gc` operations and `NativeStack::gc_thread` on other threads.
Heap controls are shared; finalizers run in the selected thread's environment
and restore a suspended C target's actual CallInfo base. Idle-target failures
reach its latest panic callback, preserve arbitrary error identity and close
surviving upvalues. Comparisons cover 52 controls/finalizer observations and
100 panic observations. Ten memory-count observations check integer/range
properties rather than equal byte charges. Reaching an older pending native
yield without fresh resume arguments now raises a Lua error; invalid signed
windows also avoid unsigned subtraction. The broader resume audit still fails
strict comparison. These changes do not establish full continuation, GC,
lifecycle, allocator or resource parity.

The [waiting-thread error checks](testing/2026-10-28-rust-lua-waiting-errors.md)
add an internal error destination so a throw through a waiting thread reaches
that thread's protected scope. Intervening C/Rust invocations return before the
destination's handler runs. Abandoned children keep their normal status, C/Lua
frames and open cells; their resume links are detached so collection can reclaim
them and close surviving upvalues. The corpus matches 736 release/checked
observations, including errors raised by a handler toward its own waiting parent.
C-call counts and 800 copy/activation controls are checked independently.
Completed native resumer invocations keep visible frames without adding a C-call
charge. These observations do not establish unprotected waiting-target behavior,
all interleaved API continuations, or full Lua 5.1 parity.

The [selected unprotected-call checks](testing/2026-10-28-rust-lua-thread-unprotected.md)
add `lua_call` through other stacks and cover 960 release/checked observations
plus 1,120 C-call count controls. They include protected waiting-state errors,
unprotected waiting-state panic recovery, and the stack/upvalue state left by
resetting a parent while its child is running. Twenty terminating scenarios
also compare buffered stdout and open-file writes. Native panic exit now flushes
VM-owned streams after a callback returns, sharing the flush used by `os.exit`.
General continuation, collector and lifecycle/allocator parity remain open.

The [selected-call yield checks](testing/2026-10-28-rust-lua-call-yields.md)
match 1,280 release/checked observations with Lua and C resumers, plus 4,224
C-call count controls. Native returns restore the pending Lua caller's register
window after an API call on its suspended state shrinks that storage. Cases
cover synchronous interleavings, protected errors, nested calls back through a
waiting state, repeated yields and a second yielded call that fails on resume.
Expired handlers remain inactive, and open cells survive until collection. A
portable regression also executes without default features. General
interleaved execution and full Lua 5.1 parity remain open.

The [collector integer checks](testing/2026-10-28-rust-lua-gc-integers.md)
match 1,063 observations on Windows x64 and i686 SSE2 against release and checked
references. Pause thresholds preserve unsigned wrapping and division order;
step limits preserve the 32-bit product followed by pointer-sized signed
assignment. Wide budget conversion also completes a 150,000-table i686 graph.
Negative/zero controls, finalizer identities, restart behavior and both native
widths are covered. Heap-count and general collector-work/timing parity remain
open.

The [selected-thread table checks](testing/2026-10-28-rust-lua-thread-tables.md)
cover `lua_gettable`, `lua_getfield`, `lua_settable`, `lua_setfield`, `lua_equal`
and `lua_lessthan` on another stack. Tag methods use that thread's context,
restore a yielded C activation's base after returning and retain their C-call
charge. Unprotected errors follow the selected protection/panic chain, including
waiting ancestors and abandoned child frames. Release and checked references
match 3,468 observations on both tested pointer widths. The CLI uses a 32 MiB
execution-thread stack to support recursive native calls in debug builds.
General metamethod continuations, allocation/GC accounting, lifecycle/allocator
APIs and full Lua 5.1 parity remain unfinished.

The [table-metamethod yield checks](testing/2026-10-28-rust-lua-table-yields.md)
match 1,078 observations through selected table/field and comparison APIs,
including direct C tag methods, typed yielded values, negative offsets and
repeated yields. Escaped calls retain pending frames while removing their
finished C-call charge and barrier. Setter resumptions preserve the operand
prefix, and collection clears open locals above the lowered physical top.
A portable regression repeats 760 observations without the default C feature.
Broader resumptions retain checked-reference invariant failures and reference
crashes; arbitrary reader/body/API interleavings, GC accounting, lifecycle and
allocator APIs, resource behavior and full Lua 5.1 parity remain open.

The [library initializer checks](testing/2026-10-28-rust-lua-library-init.md)
add `Vm::new_bare()` and the public C entrypoints for base, math, string, table,
OS and debug libraries. Bare states have empty globals and registry and no
string metatable. Opening uses the current C environment, preserves registry
table reuse and creates fresh closures; base also opens coroutine functions.
The checks match 66 transcript rows in each of twelve opening orders against
release and checked references on Windows x64 and Linux i686 SSE2. They cover
full C windows, intermediate registration values, setters and errors, closure
environments, reopening and initialization on fresh, yielded and dead threads.
Package and I/O initialization are covered below. The four core
lifecycle/allocator declarations and the broader standalone parity gates remain
unfinished.

The [package initializer](testing/2026-10-28-rust-lua-package-init.md) now exports
`luaopen_package`. It preserves registry table reuse, registration order, C
closure environments, path expansion, fresh loaders and preload tables, and
cached imports. Comparisons match 3,024 transcript rows per profile across
twelve opening orders and nine environment settings, including native loading,
partial errors and initialization on yielded or waiting C activations.
Lifecycle/allocator APIs and the other full Lua 5.1 parity gates remain open.

The [I/O close callback path](testing/2026-10-28-rust-lua-io-closers.md) now
looks up `__close` through table semantics and invokes its native body inside
the existing activation. Binding data stays separate from caller environments
and captures; modified C windows and yielded prefixes survive return hooks and
resumption. Comparisons match 231 observations on both tested widths. A separate
hook-yield audit retains unequal release behavior and a checked-reference
assertion. Lifecycle/allocator APIs and the broader standalone parity gates
remain unfinished.

The [I/O and full-library initializers](testing/2026-10-28-rust-lua-io-init.md)
now export `luaopen_io` and `luaL_openlibs`, bringing library entrypoint
declaration coverage to 8/8. `NativeCAbiLoader::standard_library(b"io")` and
`open_libraries()` expose them to Rust embeddings. Reopening creates fresh
closures, environments and standard userdata while sharing the underlying
stream buffers. Bare-state `loadfile()` can initialize stdin before the I/O
library. Full initialization calls each opener in its own C activation, in
Lua 5.1's standard order. Comparisons match 6,312 observations per profile on
Windows debug/release and Linux i686 SSE2 release. This proves the recorded
construction and stream cases; arbitrary native `FILE *` interoperability,
strict callback interleavings, collector accounting, lifecycle/allocator APIs
and full Lua 5.1 parity remain open.

The [native file userdata checks](testing/2026-10-28-rust-lua-native-files.md)
add real stdio resources for C-created `FILE **` userdata. Native payloads remain
authoritative across C-side closure, replacement and transfer; C and Lua share
the C runtime's buffers, scanner, positions and error state. Close handlers clear
the payload, while borrowed resource records do not close a handle on their own.
Comparisons match 47 observations per profile on Windows debug/release and Linux
i686 SSE2 release, with separate process-exit checks. Signed `ftell` failures and
nonzero child exit statuses are covered. Rust-created file payloads are extended
below. Shared C/Rust standard-stream buffering, lifecycle/allocator APIs and the
broader standalone parity gates remain unfinished.

The [Rust-created file payload checks](testing/2026-10-29-rust-lua-file-payloads.md)
extend real `FILE **` payloads to `io.open`, `io.tmpfile`, named input/output,
named line iterators and `io.popen` when the native C feature is enabled.
Constructors use the selected C API image from the first open, including files
created before any C API activation. `NativeLibraryLoader::as_any` lets a
configured C adapter provide that runtime; incompatible late image changes are
rejected while existing files remain usable. Comparisons cover 26 constructor
observations plus the prior 47 native-file observations per profile on Windows
debug/release and Linux i686 SSE2 release. Standard streams are extended below.
The four lifecycle/allocator APIs, collector accounting and the broader full
Lua 5.1 parity gates remain open.

The [standard-stream checks](testing/2026-10-29-rust-lua-standard-streams.md)
give stdin, stdout and stderr native payloads and one C stdio buffer per stream
when the C feature is enabled. `print`, stdin file loading, console input and
debugger diagnostics use those streams. Userdata pointers can be cleared or
replaced independently of the kernel streams, and reopening I/O preserves
unread input. `print` ignores output failures as Lua 5.1 does, while `io.write`
returns its error tuple. Both Rust and public C initialization match 13
observations per profile, with file regressions bringing the checkpoint to 297
observations across Windows debug/release and Linux i686 SSE2 release. The
lifecycle/allocator APIs, collector accounting, broader strict continuations,
diagnostics and resource/OOM gates remain unfinished.

The [physical C-hook stack checks](testing/2026-10-29-rust-lua-hook-stacks.md)
preserve popped slots until a write or collector traversal changes them. Saved
copies are unrooted, and a hook's restored top exposes the latest physical
contents. The 172-case public-header corpus matches release and checked Lua
5.1.5 on Windows debug/release and Linux i686 SSE2 release. A separate
pop-then-yield continuation mismatch remains recorded, including the checked
reference's VM-top assertion. Lifecycle/allocator APIs, collector accounting,
broader strict continuations and the other full parity gates stay open.

The [hook-yield checks](testing/2026-10-29-rust-lua-hook-yields.md) distinguish
the base selected by `lua_yield` from the top restored after a C hook returns.
Popped physical slots can become yielded values, and resume arguments overwrite
those slots after consumption. Further C API operations observe the new base
and status immediately; primitive table access no longer overwrites discarded
slots with interpreter temporaries. The 360-case corpus matches release and
checked references on Windows debug/release and Linux i686 SSE2 release. The
earlier eight pop-then-yield cases match release Lua while retaining their checked
VM-top aborts. A current I/O closer hook-yield mismatch and the remaining
lifecycle, allocator, collector and broader parity gates stay open.

The [signed hook-offset checks](testing/2026-10-29-rust-lua-hook-offsets.md)
preserve windows whose selected base lies above their restored top. Raw resume
retains negative offsets, C moves preserve physical slots, and the coroutine
library follows the reference's signed-result failure path. The 108-case corpus
matches release and checked references on Windows debug/release and Linux i686
SSE2 release. Wrapped and zero-count controls match release Lua while retaining
their checked aborts separately. The earlier I/O closer case now matches release
Lua on all three profiles, including `false, thread` and its later completion.
Lifecycle/allocator APIs, collector accounting, broader continuations and the
other full parity gates remain unfinished.

The [persistent native root stack](testing/2026-10-29-rust-lua-native-root.md)
adds `Vm::with_native_stack` and retained `NativeCDriver` entries. An idle Rust
VM can keep values on its main stack across separate native entries, without a
synthetic Lua-visible C activation. Public-header C drivers initialize libraries,
parse and dump functions, call Lua and C, run hooks/finalizers, and resume retained
coroutines on that stack. Root parser and protected-call depth limits match the
checked reference. Driver errors return through the protected Rust boundary.
At that checkpoint, four lifecycle/allocator declarations remained absent and
core coverage was 76/80. C-owned state support below extends this foundation.

The [C-owned default-state checks](testing/2026-10-29-rust-lua-native-owned.md)
add `luaL_newstate`, `lua_close` and `NativeCProgram` after engine bootstrap.
Each C-owned state retains its independent Rust VM and C image, accepts idle C
API calls, and survives between C program entries. Closing through a child
destroys the main group and retries remaining finalizers after errors. Root
errors unwind Rust before C panic handling, retain Lua's error status and close
escaped upvalues. Release and checked references match 89 observations on each
of three profiles through both the public SDK and an explicit C image.
Core declaration coverage is 77/80. `lua_newstate`, `lua_getallocf` and
`lua_setallocf`, a standalone C host artifact, exact allocation/GC accounting,
general interleavings and full parity remain open.


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
