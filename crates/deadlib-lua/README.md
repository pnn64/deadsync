# deadlib-lua

Independent Lua compiler and register interpreter in Rust, with no third-party
runtime crates or C Lua linkage. This incremental implementation does **not** yet run arbitrary Lua
programs. Production consumers use [deadlib-lua-legacy](../deadlib-lua-legacy/README.md).

```rust
use deadlib_lua::{compile, Value, Vm};

fn main() -> Result<(), deadlib_lua::Error> {
    let program = compile(b"local sum=0; for i=1,100 do sum=sum+i end; return sum")?;
    let mut vm = Vm::new();
    assert_eq!(vm.execute(&program)?, &[Value::Number(5050.0)]);
    Ok(())
}
```

Programs contain immutable, verified bytecode and work across independent VMs.
VMs retain code needed by escaped closures, globals, table identities and shared
upvalue cells across executions. Registers and call frames are contiguous
vectors. Calls use an explicit frame stack; tail calls reuse the frame. Values
are copyable enum tags with numeric object handles.
Interning accepts arbitrary byte slices and preserves string identity through
growth and collection. Its safe word loads hash every input byte; collision
resolution compares the complete byte slice. No per-string hash field is stored.

Fixed-source compilation and reader-based text `load` share a resumable scanner
and parser. Flat grammar continuations preserve partial tokens, expressions and
statements across input chunks without reparsing earlier input. Reader callbacks
use protected VM frames, preserve arbitrary error values and obey Lua 5.1 yield
restrictions. Loaded functions use the thread environment after input is read.
Code generation also uses flat continuations for blocks, expressions, argument
lists and nested functions. Ancestor capture propagation is iterative. Compilation
does not consume host stack in proportion to source nesting or expression-tree
depth; the frontend's Lua syntax limits still apply.

Function constants retain first-use order. Small pools use linear lookup; once
a pool holds 64 constants, subsequent lookups use a temporary flat u32 index.
Hash collisions compare complete typed values. Numeric equality merges both
zero signs while NaNs remain distinct. String lookups borrow bytes and allocate
an owned copy only for a new constant. The index is released when the function
is finalized, leaving the existing immutable program layout intact. See the
[compiler measurements](../../docs/performance/2026-10-26-rust-lua-constant-pool.md).

Arithmetic and index chains reuse anonymous result registers and defer right
operand storage. Named destinations retain their old values through callbacks
and metamethods. This keeps long left-associated chains within bounded scratch
storage while preserving local operand reads and floating-point evaluation order.
Constructors reuse top anonymous result slots; calls evaluate their callee before
reserving arguments one at a time. Methods retain their receiver before lookup
and argument evaluation. Compact literal record keys retain pool insertion order
while materializing after their values. These layouts support deep constructors
and long call/method chains without accumulating unused temporary slots.
Text `loadfile` and `dofile` share this frontend and the buffered stream backend.
They accept named files or underlying stdin, skip an initial `#` line, and retain
file source names. `dofile` runs through the flat scheduler, preserving multiple
results, errors and C-call restrictions. Source readers close on success, errors
and instruction aborts. Standard input keeps its lookahead across I/O and loading;
changing `io.input` does not redirect the loader.

Precompiled Lua 5.1 input has a separate resumable decoder and structural
verifier. Fixed buffers, readers and files recognize the leading escape byte,
check the native ABI header and decode flat prototype trees without replaying
input prefixes or reserving storage from untrusted declared counts. Instruction
checks cover all 38 native opcodes, constants, captures, jumps, open results and
SETLIST payloads. Named binary files reopen in binary mode before validation.
Malformed chunks report native-style diagnostics and reader stopping points.
Verified native chunks translate to the shared register VM, including closures,
per-site captures, varargs, loops, native source/debug metadata and hook events.
Translation scratch slots have collector liveness maps and remain hidden from
debug locals. Native concatenation preserves range coercions and callbacks;
generic-for iterator calls preserve native yield/depth boundaries. `string.dump`
lowers source-compiled functions to native Lua 5.1 chunks and re-exports imported
functions using retained native words/debug offsets. Dumps omit captured values
and function environments; loaded roots receive nil cells and the loading thread's
environment. Additional arguments, including a strip flag, are ignored as in 5.1.
Export checks native register/operand limits and inserts guarded relays when
lowering expands a branch beyond the native signed-jump range. Relays preserve
capture/open-result instruction groups, source lines and named local scopes
without reserving registers. Source-compiled dump bytes need
not match the C compiler's instruction stream. Binary coverage is incremental, not a claim of
complete compatibility with every C-accepted chunk.

Source diagnostics retain Lua 5.1 token labels, decoded string context, opening
delimiter lines and local/assignment limit descriptions. Lexical capture checks
enforce the 60-upvalue limit while parsing, including unfinished source, and
identify the first overflowing ancestor function. Reader lookahead precedes the
check, preserving reader-error and lexical-error ordering. `Error::message_bytes`
preserves non-UTF-8 diagnostic bytes; `message` and display provide readable UTF-8.
Reader and file loaders return the original diagnostic bytes to Lua. Long-token
scanning uses safe eight-byte delimiter checks without reading beyond input.

The parser separately tracks Lua 5.1 logical register use, including constant
operands, pending constructor fields, call arguments and multiple results. A
request reaching 250 logical slots fails at the reference token/reader boundary.
Physical result slots grow as expressions finish, so large result lists can
reuse scratch storage. Reference constant operands are retained as sparse AST
metadata, independently of the backend pool. Keys reuse
one store scratch slot; local table/key references reserve no unused storage.
Per-function zero signs preserve reference numeric pooling even when backend
conditions materialize extra constants. Numeric left operands and reference
constant operands of arithmetic/comparisons materialize after the right result,
so deep right-associated chains reuse a bounded working area. Broader
resource/debug/scratch compatibility remains unfinished. Small arithmetic trees
of numeric literals and variable references keep grouped loads when a cached
32-node bound fits the remaining physical registers.

Current Lua 5.1 coverage includes byte strings and lexical syntax, expressions,
assignments, blocks, conditionals, numeric/generic loops, table constructors,
functions, upvalues, recursion, varargs, methods, tail calls and multiple results.
Metatable dispatch includes indexing, writes, arithmetic, comparisons, calls,
concatenation and Lua 5.1 length behavior. Protected calls retain arbitrary Lua
error values; `xpcall` handlers run before faulting frames are unwound. Function
environments and dynamically compiled chunks preserve shared state and identity.
Native callbacks and metamethods use explicit continuation frames.

