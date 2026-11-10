# deadlib-lua

Independent Lua 5.1 parser/compiler, bytecode VM, garbage collector, standard
library and host API, implemented in Rust. Normal production builds have no
third-party runtime crates or C Lua linkage. Language and host behavior are checked against pinned native reference fixtures.

Lua C API/ABI compatibility is permanently outside the supported interface.
There is no C compatibility feature, exported Lua C entry point, compatibility
header, C-owned state, C module loader or generated lua51 shared library.
Platform/OS FFI, intrinsics and assembly are separate from Lua's C API and remain
available where needed. Native Lua 5.1 bytecode loading/dumping is preserved.

## Build and use

```sh
cargo run --locked -p deadlib-lua --bin deadlua -- -e 'assert(6*7 == 42)'
cargo test --locked -p deadlib-lua
cargo clippy --locked -p deadlib-lua --all-targets --no-deps -- -D warnings
cargo fmt -p deadlib-lua -- --check
```

```rust
use deadlib_lua::{compile, Value, Vm};

fn main() -> Result<(), deadlib_lua::Error> {
    let program = compile(b"local x = 6; return x * 7")?;
    let mut vm = Vm::try_new()?;
    assert_eq!(vm.execute(&program)?, &[Value::Number(42.0)]);
    Ok(())
}
```

Compilation is independent of a VM. Verified programs can run in multiple VMs;
mutable Lua objects belong to their originating VM. `ProgramCache` persists
verified code with source, target, dialect and compiler identities. `build.rs`
still computes the program-cache build identity; it does not compile C code.
See [the cache contract](../../docs/lua-cache.md).

## Rust embedding contract

`Vm::try_new` opens the standard libraries; `try_new_bare` creates a state
without them. Both return startup allocation errors and release partial state.
`new`, `new_bare` and `intern` remain infallible convenience APIs. Fallible
operations and the existing ownership/recovery regressions remain supported.

`execute_with_args`, `call`, `load_file` and `try_load_program` load and execute
programs. Table/value operations, userdata, environments, debug hooks, protected
calls, arbitrary Lua error values and coroutines use the shared VM and GC.

`new_host_function` registers a synchronous Rust callback. `HostCall` borrows its
arguments and appends results separately. Lua captures are traced and mutable;
`new_host_function_with_data` additionally owns typed Rust data until the closure
is collected. Put retained Lua values in traced captures: opaque Rust data is
not scanned. Nested `HostCall::call` uses a protected Lua boundary. A synchronous
Rust callback cannot suspend its Rust stack. After an unwinding Rust panic,
discard the VM; interrupted execution skips Lua finalizers.

`HostCall::try_new_userdata` allocates zeroed, VM-owned byte storage in the
callback's environment and propagates allocation or VM memory-limit errors.
Root or publish the returned value before calling Lua or collecting garbage.

`table_host_state` and `set_table_host_state` attach a private table to a Lua
table through `Vm` or `HostCall`. The owner traces that state strongly, including
when its ordinary entries are weak. Lua field/metatable edits and iteration do
not expose the attachment. Unreachable cycles remain collectible. This supports
embedding-owned state that must follow Lua object lifetime without a global root.
The host still owns the slot's policy; it is not copied by ordinary Lua table copies.

Copies of `Value` do not keep objects alive. Use `pin`, `rooted`, `unpin` and
temporary root scopes across calls or collection. Roots check VM ownership and
generations. The existing `NativeStack`, `NativeThreadStack`, callback/body and
hook APIs are Rust interfaces used by host execution, not Lua C ABI exports.
Their stack, protected-error and selected-thread behavior remains supported.
See the API documentation and the `host_*`, `native_*`, `userdata` and coroutine
tests for the retained contracts.

## Module loading

`require` supports Lua source/native-chunk files, Lua or Rust functions in
`package.preload`, custom Lua loaders and `package.loaded` caching. `LUA_PATH`,
`package.path`, module environments and `package.seeall` remain supported.

The Lua 5.1 package table retains four loader slots. Slots 1 and 2 load preload
and Lua-file modules. Slots 3 and 4 validate the module name and return exactly
`"\n\tC modules are not supported by deadlib-lua"`. They never open libraries.
`package.loadlib(path, symbol)` validates its string arguments, then always
returns `nil, "C modules are not supported by deadlib-lua", "absent"`.
`package.cpath` initially contains the empty string; `LUA_CPATH` is ignored.
Scripts may assign `cpath`, but it has no effect. There is no `_LOADLIB` registry
or host library-loader hook. Supply native functionality through Rust callbacks
and preload modules.

## Testing

Initialize the pinned [test fixtures](../../README.md#test-fixtures) before
running the crate suite. Language, host ownership, allocation recovery, coroutine
and module behavior are covered by the current implementation's regression
tests. Native fixture generation tools are development utilities; production
consumers use the shared Rust engine described in the
[architecture guide](../../docs/lua-architecture.md).
