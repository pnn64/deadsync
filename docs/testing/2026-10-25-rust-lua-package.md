# Lua 5.1 package and light-userdata validation

The independent engine provides `require`, `module`, `package.preload`,
`package.loaded`, four searchers, path/config fields and `package.seeall`.
Preloaded, Lua-file and user-defined loaders run through explicit native phases
and the flat scheduler. Module files reuse the existing protected file frontend;
its internal continuation stays hidden from hooks and stack inspection.

`require` reads the registry's `_LOADED` table, retaining that table throughout
the operation even if a callback replaces the registry or package field. Truthy
values are cached, false values trigger another load, and missing module returns
default to true only while the loading sentinel remains. Provider failures retain
the sentinel so later attempts report a loop/previous error. Searchers run in
array order through a captured loader table, stopping at its first nil entry.
String/number search errors accumulate; callbacks and table metamethods preserve
their normal side effects and arbitrary error values.

Lua paths replace module dots with the platform separator and substitute every
`?` in each nonempty template. State creation reads `LUA_PATH`/`LUA_CPATH`, expands
double separators using the defaults, and expands executable-directory marks on
Windows. A readable Lua file is loaded with its filename and receives only the
module name as its argument. Its thread environment and escaped object identity
are shared with other execution.

`module` reuses loaded tables or creates dotted globals, initializes `_M`, `_NAME`
and `_PACKAGE` when needed, changes its Lua caller's environment and then invokes
options in order. `seeall` reuses or creates the module metatable and assigns the
current thread globals as its index fallback. Module setters and option callbacks
can invoke Lua, collect, inspect/mutate locals and propagate errors.

The package sentinel uses real light userdata. `Value::LightUserdata(u64)` holds
an unmanaged identity without owning or dereferencing memory. It has raw identity
equality, a distinct table key, noncollectible weak-table behavior, no environment
and a per-VM type metatable. Its ID may cross VMs; full object handles retain their
VM ownership restrictions. Values remain 16 bytes and frames 64 bytes on this
target. The collector roots light-userdata metatables independently of values.

Searcher/provider callbacks observe `require`'s five retained native slots.
Module options observe the original arguments, loaded table and two distinct
module-table copies. Captured names remain separate from mutable argument slots,
including embedded-NUL names. Mutation of either module copy affects its own
subsequent uses. Native-call limit checks also cover nested metatable callbacks;
a cheap barrier/depth guard avoids a stack walk for ordinary shallow calls.

All 210 tests pass: 196 integration groups, thirteen unit groups and one runnable
documentation example. Formatting, strict Clippy and the zero normal/build
dependency gate pass. Twenty-five new groups cover:

- Package/registry identity, builtins, native environments and default paths.
- Cache values, false/nil results, retries and provider argument/result arity.
- Recursive/failed imports, light-userdata sentinels and weak references.
- Loader order, table snapshots, callable loaders and accumulated messages.
- Named Lua sources, repeated template marks and traditional dotted modules.
- Search errors, malformed modules, missing C paths and searcher result counts.
- Module fields, caller environments, option ordering and conflict behavior.
- Registry, preload, module and metatable callbacks under collection.
- Arbitrary byte names, NUL prefixes and package argument diagnostics.
- Nested imports, hooks, faulting stacks, error handlers and yield restrictions.
- Native temporary inspection and mutation without aliasing captured values.
- Non-null/null unmanaged identities, raw/weak table keys, type metatables,
  mixed full/light-userdata comparisons and pinned-platform pointer formatting.
- Independent VM metatables sharing one unmanaged identity.
- Native recursion limits against the reference.
- A provider quota abort retaining its initialized sentinel and restoring main.
- Two thousand transient imports with bounded reusable storage below 2 MB.
- A file-import abort releasing its reader and allowing immediate file deletion.
- Environment path expansion in separate Rust/reference child processes.

Three groups are engine-only: cross-VM host identities, import-abort/reclamation
and file-reader abort cleanup. The native-loading group compares missing-path
searches with the reference and separately verifies the engine's disabled-loading
status. Other groups compare scalar bits/bytes, arity and identity predicates with
vendored official Lua 5.1.5 on Windows x86-64 MSVC, static CRT and C locale. Plain
`lua_pcall` entries avoid host error-handler slots below the test chunk. Fixtures
are owned files/directories under `target`; environment changes stay in child
processes. No engine unsafe code or runtime dependencies were added.

Native C-library loading is unsupported. `package.loadlib` returns nil, an
explanation and `"absent"`; a readable C-module candidate reports disabled
loading. These results are not Windows DLL-loader parity. Binary Lua chunks,
native loader registry internals, exact scratch layouts beyond the tested phases,
full diagnostics, unusual path/device/encoding cases and broad resource limits
remain open. Compiler bounds, incremental GC, remaining OS/base functions,
persistent caches, later dialects and gameplay integrations also remain unfinished.

```sh
cargo test -p deadlib-lua
cargo fmt -p deadlib-lua -- --check
cargo clippy -p deadlib-lua --all-targets --no-deps -- -D warnings
cargo tree -p deadlib-lua --edges normal,build
python scripts/check_rust_lua_suite.py --output docs/testing/rust-lua-package-suite.json
```

The [official report](2026-10-25-rust-lua-package-suite.json) retains sixteen
unchanged probes, the pinned archive checksum, `_U=true`, a 100-million-instruction
budget and 60-second per-file deadlines. Rust passes ten: `constructs.lua`,
`locals.lua`, `nextvar.lua`, `vararg.lua`, `pm.lua`, `events.lua`, `gc.lua`,
`closure.lua`, `db.lua` and now `math.lua`. Fifteen reference probes pass. The
retained `files.lua` reference failure is the Windows rename of the still-open
default input file at line 59; Rust reaches missing `os.tmpname` at line 19.

| Remaining probe | Rust stopping point |
| --- | --- |
| `calls.lua` | `string.dump`, line 243 |
| `literals.lua` | `os.setlocale`, line 161 |
| `strings.lua` | `os.setlocale`, line 150 |
| `sort.lua` | `os.clock`, line 25 |
| `errors.lua` | Syntax-message assertion, line 20 |
| `files.lua` | `os.tmpname`, line 19 |

These are independent probes, not complete `all.lua` or C API coverage. Production
uses the preserved legacy engine. The
[paired measurements](../performance/2026-10-25-rust-lua-package.md) include thirty
runtime workloads and eight frontend cases. Behavior was checked against vendored
official `loadlib.c`/VM sources and the
[Lua 5.1 manual](https://www.lua.org/manual/5.1/manual.html#pdf-require).