Eligible right-associated concatenation chains use one range operation and the
same buffered runtime path as imported native chunks. Operand evaluation order,
left-associated groups, logical paths and named assignment destinations remain
observable at their original points. Range operands stay rooted through callbacks.

Runtime type errors recover scoped local, global, upvalue, field and method names
from verified code and existing debug metadata. Name reconstruction runs only
when formatting an error and retains raw byte names. Metatable redirection and
anonymous intermediate values keep their reference diagnostic behavior.

Native argument diagnostics retain caller names, method numbering and bad-self
messages. Missing arguments differ from explicit nil, and type, domain, option
and file errors preserve their Lua 5.1 validation order. Formatting runs only on
failure and preserves raw byte names and option/path context.

`debug.getinfo` reports function sources, definitions, active/current lines,
upvalue counts, caller-derived names and function identity. Stack queries include
lost tail calls and active, suspended or failed coroutine frames. `getupvalue`
and `setupvalue` inspect and mutate shared Lua cells, including cells owned by
other threads. `getlocal` and `setlocal` access parameters, scoped locals, loop
controls and mapped temporary/argument slots, including suspended/failed threads.
Prototype metadata, call sites and temporary ranges are verified; names resolve
through prototype indices during inspection. `debug.traceback` formats current or
other-thread stacks, including lost tail calls, reference truncation and native
concatenation callbacks. Formatting resumes through flat VM continuations, with
retained target roots across callbacks and coroutine switches. `debug.sethook`
and `gethook` provide per-thread call, return, tail-return, line and count hooks.
Callbacks can inspect/mutate stacks, collect and resume children; recursive hook
delivery is suppressed. Broader instruction-event parity and exact scratch-stack
bounds remain unfinished.

Unary instructions read local operands directly at their hook boundary. Constant
logical negation folds without a temporary load; negated comparisons retain the
original predicate/metamethod and invert its later boolean materialization.
[Unary validation](../../docs/testing/2026-10-26-rust-lua-unary-hooks.md) covers
hook mutation, NaNs, native exports and persistent reloads.
Negated logical values retain separate short-circuit tests and final boolean
loads, including repeated negation and callback-visible named destinations.
Pending branches use flat compiler jobs and linked instruction slots.
[Boolean validation](../../docs/testing/2026-10-26-rust-lua-boolean-hooks.md)
covers both paths, long chains, native export and fresh-process cache reloads.

Arithmetic, comparisons and table access accept register-or-constant operands.
Eligible constants remain in the program pool instead of being written into
hidden operand registers. Source debug metadata records the reference logical
stack bound independently of physical scratch registers. Hooks expose unnamed
slots and their registry-table slot; temporary writes remain roots while the
hook runs. Return hooks use the actual result window, including empty returns
and open results. [Validation](../../docs/testing/2026-10-26-rust-lua-rk-windows.md)
also covers native import/export and schema-2 persistent reloads. Complete
scratch-stack and hook parity remain unfinished.

Thread marking clears unused Lua register slots, including scalar values exposed
by debug hooks. Native arguments, open upvalues and active hook/result windows
remain roots. Open hook results can extend beyond the fixed register bitmap.
The collector builtin also preserves its pushed result in a caller's spare
slot. [Stack-clearing validation](../../docs/testing/2026-10-26-rust-lua-stack-clearing.md)
covers explicit and stepped collection, suspended threads and persistent reloads.

Base functions include `assert`, `error`, `type`, radix-aware `tonumber`, raw
access, `select`, `unpack`, `next`, `pairs`, `ipairs`, metatable operations,
`pcall`, `xpcall`, `tostring`, `print`, `getfenv`, `setfenv`, text `load`,
`loadstring`, `loadfile`, `dofile` and `newproxy`.
`select`, `unpack`, `string.char` and `string.rep` narrow integer arguments to
the reference's native 32-bit `int` after pointer-width Lua integer conversion.
`unpack` and `string.byte` check the 8,000-slot native stack limit, including
unused arguments, before growing result storage. Overflow errors are catchable
Lua errors; empty unpack ranges return no values.
`require`, `module` and the package namespace provide preloaded, Lua-file and
custom loaders, shared import caching and Lua 5.1 module environments.
The Lua 5.1 `math` function set and debug metatable/environment operations are available. Binary
decomposition/scaling preserve signed zero and round subnormals once. The random
stream matches the MSVC reference, including per-OS-thread state shared by VMs;
other CRT random sequences are not emulated. `compile_named` assigns
chunk names for error locations. `print` writes to stdout unless capture is
enabled through `Vm::set_output_capture`.

`math.ldexp` checks its exponent before its mantissa. Binary `pow`, `fmod`/`mod`
and `atan2` check their second operand first on Linux/glibc and their first
operand first elsewhere. These error orders follow the pinned GCC and MSVC
reference builds; C itself does not specify the evaluation order. The
[library argument audit](../../docs/testing/2026-10-28-rust-lua-thread-tables-stdlib.json)
now matches all 5,507 bounded cases, including native-loading argument errors.
It does not establish full standard-library parity.

The development [native-boundary proof](../../docs/testing/2026-10-26-rust-lua-native-boundary.md)
exercises real C callbacks through the Rust host API, including recoverable
errors under `panic=abort`. Its C-only jump shim now also serves the default
Windows/Linux native adapter; the complete Lua C API remains unfinished.

`HostCall::native_stack` opens a VM-owned mutable argument/temporary window.
`NativeStack` supports signed indexes, growth/pop, numeric reads, nested calls
and selection of the last values as results. Its 8,000-slot bound is independent
of closure captures. Popped arguments stop acting as roots, and return hooks can
inspect and mutate selected result slots before copying them to the caller.
The [native-stack validation](../../docs/testing/2026-10-26-rust-lua-native-stack.md)
replaces the development bridge's capture-based temporary stack.

