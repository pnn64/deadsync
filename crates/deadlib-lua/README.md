# deadlib-lua

Independent Lua compiler and register interpreter in Rust, with no runtime
dependencies. This incremental implementation does **not** yet run arbitrary Lua
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

Base functions include `assert`, `error`, `type`, radix-aware `tonumber`, raw
access, `select`, `unpack`, `next`, `pairs`, `ipairs`, metatable operations,
`pcall`, `xpcall`, `tostring`, `print`, `getfenv`, `setfenv`, text `load`,
`loadstring`, `loadfile`, `dofile` and `newproxy`.
`select`, `unpack`, `string.char` and `string.rep` narrow integer arguments to
the reference's native 32-bit `int` after 64-bit Lua integer conversion.
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
frontier and sweep bounded batches of arena slots. Mutator barriers preserve new
references in reached tables, closures, environments and cells. Thread stacks,
open cells and tables with metatables are refreshed before weak processing.
New allocations and re-interned strings survive the active cycle. Full collection
restarts marking and remains synchronous. Individual object scans, final root
and weak processing, cache rebuilding and callback batches still contain atomic
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

The partial `os` library supplies `difftime`, `getenv`, `remove` and `tmpname`.
Time differences use the pinned MSVC 64-bit integer conversion and validation
order. Temporary names use the system temporary directory; Windows calls return
unused names without creating files. POSIX calls create private empty files.
Names and numeric components of temporary names are implementation-dependent.
Environment access reads the host process environment. Windows paths, environment
names and values currently use UTF-8 conversion; reference checks cover ASCII.
Windows CRT environment changes made outside this engine are not synchronized.
`execute` returns the Lua 5.1 numeric shell status, and `exit` flushes this VM's
open streams and shared stdout before terminating the process without Lua
finalizers. Windows shell selection, raw command lines and exit-code widths match
the pinned CRT probes. An explicitly empty Windows `COMSPEC` aborts as in that
reference build. The interpreter and compiler use safe Rust. Native declarations
are restricted to the Windows OS/locale bridges and narrow POSIX clock/calendar bridges. These
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
Windows has differential runtime coverage; selected Linux/Apple targets have
compile coverage only. Android declarations need target validation. Other
targets report unsupported calendar operations.
`setlocale` supports all six Lua 5.1 categories on Windows/MSVC, including native
names, aliases, default selection, queries and unavailable locales. Numeric
conversion, formatting and stream scanning follow the numeric decimal point;
source literals retain Lua's period syntax. Byte character classes, casing,
identifier scanning and cached patterns follow the ctype category. Pattern
iterators and replacement callbacks observe subsequent locale changes. String
ordering uses native collation, including Lua 5.1's embedded-NUL segmentation;
equality remains byte identity. Calendar formatting follows the time category.
Locale state is shared between Rust VMs. The native bridge holds the CRT locale
lock while copying its returned storage. Cached Rust views are updated through
this Lua API; external CRT locale changes and host per-thread locale modes are
outside the current contract. Other targets report unsupported native locales.
Broader locale/encoding coverage remains unfinished.

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

Other standard libraries, native C-library loading, complete diagnostic text and
debug behavior, persistent caches, host callback registration and
later dialects remain unfinished. Numeric conversion edge cases and resource
limits also need broader compatibility work. Native locales require Windows/MSVC,
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

```sh
cargo test -p deadlib-lua
cargo clippy -p deadlib-lua --all-targets --no-deps -- -D warnings
cargo tree -p deadlib-lua --edges normal,build
cargo bench -p deadlib-lua --bench baseline
cargo run -p deadlib-lua --bin deadlua -- -e 'assert(6*7 == 42)'
python scripts/check_rust_lua_suite.py --output docs/testing/rust-lua-suite.json
```

Tests use the official vendored Lua 5.1 interpreter through the legacy binding
as a development-only oracle. Production builds link no C Lua or third-party
crates. Observations compare number bits (including signed zero), byte strings,
booleans, arity and identity predicates. NaN payloads are excluded. This corpus
provides regression coverage, not full compatibility. The official suite runner
currently exits unsuccessfully: seventeen of eighteen reference and Rust probes pass. The Rust engine
passes unchanged `constructs.lua`, `locals.lua`, `nextvar.lua`, `vararg.lua`,
`pm.lua`, `events.lua`, `gc.lua`, `closure.lua`, `db.lua`, `math.lua`, `errors.lua`,
`sort.lua`, `literals.lua`, `strings.lua`, `calls.lua`, `attrib.lua` and `verybig.lua`. The retained
`files.lua` also fails in the C reference at a Windows open-file rename. The runner verifies the official
archive checksum and keeps the test files unchanged. See the
[suite report](../../docs/testing/2026-10-26-rust-lua-intern-hash.md) and
[work plan](../../docs/lua-engine.md) for remaining requirements and gates.
