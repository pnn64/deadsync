# Bare states and standard library initializers

`Vm::new_bare()` creates a state with empty globals and registry, and no string
metatable. Compilation, core language execution and host callbacks remain
available. `Vm::new()` retains its existing library initialization sequence.
`Vm::new_native_function()` binds a resolved native symbol without requiring
the package library to have been opened.

The bundled C API now exports `luaopen_base`, `luaopen_math`, `luaopen_string`,
`luaopen_table`, `luaopen_os` and `luaopen_debug`. The base initializer also
opens `coroutine`. `NativeCAbiLoader::standard_library()` resolves these six
entrypoints from the same C API image used by the VM's other native calls.
Library construction uses the public C API; the executable library bodies
remain in Rust.

Opening a library follows Lua 5.1 registration order and registry lookup rules.
Reopening reuses the registered table while creating fresh C closures. Function
environments follow the current C activation, including a selected thread's
suspended activation. String initialization replaces its type metatable;
base initialization creates fresh iterator captures and a fresh proxy weak
table. A table setter can observe the intermediate removed `string.gfind`
function before the compatibility alias replaces it.

This exposed a builtin-body stack bug: coroutine creation, wrapping, status
and running returned their original arguments through C instead of the new
result suffix. Their full C windows now retain the arguments and append the
results. On the main thread, `coroutine.running` appends the main thread object
and then nil, matching the reference's internal stack.
The [preserved CLI comparison](2026-10-28-rust-lua-library-init-coroutine-before.json)
records four incorrect results when an ordinary C module calls these function
bodies directly. The [current CLI comparison](2026-10-28-rust-lua-library-init-coroutine-current.json)
matches both references for all four results.

The [Windows debug comparison](2026-10-28-rust-lua-library-init-focused.json),
[Windows release comparison](2026-10-28-rust-lua-library-init-current.json) and
[i686 SSE2 comparison](2026-10-28-rust-lua-library-init-i686-current.json) each
match release and assertion-enabled Lua 5.1.5. Each profile compares 66 transcript
rows in twelve cyclic or reversed opening orders, for 792 rows per profile.
The unchanged public-header C fixture observes:

- Empty initial globals and registry, and the absent string metatable.
- Each initializer's complete C stack, result count, aliases and table identity.
- Registered function names, executable bodies, coroutine yields and resumptions.
- Reopening, fresh closures, proxy validity and string metatable replacement.
- Cached registry tables, registry `__index`, registration setters and order,
  partial registration errors, global name conflicts and closure environments.
- Fresh, yielded and dead selected threads, their globals and C environments,
  preservation of their existing stack and subsequent resumptions.

The [baseline](2026-10-28-rust-lua-library-init-before.json) preserves the CLI and
C SDK from commit `a2057303b`; none of the six entrypoints were exported there.
The [i686 verification](2026-10-28-rust-lua-library-init-i686-verification.json)
checks artifact and source hashes, ELF32 width, SSE2 flags, checked assertions
and the six SDK exports. Successful Rust comparison processes also assert
balanced native dispatches, states, groups and library lifetimes.

[Full validation](2026-10-28-rust-lua-library-init-validation.json) passes 809
tests on Windows x64 and 810 on Linux x64 and i686 SSE2. Strict Clippy, formatting,
checks without default features and the empty normal/build dependency tree pass.
The [portable tests](2026-10-28-rust-lua-library-init-portable.json) execute bare
state and builtin-body regressions with the default C feature disabled.
The [library argument audit](2026-10-28-rust-lua-library-init-stdlib.json) matches
all 5,507 cases. The [combined native audit](2026-10-28-rust-lua-library-init-boundary-current.json)
rechecks the existing native corpora and cleanup guards.

Both native-enabled i686 CLIs pass unchanged `main.lua` and `all.lua` in the
[full stock suite](2026-10-28-rust-lua-library-init-i686-suite.json); T/testC remains
absent. [Windows independent files](2026-10-28-rust-lua-library-init-windows-suite.json)
retain 17/18 passes in all four modes, with the shared `files.lua:59` rename
failure. The [evidence manifest](2026-10-28-rust-lua-library-init-evidence.json)
binds the current sources, reports, binaries, public modules and logs.

Full Lua 5.1 parity remains unfinished. `luaopen_io`, `luaopen_package` and
`luaL_openlibs` remain unimplemented. Core C declaration coverage remains
76/80: `lua_close`, `lua_getallocf`, `lua_newstate` and `lua_setallocf` are still
missing. Exact allocation/heap accounting, collector work, broader strict
continuation failures, reader/body/API interleavings, diagnostics and resource
or OOM behavior remain open. These library entrypoints do not close those gates.