`NativeStack::call` protects nested errors so Rust may recover.
`call_unprotected` instead retains failing child frames, locals and open upvalues
until the enclosing Lua handler runs. Errors remain pending if Rust ignores
them, and later stack writes cannot overwrite the fault window. Protected
loaders reached through this path inherit the enclosing error handler; an
intervening protected call stops that inheritance. The development C bridge
uses this path for `lua_call`. See the
[unprotected-call checks](../../docs/testing/2026-10-26-rust-lua-unprotected.md).

`HostThreadState` attaches typed host resources to a Lua thread, including the
main thread. `HostCall` exposes its thread identity and initializes each type
once. State roots are validated, traced and protected by the incremental GC
barrier; they must remain unchanged after initialization. Resources survive
coroutine suspension and drop with a collected thread, or after shutdown Lua
finalizers. Their reported allocation bytes contribute to heap accounting.
The [persistent-state proof](../../docs/testing/2026-10-26-rust-lua-thread-states.md)
keeps a C state pointer stable across calls, reentry, suspension and VM moves,
and frees every state while keeping its DLLs loaded. It covers operations on
the currently executing thread; complete inactive/cross-thread C API access
remains unfinished.

`NativeThreadStack` also supports table creation and raw access/traversal,
metatables, userdata and host closure creation, environments and indexed
upvalues. These operations use the target thread's calling environment and
preserve its execution status and stack prefix. Shared Lua cells and native
captures remain live collector roots. The
[inactive-object checks](../../docs/testing/2026-10-28-rust-lua-inactive-objects.md)
compare real C operations on fresh, yielded, failed, completed and waiting
threads. `NativeStack::protected_call_thread` also executes protected calls
through a selected thread's own stack, with result counts, error handlers and
its retained status. The [thread-call checks](../../docs/testing/2026-10-28-rust-lua-inactive-calls.md)
cover reentry, hooks and later resumption. The
[API-yield checks](../../docs/testing/2026-10-28-rust-lua-api-yields.md) also match
protected yields on completed, yielded and failed threads, preserving pending
frames and expiring the original error handler. Completed targets can resume
with the requested result count and live shared cells. Unprotected calls,
direct metamethod operations and other executing APIs on inactive threads remain
unfinished.

`NativeStack::load_reader_thread` also compiles through a selected thread's own
stack without resuming its pending computation. The C adapter supports
`lua_load` and its buffer/string helpers on these stacks. Reader callbacks see
the target's environment and status, and may reenter Lua or another loader.
The [thread-loader checks](../../docs/testing/2026-10-28-rust-lua-thread-loads.md)
cover syntax/reader errors, stack-base restoration and permitted reader yields.
Broader executing APIs remain open.

The [reader-continuation checks](../../docs/testing/2026-10-28-rust-lua-reader-continuations.md)
also preserve the parser's final physical top and suspended reader frames when
loading succeeds after a yield. The visible window can be empty even after
successful parsing. Current-function environments follow the retained activation.
The release corpus matches 122 observations. A separate strict resume audit
still fails on five reference crashes, and the checked reference rejects further
continuation shapes; arbitrary reader continuation parity remains unproven.

The [native panic checks](../../docs/testing/2026-10-28-rust-lua-native-panic.md)
add `lua_atpanic` and unprotected `lua_error` on idle other threads. Callback
replacement is shared within a VM and isolated across VMs. Stack reset closes
upvalues and preserves the original error; C callbacks run after Rust dispatch
returns and can recover through a C-local jump. Fifty-four observations and
three terminating subprocess cases match Lua 5.1.5. Protected routing into a
waiting target and complete unprotected error/lifecycle behavior remain open.

The [selected-thread file checks](../../docs/testing/2026-10-28-rust-lua-thread-files.md)
add `NativeStack::load_file_thread` and direct `luaL_loadfile` on another thread.
Named files and buffered stdin preserve the target's environment, pending frames
and retained status. File-open failures keep a yielded C suffix; parser failures
restore its CallInfo base. Filename-push finalizer errors reach an idle target's
latest panic callback after Rust dispatch returns. The comparisons cover 111
observations and two additional terminating subprocess cases. Protected error
routing into a waiting target remains open.
An additional public-module CLI audit retains a mismatch in automatic GC timing;
explicit full-collection controls isolate and verify the new panic routing.

The [native allocation checks](../../docs/testing/2026-10-28-rust-lua-native-alloc-gc.md)
add the required collector safe points to allocating C API operations. Completed
primitive host actions avoid entering the interpreter for an unrelated check.
Finalizer calls restore the C stack base on selected threads; steps without a
tag method keep a yielded suffix. All 126 allocation/control observations match,
including original errors and C-local panic recovery. Supplementary checks cover
operand visibility and mutation during collection. General GC timing remains
unfinished, including the retained unsettled file-panic mismatch.

The [selected-thread execution checks](../../docs/testing/2026-10-28-rust-lua-thread-execution.md)
add thread creation, formatted messages and primitive/metamethod concatenation
on another thread. New threads inherit the target's environment and hook
configuration. Concatenation follows Lua 5.1's relative operand offsets when a
tag call restores a suspended C thread's base, including returned operand
identity. Idle-target errors reach its panic callback and close surviving
upvalues. The comparisons cover 54 execution observations, ten panic observations
and 156 allocation/control observations. Four concatenation-yield cases remain
strict mismatches against both release and checked references. Waiting-target
error routing, general GC timing and full Lua 5.1 parity remain unfinished.

The [concatenation-yield checks](../../docs/testing/2026-10-28-rust-lua-concat-yields.md)
close those four initial mismatches. Selected concatenation retains pending Lua
frames and continues the C API's physical stack reductions after a permitted
yield. `top_offset` exposes the signed Lua 5.1 stack offset; `top` counts
accessible values. Forty observations cover yielded-value counts, pushes through
negative offsets, completed-target resumptions, repeated yields and surviving
upvalues. Broader resumptions still include checked-reference invariant failures
and reference crashes; these remain separate strict audits. Full continuation,
GC, lifecycle and resource parity remain unfinished.

The [selected-thread collector checks](../../docs/testing/2026-10-28-rust-lua-thread-gc.md)
add `NativeStack::gc_thread` and `lua_gc` on another thread. Controls remain
shared across the VM; finalizers observe the selected thread and its calling
context. Fifty-two control/finalizer observations and one hundred panic
observations match release and checked references. Heap-count cases check the
integer/range contract, without claiming equal allocation charges. Invalid
resumption states from the broader audit now return Lua errors instead of
aborting Rust. That audit remains unequal, and full continuation, collector,
lifecycle, allocator and resource parity remain unfinished.

