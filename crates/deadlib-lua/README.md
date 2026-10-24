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

Fixed-source compilation uses a resumable byte scanner. It preserves token and
string-decoding state across append-only input chunks, including split escapes,
numbers, long delimiters and CR/LF pairs. Reader-based `load` still requires parser
resumption and its protected callback boundary; it is not yet available.

Current Lua 5.1 coverage includes byte strings and lexical syntax, expressions,
assignments, blocks, conditionals, numeric/generic loops, table constructors,
functions, upvalues, recursion, varargs, methods, tail calls and multiple results.
Metatable dispatch includes indexing, writes, arithmetic, comparisons, calls,
concatenation and Lua 5.1 length behavior. Protected calls retain arbitrary Lua
error values; `xpcall` handlers run before faulting frames are unwound. Function
environments and dynamically compiled chunks preserve shared state and identity.
Native callbacks and metamethods use explicit continuation frames.

Base functions include `assert`, `error`, `type`, radix-aware `tonumber`, raw
access, `select`, `unpack`, `next`, `pairs`, `ipairs`, metatable operations,
`pcall`, `xpcall`, `tostring`, `print`, `getfenv`, `setfenv`, `loadstring` and `newproxy`.
The Lua 5.1 `math` function set and debug metatable/environment subsets are available. Binary
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

Host copies of `Value` do not keep objects alive. Use `Vm::pin`, `rooted` and
`unpin` for values retained across executions or collections. Root handles check
VM ownership and generations; ordinary object handles must stay in their
originating VM. A live host `Program` retains its per-VM entry closure/environment,
while escaped Lua closures retain their code independently. `Vm::heap_stats`
reports object counts and capacity-based memory estimates, including reusable
storage. These are not process RSS or exact allocator peaks.

Other standard libraries, light userdata, complete diagnostic text and
debug metadata, binary chunks, persistent caches, host callback registration and
later dialects remain unfinished. Numeric conversion edge cases and resource
limits also need broader compatibility work. In particular, `string.dump` is
unsupported, locale behavior is fixed to C, and patterns/table operations have
work limits. Debug stack inspection and hooks remain open. The API and bytecode
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
currently exits unsuccessfully: all thirteen reference probes pass; the Rust engine
passes unchanged `locals.lua`, `nextvar.lua`, `vararg.lua`, `pm.lua`, `events.lua`
and `gc.lua`, while other probes reach
missing functions. It verifies the official
archive checksum and keeps the test files unchanged. See the
[suite report](../../docs/testing/2026-10-25-rust-lua-userdata.md) and
[work plan](../../docs/lua-engine.md) for remaining requirements and gates.
