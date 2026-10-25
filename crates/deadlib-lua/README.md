# deadlib-lua

Independent Lua compiler and register interpreter in safe Rust, with no runtime
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
reuse scratch storage. Physical allocation still rejects some reference-valid
programs with many indexed assignment targets; full resource-limit parity is
unfinished.

Current Lua 5.1 coverage includes byte strings and lexical syntax, expressions,
assignments, blocks, conditionals, numeric/generic loops, table constructors,
functions, upvalues, recursion, varargs, methods, tail calls and multiple results.
Metatable dispatch includes indexing, writes, arithmetic, comparisons, calls,
concatenation and Lua 5.1 length behavior. Protected calls retain arbitrary Lua
error values; `xpcall` handlers run before faulting frames are unwound. Function
environments and dynamically compiled chunks preserve shared state and identity.
Native callbacks and metamethods use explicit continuation frames.

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
`Vm::collect_garbage` are available. Each cycle currently completes atomically;
`step` accumulates a work budget before running a complete cycle. Incremental
pauses remain unfinished.

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

`Value::LightUserdata` carries an unmanaged opaque identity. It owns no storage,
is never dereferenced, survives weak tables and has a metatable shared by light
userdata within its VM. Its numeric identity may be shared across VMs; arena
object handles retain their ownership restrictions. Package loading uses this
value type for its observable sentinel.

Other standard libraries, native C-library loading, complete diagnostic text and
debug behavior, binary chunks, persistent caches, host callback registration and
later dialects remain unfinished. Numeric conversion edge cases and resource
limits also need broader compatibility work. In particular, `string.dump` is
unsupported, locale behavior is fixed to C, and patterns/table operations have
work limits. Broader source/register diagnostics, hook coverage and native
scratch-stack behavior remain open. The API and bytecode
are provisional. The CLI accepts a file, stdin or `-e source`.

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
currently exits unsuccessfully: fifteen of sixteen reference probes pass; the Rust engine
passes unchanged `constructs.lua`, `locals.lua`, `nextvar.lua`, `vararg.lua`,
`pm.lua`, `events.lua`, `gc.lua`, `closure.lua`, `db.lua`, `math.lua` and `errors.lua`. The retained
`files.lua` also fails in the C reference at a Windows open-file rename. Other
probes reach missing libraries/functions. It verifies the official
archive checksum and keeps the test files unchanged. See the
[suite report](../../docs/testing/2026-10-25-rust-lua-register-budget.md) and
[work plan](../../docs/lua-engine.md) for remaining requirements and gates.