The [waiting-thread error checks](../../docs/testing/2026-10-28-rust-lua-waiting-errors.md)
route errors through the selected thread's protection chain. Child handlers are
bypassed, abandoned threads retain their normal status and fault frames, and
open upvalues close when those threads are collected. C jumps still occur after
Rust dispatch returns. The release and checked references match 736 observations
across Lua/C resumers, protected API calls, GC errors, error handlers and eight
error-value types. Supplementary checks cover C-call counts and 800 count-copy
and activation controls. Broader cross-thread continuations remain unfinished.

The [selected unprotected-call checks](../../docs/testing/2026-10-28-rust-lua-thread-unprotected.md)
add `lua_call` through other thread stacks. They match 960 observations for
results, errors, waiting protection scopes and panic recovery, plus 1,120 C-call
count controls. Unprotected waiting states now reach their panic callback and
retain the reset stack and closed upvalues while Rust returns through discarded
invocations. Twenty terminating cases also preserve buffered stdout and file
writes. Interleaved continuations, collector behavior and lifecycle/allocator
coverage remain unfinished.

The [selected-call yield checks](../../docs/testing/2026-10-28-rust-lua-call-yields.md)
cover 1,280 protected/unprotected call and resume observations, including
synchronous calls while a yield is pending, nested calls through other threads,
repeated yields, and a second pending call that raises on resume. Native returns
restore the pending Lua caller's register window after an intervening API call
shrinks its storage. Release and checked references also match 4,224 C-call count
controls. Arbitrary interleaved execution and full Lua 5.1 parity remain open.

The [collector integer checks](../../docs/testing/2026-10-28-rust-lua-gc-integers.md)
match 1,063 observations in 64-bit and 32-bit reference profiles. Pause thresholds
preserve wrapping and division order; step limits preserve the unsigned product
and signed target width. A wide intermediate retains large i686 budgets before
conversion into tracing units. Negative controls, finalizer error identities and
a 150,000-table graph are covered. Exact heap counts and collector work scheduling
remain unfinished.

The [selected-thread table checks](../../docs/testing/2026-10-28-rust-lua-thread-tables.md)
add metamethod-capable table/field access and equality/ordering on other stacks.
Callbacks run in the selected thread; errors follow its protection or panic
chain. Completed tag calls restore a yielded C thread's original stack base and
retain their C-call charge. Release and checked references match 3,468 observations
on Windows x64 and Linux i686 SSE2, including waiting parents and open cells.
The CLI runs Lua on a thread with a 32 MiB stack so recursive native calls can
reach Lua's C-call limit in debug builds. Embedding hosts choose their execution
thread's stack size. General native continuation and resource parity remain open.

The string library provides byte operations, method lookup, Lua patterns,
`find`, `match`, independent `gmatch` iterators, `gsub` with string/table/function
replacements, and Lua 5.1 formatting. Patterns use cached compact instructions,
bitmap classes and iterative backtracking. Malformed patterns report errors when
matching reaches them. The table library includes raw concatenation, insertion,
removal, size queries, traversal callbacks and sorting. Sort comparison order and
callback mutations are checked against the reference. Scratch buffers are reused
across calls; nested native callbacks keep separate continuation state.
Pattern searches, table concatenation and insertion impose no artificial work
count limit. Execution quotas apply at VM driver steps and do not interrupt a
running native loop. `table.sort` checks its native stack reserve before validating
the comparator and discards unused arguments before comparisons.
Sort and traversal reread native argument slots changed through `debug.setlocal`.
Replaced sort arguments stop acting as collector roots. Traversal keeps its
iteration state separate from callable self arguments and discarded native results.
Native operand errors retain their C-frame origin; Lua operations retain source
locations, and protected-call argument diagnostics retain names and slot numbers.
Native string checks coerce numeric arguments in place. Return hooks and partial
pattern/I/O failures preserve the argument slots independently of result writes.
Malformed formatting conversions preserve Lua 5.1's diagnostic bytes, including
the omitted NUL for a terminal or binary NUL conversion specifier. Literal NULs
in the format text remain in successful output.
`string.gsub` rereads its replacement after callbacks and releases replaced
objects. Its native buffer stack exposes partial strings, preserves lazy numeric
replacement conversion, and resumes Lua concatenation metamethods without a
recursive Rust driver. Pending buffer actions remain rooted across collection.

Coroutines provide `create`, `resume`, `running`, `status`, `wrap` and `yield`.
Each thread owns its register/frame vectors and environment; threads share the
heap, closures and object identity. Open upvalues can read and write suspended,
normal and failed thread stacks. Resume/yield uses an explicit scheduler with
no recursive interpreter entry. Lua 5.1 C-call yield restrictions and inherited
resume-depth limits are checked against the reference. An instruction quota
covers all resumed threads and restores the main context after an abort.

An iterative tracing collector reclaims cyclic tables, strings, closures, code,
native closures, cells and unreachable threads. Lua 5.1 weak tables keep strings
strong and do not use later dialects' ephemeron rules. Collection traces native
continuations and compiler register maps, and closes surviving cells before
freeing their thread. Object slots carry generations so stale handles cannot
alias reused slots. `collectgarbage`, `gcinfo`, automatic pressure checks and
`Vm::collect_garbage` are available. Automatic checks and `step` advance a marking
frontier and sweep bounded batches of arena slots. Table propagation visits at
most 64 physical array/hash slots per work item, counting holes and tombstones.
Scans resume after their children; compaction, hash-to-array moves and metatable
changes restart pending scans. Mutator barriers preserve new references in
reached tables, closures, environments and cells. Thread stacks,
open cells and tables with metatables are refreshed before weak processing.
New allocations and re-interned strings survive the active cycle. Full collection
restarts marking and remains synchronous. Other object scans, final root and
metatable refreshes, weak processing, cache rebuilding and callback batches contain atomic
work; fully bounded pauses remain unfinished.

Full collection, explicit steps and the host collection API re-enable automatic
collection. Restart waits for the next allocation; pause settings take effect
when a cycle completes. A successful finalizer restores the collector controls
saved on entry; a failing finalizer retains its last changes. A pending
finalization phase resumes before another mark/sweep begins, including the
[finalizer phase regression](../../docs/testing/lua51-gc-finalizer-phase.lua).
Exact PUC step work units and fully incremental behavior remain unfinished.

