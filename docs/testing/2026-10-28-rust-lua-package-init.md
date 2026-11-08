# Package library initialization

The C SDK now exports `luaopen_package`, also available through
`NativeCAbiLoader::standard_library(b"package")`. It opens the package library
in a bare state, using the existing Rust implementations for imports, module
environments, searchers, native loading and library finalization.

Initialization reuses `_LOADLIB` and the registry's loaded package table. It
replaces the finalizer, library functions, four loaders, `preload`, and global
`module`/`require` closures. It retains the loaded-module cache. Registration
order, setter calls, partial failures, path/config fields and the complete C
stack follow Lua 5.1. The metatable remains below the returned package table.

The initializer changes its own C closure environment to the package table.
Functions created before that change retain the previous environment;
loaders and global import functions inherit the package table. Reopening and
cached tables preserve this distinction, including after an initialization
error. Windows paths use the process executable directory. Environment paths
preserve empty values and apply Lua's `;;` and internal-marker expansion.

The [Windows debug](2026-10-28-rust-lua-package-init-focused.json),
[Windows release](2026-10-28-rust-lua-package-init-current.json) and
[i686 SSE2](2026-10-28-rust-lua-package-init-i686-current.json) comparisons each
match release and assertion-enabled Lua 5.1.5. Each profile compares 28 transcript
rows in twelve opening orders and nine environment settings, for 3,024 rows.
The public-header fixture checks:

- Package initialization before, between and after the other available libraries.
- Full C windows, registry/table identity and closure environments.
- Preloaded imports, cache reuse, nil results, module options and collection.
- Native `loadlib`, C imports, C-root searchers and their function environments.
- Empty search paths and the complete missing-module error suffix.
- Reopening, cached alternate tables, registration setters, partial errors and
  global name conflicts.
- Initialization on yielded and waiting coroutines, preservation of their
  existing stacks and successful later resumption.

The environment settings cover absent and empty variables, literal paths,
fallback markers, repeated separators, executable markers, internal markers,
and independent `LUA_PATH`/`LUA_CPATH` overrides. Expected Windows directory
substitution is checked against each process's actual directory; the fixture
does not compare unrelated executable directories as literal equal paths.

Selected-thread package calls require an existing C activation because the
initializer writes `LUA_ENVIRONINDEX`. The fixture uses suspended `yield` and
waiting `resume` activations. Fresh/dead threads without a C activation are not
counted as valid direct calls. The ordinary Rust binding invokes the entrypoint
through a C activation.

The [baseline](2026-10-28-rust-lua-package-init-before.json) preserves the CLI
and C SDK from `2a612339d`, where `luaopen_package` was absent. The
[i686 verification](2026-10-28-rust-lua-package-init-i686-verification.json)
checks current artifacts and sources, ELF32 width, SSE2 flags, checked assertions
and all seven standard-library exports. Successful Rust comparison processes
assert balanced dispatches, states, groups and library lifetimes.

[Full validation](2026-10-28-rust-lua-package-init-validation.json) passes 810
tests on Windows x64 and 811 on Linux x64 and i686 SSE2. Strict Clippy, formatting,
checks without default features and the empty normal/build dependency tree pass.
The earlier six-initializer corpus remains in the Rust integration test.
The [library audit](2026-10-28-rust-lua-package-init-stdlib.json) matches all
5,507 cases, and the [combined native audit](2026-10-28-rust-lua-package-init-boundary-current.json)
rechecks the preceding native corpora and cleanup guards.

Both native-enabled i686 CLIs pass unchanged `main.lua` and `all.lua` in the
[full stock suite](2026-10-28-rust-lua-package-init-i686-suite.json); T/testC
remains absent. [Windows independent files](2026-10-28-rust-lua-package-init-windows-suite.json)
retain 17/18 passes in all four modes, including the shared `files.lua:59`
rename failure. The [evidence manifest](2026-10-28-rust-lua-package-init-evidence.json)
binds current sources, reports, binaries, modules and logs.

Full Lua 5.1 parity remains open. `luaopen_io` and `luaL_openlibs` are still
missing. I/O initialization also needs the direct C-function close-handler
path; the current Rust close path recognizes its built-in handlers. Core C
declaration coverage remains 76/80, with `lua_close`, `lua_getallocf`,
`lua_newstate` and `lua_setallocf` absent. Exact allocation/heap accounting,
collector work, broader strict continuations, reader/body/API interleavings,
diagnostics and resource/OOM behavior remain unfinished.
