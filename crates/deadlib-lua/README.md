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

Current Lua 5.1 coverage includes byte strings and lexical syntax, expressions,
assignments, blocks, conditionals, numeric/generic loops, table constructors,
functions, upvalues, recursion, varargs, methods, tail calls and multiple results.
Metatable dispatch includes indexing, writes, arithmetic, comparisons, calls,
concatenation and Lua 5.1 length behavior. Protected calls retain arbitrary Lua
error values; `xpcall` handlers run before faulting frames are unwound. Function
environments and dynamically compiled chunks preserve shared state and identity.
Native callbacks and metamethods use explicit continuation frames.

Base functions include `assert`, `error`, `type`, default-base `tonumber`, raw
access, `select`, `unpack`, `next`, `pairs`, `ipairs`, metatable operations,
`pcall`, `xpcall`, `tostring`, `print`, `getfenv`, `setfenv` and `loadstring`.
Numeric `math` and debug-metatable subsets are available. `compile_named` assigns
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

Coroutines, GC, userdata, other standard libraries, complete diagnostic text and
debug metadata, binary chunks, persistent caches, host callback registration and
later dialects remain unfinished. Numeric conversion edge cases and resource
limits also need broader compatibility work. In particular, `string.dump` is
unsupported, locale behavior is fixed to C, and patterns/table operations have
work limits. Heap
objects live until VM drop. Handles must remain in their originating VM. The API
and bytecode are provisional. The CLI accepts a file, stdin or `-e source`.

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
currently exits unsuccessfully: all eleven reference probes pass; the Rust engine
passes unchanged `locals.lua`, `vararg.lua` and `pm.lua`, while other probes reach
missing functions. It verifies the official
archive checksum and keeps the test files unchanged. See the
[suite report](../../docs/testing/2026-10-24-rust-lua-stdlib-suite.md) and
[work plan](../../docs/lua-engine.md) for remaining requirements and gates.