Full userdata has byte storage, independent identity, a metatable and an
environment. `newproxy` follows Lua 5.1's weak registry of valid metatables.
Finalizers run once in reverse creation order through explicit continuation
frames, support resurrection and nested collection, and retain arbitrary Lua
error objects. Weak values clear before finalization while weak keys survive
until a later cycle. VM destruction finalizes existing reachable userdata and
continues after Lua errors. Instruction limits cover finalizer execution.
Hosts can allocate userdata with `Vm::new_userdata` and borrow its byte storage.
`Vm::collect_garbage` returns `Result<HeapStats, Error>` so finalizer failures
can propagate; pending finalizers remain available to later collection.

Lua recursion overflow is catchable. Each thread tracks Lua 5.1's logical
call-info growth and error-handler reserve independently of its frame vector.
Protected recovery and GC update that capacity; internal continuations consume
no Lua call slots. Error handlers can re-enter after runtime errors, use the
native-call emergency range and terminate with the reference error object when
that range is exhausted. Instruction aborts remain uncatchable.

The I/O library provides named/temporary files, default streams, process pipes,
reads, writes, seeking, buffering, close/type queries and line iterators. File
userdata owns a generation-checked stream record and uses the shared collector
and finalizer machinery. Lua 5.1 writes return booleans; iterator closures retain
their file. Windows text files translate newlines and Ctrl+Z, including the
reference's buffered seek cookies. Captured stdout keeps logical byte order across
`print` and I/O writes; the Windows CLI emits translated newlines.

Numeric stream reads preserve the native runtime's consumed prefix on failure.
The Linux/glibc profile consumes incomplete exponents while converting their
mantissa, accepts `0x.` as zero, and consumes the mismatching byte of a failed
infinity or NaN token. Special-token matching follows ctype before ASCII numeric
conversion, including Turkish casing. GNU end-of-file state remains set across
line-iterator calls; ordinary reads and successful seeks clear it. The
[numeric stream checks](../../docs/testing/2026-10-27-rust-lua-gnu-numbers.md)
retain values, file offsets, unread bytes and math error messages from real CLIs.
The Windows/MSVC profile also preserves NUL/`0xff` lookahead loss at native
floating-parser stage boundaries. A lost byte can expose more digits, an
exponent or an infinity/NaN suffix. The
[expanded stream audit](../../docs/testing/2026-10-27-rust-lua-ucrt-streams.md)
checks every byte after selected prefixes, including the distinct lookahead
behavior of special and generic NaN payloads.

The partial `os` library supplies `difftime`, `getenv`, `remove` and `tmpname`.
Time differences convert inputs to the native `time_t` width before subtraction,
preserving the pinned reference's validation order. Windows temporary names use the system temporary directory and return
unused names without creating files. POSIX calls reserve extensionless private
empty files under `/tmp`.
Names and numeric components of temporary names are implementation-dependent.
Environment access reads the host process environment. Windows paths, environment
names and values currently use UTF-8 conversion; reference checks cover ASCII.
Windows CRT environment changes made outside this engine are not synchronized.
`execute` returns the Lua 5.1 numeric shell status, and `exit` flushes this VM's
open streams and shared stdout before terminating the process without Lua
finalizers. Windows shell selection, raw command lines and exit-code widths match
the pinned CRT probes. An explicitly empty Windows `COMSPEC` aborts as in that
reference build. The interpreter and compiler use safe Rust. Native declarations
are restricted to the Windows OS/locale bridges and narrow POSIX clock/calendar/locale bridges. These
add no Cargo dependency or C Lua linkage.

`clock` uses the native runtime's clock and preserves its epoch and units. The
pinned MSVC build measures wall time since CRT initialization in milliseconds;
the declared Linux/Android/Apple targets use their native CPU clock. Other targets
currently report an unsupported clock. Windows `rename` refuses to replace an
existing destination; same-volume moves are atomic and cross-volume behavior
uses the CRT's copy flag. POSIX replacement follows its native rules. Errors
retain the source filename and Lua 5.1 result tuple. Longer paths
retain their complete names through native conversion.

`time` reads calendar fields in Lua 5.1 order through the existing callback
scheduler, preserving metamethods, errors, yield restrictions and collection.
Native conversion normalizes an owned calendar record without changing the Lua
input table. `date` returns local or UTC fields and formats each conversion with
the native formatter, retaining the reference's NUL and percent handling.
Calendar result tables reserve their known field count and charge that storage
to the collector before filling the fields.
Calendar validity, DST rules and format extensions follow the pinned runtime.
Windows and Linux/glibc have differential runtime coverage; selected Apple
targets have compile coverage only. Android declarations need target validation. Other
targets report unsupported calendar operations.
`setlocale` supports all six Lua 5.1 categories on Windows/MSVC and Linux/glibc, including native
names, aliases, default selection, queries and unavailable locales. Numeric
conversion, formatting and stream scanning follow the numeric decimal point;
source literals retain Lua's period syntax. Byte character classes, casing,
identifier scanning and cached patterns follow the ctype category. Pattern
iterators and replacement callbacks observe subsequent locale changes. String
ordering uses native collation, including Lua 5.1's embedded-NUL segmentation;
equality remains byte identity. Calendar formatting follows the time category.
Locale state is shared between Rust VMs. Windows copies native storage under
the CRT locale lock. Linux/glibc owns native locale objects and borrows them
under a mutex for collation, dates and error messages. Character classes and
case maps publish together under a read/write lock. Linux locale changes affect
engine VMs without mutating the host's process-global C locale. External native
bindings and host locale synchronization remain outside this contract. Other
libc ABIs report unsupported native locales. The
[POSIX locale checks](../../docs/testing/2026-10-27-rust-lua-posix-locale.md)
cover queries, category restoration, encodings, numeric streams and concurrent
native use. Broader locale/encoding coverage remains unfinished.

Physical stdout is shared across VMs and buffers logical bytes before Windows
newline translation. `print`, stdout writes and `setvbuf` share that buffer;
`os.execute` leaves pending output buffered. Explicit flush, process exit and VM
shutdown flush it. Terminal behavior, unusual buffer sizes, host-owned streams
and exit-time flushing of named files belonging to other VMs need broader checks.

`debug.getregistry` returns the shared, rooted registry. Its `FILE*` entry controls
file type checks, and `_LOADED` contains the registered builtin modules. The package
library shares that table through `package.loaded`. `require` preserves loader
order, truthy cache results, retryable false results and the loading/error
sentinel. Path templates, environment defaults and `module`/`seeall` use Lua 5.1
rules. Callbacks use explicit continuations and retain their native argument
slots. Native C-library loading remains unsupported; `package.loadlib` returns
nil, an explanation and `"absent"`. Windows path conversion currently uses UTF-8; ASCII
paths are covered by the reference checks. Exact standard-stream buffering, more
device/error cases, native scratch stacks and I/O cancellation remain open.

Host copies of `Value` do not keep objects alive. Use `Vm::pin`, `rooted` and
`unpin` for values retained across executions or collections. Root handles check
VM ownership and generations; ordinary object handles must stay in their
originating VM. A live host `Program` retains its per-VM entry closure/environment,
while escaped Lua closures retain their code independently. `Vm::heap_stats`
reports object counts and capacity-based memory estimates, including reusable
storage. These are not process RSS or exact allocator peaks.

`Vm::execute_with_args` supplies chunk parameters and varargs; `Vm::call` invokes
live functions, builtins and callable objects. Both reuse the existing call driver
and validate handles. `new_table` and `raw_table_set` build host-owned data, while
`set_global` honors the global table's `__newindex`. `Vm::load_file` uses the VM's
file streams and verifier independently of a replaced Lua `loadfile` global.
`load_program` creates a callable binding without running it; dropping the host
Program before calling lets Lua ownership determine its environment's lifetime.
`begin_standalone`/`end_standalone` add a parked C main frame and protected message
handling for CLI-style entry. Default/custom tracebacks inspect live faulting
frames, and uncaught runtime errors force collection after the handler returns.
`get_global` honors global `__index` callbacks; `global` performs a raw lookup.
`run_interactive` runs the Lua 5.1 console inside a standalone context, reporting
recoverable diagnostics through a caller-provided function.

`Vm::new_host_function` registers a Rust function pointer as a native Lua
function. `HostCall::arguments` borrows the original register range, while
`push` appends results in separate storage. Each registration has its own
identity and up to 255 captured Lua values; `set_capture` updates them with the
collector barrier. Captures and returned values can retain tables, functions,
threads and userdata. Native userdata allocation inherits the function's
environment. Callback errors use `error` for a message or `raise` for an exact
Lua error value, including nil and object identities.

`new_host_function_with_data` also owns private typed binding data, borrowed
through `HostCall::function_data`. This data is separate from Lua upvalues and
is released with the closure. Retained Lua values belong in traced captures;
opaque data is not traced. Its boxed size enters heap accounting, while
allocations it owns indirectly are excluded. `NativeStack::push_closure`
replaces topmost captures with a closure inheriting the current environment.
Native stack indexes also support the Lua 5.1 registry, environment, globals
and upvalue pseudo-indexes. Negative numeric argument errors retain the supplied
index. The [C closure checks](../../docs/testing/2026-10-26-rust-lua-c-closures.md)
exercise these operations against the same C module built for official Lua.
The Lua source compiler's separate 60-upvalue limit remains in place.

Native stacks now support table creation with capacity hints, raw and
metamethod-aware access, field and integer-key operations, actual metatable
access, traversal and ordinary stack insertion/removal. Metamethods run through
the shared VM scheduler without adding Lua wrapper frames. Their failures keep
the native inputs and failing Lua frames available to outer handlers; field
keys remain private temporaries. The
[native table checks](../../docs/testing/2026-10-26-rust-lua-native-tables.md)
compare these operations, call limits and overflow-handler reserves against
official Lua in matching standalone contexts. These operations also serve the
default native-module adapter.

Native stacks also expose full userdata payloads, function/userdata/thread
environments, raw object lengths and numeric-to-string conversion in the
original slot or capture. Userdata storage is zero-initialized, aligned for
pointers and doubles, and stable while rooted. Interned strings retain a
trailing NUL outside their logical byte length; binary strings keep embedded
NUL bytes. The [native userdata checks](../../docs/testing/2026-10-26-rust-lua-native-userdata.md)
compare payload mutation, finalizers, resurrection, environments and stable C
string pointers against official Lua. Complete C API and module loading remain
open compatibility gates.

`NativeStack::protected_call` implements Lua 5.1 protected execution with an
optional error-function index. It returns `NativeCallStatus`, replaces the
function/arguments with results or one error value, and keeps the callback
usable after runtime errors. Error handlers run before faulting frames and open
locals unwind. Handler failure has its own status, even when a successful
handler returns identical diagnostic bytes. The
[protected native checks](../../docs/testing/2026-10-27-rust-lua-native-protected.md)
compare `lua_pcall`, `lua_cpcall`, handler recursion and stack effects against
official Lua. Instruction/resource limits still abort the outer Rust entry.

Native value operations also provide nonmutating number/integer/boolean
conversions, integer pushes, equality, less-than comparison and concatenation.
Comparisons and concatenation share the VM's metamethod scheduler and preserve
fault frames. Concatenation coerces and reduces the original operand slots;
their visible top and roots remain intact until the whole operation succeeds,
as in Lua 5.1. Comparison callbacks can release cleared operand roots without
private argument copies retaining them. The
[native value checks](../../docs/testing/2026-10-27-rust-lua-native-values.md)
compare thirteen additional C API functions against the Windows/MSVC reference.
The C API remains partial.

`NativeStack::push_where` provides Lua 5.1 source prefixes, and
`argument_error` applies caller names and method/self adjustment. Primitive
formatted-message concatenation preserves Lua 5.1's lack of a collector check
inside the formatter. The development bridge adds C varargs formatting and
auxiliary argument checks, tagged userdata, metafields, library registration,
dotted table creation and reference handles. Its
[auxiliary API checks](../../docs/testing/2026-10-27-rust-lua-native-auxiliary.md)
compare the same C module against official Lua, including error-stack locals,
binary strings, capture/environment inheritance and reference reclamation.
These helpers also serve the default native-module adapter; full standalone
parity remains open.

Callbacks run synchronously and can allocate strings, tables and userdata or
perform raw table access. `HostCall::call` invokes a callable through a protected
nested Lua boundary; `call_and_return` appends its results without an intermediate
vector. Nested calls share the outer instruction quota. Rust may handle runtime
errors, while resource limits still abort the outer entry. Returned objects and
error values remain rooted until a later call or result write reuses their
storage; retain them in captures or results before then. Child coroutines may
resume and yield, but execution cannot suspend the Rust callback's stack.
Panics follow the Rust build strategy; after an unwind, discard
the VM. Dropping that interrupted VM skips Lua finalizers. Raw object identities
can coincide across VMs, so values and captures must remain in their originating
VM. [Callback measurements](../../docs/performance/2026-10-26-rust-lua-host-reentry.md)
cover direct host entry, nested Lua calls, Lua loops, allocations, heaps and process CPU.

`ProgramCache` loads or saves immutable compiled programs in a caller-selected
directory. It compares full source/name bytes, compiler build and lexer locale
classes, bounds decoding, checks integrity and verifies shared bytecode. Native
chunk debug/export metadata is preserved. Cache misses and I/O failures fall
back to ordinary compilation; `CacheStatus` reports Hit, Stored or Bypassed.
Each VM still creates its own runtime objects and environments. Use a directory
trusted to hold executable code. Default limits are 64 MiB per file and 256 MiB
of decoded object/payload storage. Tiny programs and native chunks can be slower
through disk caching; the API is opt-in. See the [format and API notes](../../docs/lua-cache.md)
and [measurements](../../docs/performance/2026-10-26-rust-lua-constant-pool.md).

`debug.debug` runs commands against the current thread while preserving the
suspended caller stack for inspection and mutation. It uses original stdin,
writes prompts and diagnostics to stderr, and resumes after syntax/runtime
errors. Commands use 249-byte input fragments and return with exact `cont\n`
input or EOF. Each completed command clears its native arguments and results.
Non-string/non-number errors use `(null)`; the C reference's null-pointer output
for those values is outside the byte-exact contract.

`Value::LightUserdata` carries an unmanaged opaque identity. It owns no storage,
is never dereferenced, survives weak tables and has a metatable shared by light
userdata within its VM. Its numeric identity may be shared across VMs; arena
object handles retain their ownership restrictions. Package loading uses this
value type for its observable sentinel.

`NativeStack::get_stack`, `get_info`, `get_local` and `set_local` expose Lua 5.1
activation records for current and inactive threads. Inspection uses the selected
thread's native stack, including function-pop and result-push effects. Debug
strings share interned storage and remain stable while their program is reachable;
activation records and string handles do not root their owners. The development
C fixture compares the corresponding four APIs; native hooks and the remaining
C APIs require broader compatibility checks.

External native hooks use `NativeStack::set_hook` and `hook` for per-thread
controls and `HostCall::hook_event` for the activation record and event. The
callback runs against the interrupted native/Lua stack without adding a visible
function frame. Hooks retain their host binding and captures through collection,
including after disabling themselves. Temporary pushes alias debug locals and
are removed after delivery; hidden VM storage stays separate. Native callbacks
can call Lua, inspect/mutate locals and resume children with recursive hook
delivery suppressed on their own thread. Line/count hooks can yield zero values
and resume the pending instruction. The development C boundary adds `lua_sethook`,
`lua_gethook`, `lua_gethookmask` and `lua_gethookcount`. Its
[native hook checks](../../docs/testing/2026-10-27-rust-lua-native-hooks.md)
compare native event records, pointer restoration, signed counts, stack aliases,
coroutine inheritance/yields and nested C-call counts. These checks also cover
math and iterator return windows. Broader builtin scratch stacks, instruction
events and complete C API coverage remain open gates.

Native API environment/upvalue pseudo-indexes inside an external hook refer to
the interrupted function. Hook registration data and Rust captures remain
separate through `HostCall`. Userdata and new native closures inherit the actual
calling function's environment, including when scheduler continuations are
present. Inactive stack access follows the same visible function identity for
waiting and hook-suspended threads. Replacing the calling environment on a fresh
thread returns `no calling environment` without changing its globals. The
[native context checks](../../docs/testing/2026-10-27-rust-lua-native-context.md)
compare these reads, writes and allocation identities against official Lua.

Base/string return hooks now see the library's retained and consumed arguments,
temporary values and actual result suffix, with writes changing pending results.
Discarded native arguments and scratch stop retaining objects. Lua reader-based
`load` shares the incremental native parser-root model, preserving constants
tables, prototypes, EOF nils and protected error stack layouts. The
[return-stack checks](../../docs/testing/2026-10-27-rust-lua-return-stacks.md)
compare 252 C-hook observations and portable Lua debug-hook/reader regressions.

Return-hook errors escape a completed `pcall`, `xpcall` or loader's protection
scope. Active parents still protect child call/return hooks, and loaders invoked
inside a hook inherit the active outer error handler. The
[protection-scope checks](../../docs/testing/2026-10-27-rust-lua-protection-hooks.md)
compare 39 C-hook scenarios plus portable Lua/native hook regressions.

The [library stack checks](../../docs/testing/2026-10-27-rust-lua-library-stacks.md)
extend return-hook windows across table, debug, I/O and OS operations, including
cross-thread debug calls, failed-open userdata, close temporaries and iterator
EOF. `debug.setupvalue` consumes the actual topmost argument; nil/closed default
line and close arguments follow the reference's checks. Four portable groups
cover Lua-hook writes, result aliases and collector lifetime effects.

The [native body checks](../../docs/testing/2026-10-27-rust-lua-native-bodies.md)
add `lua_tocfunction` to the development C boundary. Builtin implementation
pointers can be compared, called directly, or copied into closures with their
own captures and environments. Direct body calls preserve the current function
and C stack without another visible frame or call/return hook. Body handles do
not retain the closure that supplied them. The comparison covers 543 copied-body
stack observations, coroutine suspension, external hooks and collection. The
boundary now declares 76 of 80 core APIs; complete semantics remain unproven.

The [package stack checks](../../docs/testing/2026-10-27-rust-lua-package-stacks.md)
extend those comparisons to `require`, `module`, `seeall` and package searchers.
`require` maintains its live C window through loader and cache callbacks.
Retained nils, import sentinels, module copies, preload tables and search strings
remain visible and writable; consumed options/results release their roots.
Captured search names remain independent of writes to exposed temporary slots.
These fixes also apply when native code copies and calls builtin bodies.

The [native-library integration checks](../../docs/testing/2026-10-27-rust-lua-library-loading.md)
add a host ABI resolver to `package.loadlib` and the C/C-root searchers. Library
handles live in registry-cached `_LOADLIB` userdata, with failed-open retries,
symbol errors, explicit close/reopen and collection/shutdown cleanup. Resolved
functions inherit the actual calling environment and use the ordinary native
boundary. `Vm::set_native_library_loader` installs or replaces an adapter.

The [default C ABI checks](../../docs/testing/2026-10-28-rust-lua-default-native.md)
enable real native modules in `Vm::new` and the CLI on Windows and Linux through
the default `native-c-api` feature. The build compiles a small C-only jump shim
with MSVC or a target C compiler; it contains no C Lua VM and adds no normal/build
Cargo dependency. Its image is embedded and loaded lazily from a byte-verified
per-build cache. Windows modules can link the generated `OUT_DIR/lua51.lib`;
Linux modules resolve the globally loaded C API. Public-header module tests cover
the actual CLI, child coroutines, replacement adapters, parallel VMs and cleanup.
`--no-default-features` omits the bundled adapter and its C build. Custom providers
remain available. The adapter declares 76 of 80 core APIs; this is partial C API
support, with lifecycle/allocator entry points and broader semantics still open.

Other standard libraries, complete native C API semantics, diagnostic text and
debug behavior, cache eviction/package integration, richer Rust host objects and
later dialects remain unfinished. Numeric conversion edge cases and resource
limits also need broader compatibility work. Native locales require Windows/MSVC or Linux/glibc,
and broader native resource bounds need validation.
Broader source/register diagnostics, hook coverage and native
scratch-stack behavior remain open. The API and bytecode
are provisional. The CLI accepts scripts with positional arguments, stdin, chained
`-e`/`-l` actions, `--`, `-` and `LUA_INIT` strings/files. Script values appear in
`...` and the Lua 5.1 `arg` table, including negative option indices. `-i` enters
the console after commands/scripts; an argument-free terminal enters it directly.
The console supports `=expression`, continued chunks, mutable prompts, shared stdin
and recovery after chunk/print errors. The version banner identifies DeadLua.
Console signals, terminal-mode/echo details and broader standalone compatibility
remain unfinished; raw CLI audit results retain all banner differences.

On POSIX, `os.tmpname` reserves an extensionless file under `/tmp`, matching
Lua 5.1's template. These names can be required directly when `package.path`
contains `?`. The
[standalone suite checks](../../docs/testing/2026-10-27-rust-lua-full-suite.md)
run unchanged `main.lua` and `all.lua` through each real CLI, including child
processes. The explicit Linux i686 SSE2 profile passes both stock entry points
in C and Rust. Stock mode still skips T/testC and native suite libraries;
full standalone parity remains open.

The i686 profile uses pointer-width Lua integers, 32-bit C long/unsigned long
and time values, and binary64 SSE2 arithmetic. Its reference build selects
`-m32 -msse2 -mfpmath=sse`; native builds keep their default flags. The runner
checks both executables' ELF32 headers and native Lua chunk format. The default
i686 x87 compiler model remains a recorded diagnostic baseline with different
rounding; it is not the declared Rust arithmetic profile. See the
[i686 suite checks](../../docs/testing/2026-10-27-rust-lua-i686-suite.md).

```sh
cargo test -p deadlib-lua
cargo clippy -p deadlib-lua --all-targets --no-deps -- -D warnings
cargo tree -p deadlib-lua --edges normal,build
cargo run -p deadlib-lua --bin deadlua -- -e 'assert(6*7 == 42)'
python scripts/check_rust_lua_suite.py --output docs/testing/rust-lua-suite.json
python scripts/check_rust_lua_full_suite.py --output docs/testing/rust-lua-full-suite.json
python scripts/check_rust_lua_unpack.py --run-root target/unpack-runs --output docs/testing/rust-lua-unpack.json
```

On Linux with the i686 Rust target and C multilib toolchain installed:

```sh
CARGO_TARGET_DIR=/tmp/deadsync-lua-target CFLAGS_i686_unknown_linux_gnu='-msse2 -mfpmath=sse' cargo test -p deadlib-lua --target i686-unknown-linux-gnu
CARGO_TARGET_DIR=/tmp/deadsync-lua-target python3 scripts/check_rust_lua_full_suite.py --target i686-unknown-linux-gnu --reference-fp-model sse2 --native-libraries --run-root /tmp/deadsync-lua-suite --output docs/testing/rust-lua-i686.json
```

Tests use the official vendored Lua 5.1 interpreter through the legacy binding
as a development-only oracle. Production builds link no C Lua or third-party
crates. Observations compare number bits (including signed zero), byte strings,
booleans, arity and identity predicates. NaN payloads are excluded. This corpus
provides regression coverage, not full compatibility. On Windows, the official
file runner exits unsuccessfully: seventeen of eighteen reference and Rust probes pass. The Rust engine
passes unchanged `constructs.lua`, `locals.lua`, `nextvar.lua`, `vararg.lua`,
`pm.lua`, `events.lua`, `gc.lua`, `closure.lua`, `db.lua`, `math.lua`, `errors.lua`,
`sort.lua`, `literals.lua`, `strings.lua`, `calls.lua`, `attrib.lua` and `verybig.lua`. The retained
`files.lua` also fails in the C reference at a Windows open-file rename. The runner verifies the official
archive checksum and keeps the test files unchanged. On Linux, all eighteen
independent files pass in reference, Rust and both cache modes; unchanged
`all.lua` still fails at `big.lua:11` in both 64-bit engines; i686 SSE2 passes
both full stock entry points. See the
[suite report](../../docs/testing/2026-10-26-rust-lua-boolean-hooks.md) and
[work plan](../../docs/lua-engine.md) for remaining requirements and gates.
The [portable regression checks](../../docs/testing/2026-10-27-rust-lua-portable-tests.md)
also retain GNU C crashes on overflowing `unpack` ranges in separate processes.
The strict standalone audit continues to fail on those inputs. Sanitizer and
signed-wrapping builds are diagnostic controls, while the default reference
and production engines keep their existing compiler flags.
