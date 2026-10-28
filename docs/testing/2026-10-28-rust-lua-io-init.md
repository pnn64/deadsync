# Lua 5.1 I/O and full-library initialization

`luaopen_io` and `luaL_openlibs` now run through the bundled C API image.
The C code constructs tables, closures, environments and userdata through the
public Lua 5.1 API; library function bodies continue to execute in Rust.
`NativeCAbiLoader::standard_library(b"io")` resolves the I/O opener, and
`open_libraries()` supplies a callable wrapper for the void `luaL_openlibs` API.
Default `Vm::new()` initialization order is preserved.

I/O construction follows PUC's order: register the file metatable, replace the
calling C closure's environment, register the library, create standard userdata,
then give `popen` its separate environment. Standard userdata has a pointer-sized
payload containing the boundary image's standard C stream pointer. A private
bridge operation attaches the corresponding Rust stream resource after the
public allocation and metatable operations. Fresh userdata from repeated opens
shares that resource and its unread buffer. The collector roots the three
process streams, and bare-state file loading lazily obtains the same stdin.

`luaL_openlibs` calls base, package, table, I/O, OS, string, math and debug in
separate C activations. Package and I/O environment replacement stays within
their activations. The caller's existing C stack values remain intact.

The [baseline image](2026-10-28-rust-lua-io-init-baseline.json), preserved from
`0a2d9c39a`, lacks both exports. The new public-header C fixture and bare Rust
embedding compare against pinned release and checked Lua 5.1.5 references.
The [evidence manifest](2026-10-28-rust-lua-io-init-evidence.json) verifies source,
binary, module, report and log hashes and embeds the private audit tools.

| Corpus | Observations per run | Runs per profile | Observations per profile |
| --- | ---: | ---: | ---: |
| I/O initialization | 36 | 108 | 3,888 |
| Full initialization | 22 | 108 | 2,376 |
| Shared buffered stdin | 3 | 12 | 36 |
| Stdin loading before I/O | 1 | 12 | 12 |
| Total | | 240 | 6,312 |

The initialization corpora use nine controlled package-path environments and
twelve trials per environment. The I/O trials rotate the other seven libraries;
full initialization always uses the standard order. The two input corpora use
one environment and process-supplied input. Across Windows debug, Windows release
and Linux i686 SSE2 release, 720 Rust processes match 18,936 observations against
both reference builds. Each Rust process asserts balanced operations, state
groups, states and loaded libraries after dropping its VM.

The I/O corpus checks complete C windows, registration order, file/library table
reuse, fresh closures, shared and separate environments, standard payloads,
no-close handlers, file reads/writes/lines, reopening, registration callbacks
that collect or fail, files allocated during registration, and initialization
on yielded or waiting C activations. The full initializer corpus checks empty
states, stack arguments, library registration, closure environments, aliases,
file/import bodies, repeated opening under custom caller environments and
partial failures at each later library. Input probes interleave reads through
old and new stdin userdata, collect discarded wrappers, and load stdin before
opening I/O.

The full comparison reports are retained for I/O on
[Windows debug](2026-10-28-rust-lua-io-init-focused.json),
[Windows release](2026-10-28-rust-lua-io-init-current.json) and
[i686 release](2026-10-28-rust-lua-io-init-i686-current.json), and full
initialization on [Windows debug](2026-10-28-rust-lua-openlibs-focused.json),
[Windows release](2026-10-28-rust-lua-openlibs-current.json) and
[i686 release](2026-10-28-rust-lua-openlibs-i686-current.json).
The [i686 verifier](2026-10-28-rust-lua-io-init-i686-verification.json) checks
artifact/source hashes, ELF32 width and checked-reference assertion recipes.

[Full validation](2026-10-28-rust-lua-io-init-validation.json) passes 813 tests
on Windows x64 and 814 on Linux x64 and i686 SSE2, strict Clippy, formatting,
all-target checks without default features and an empty normal/build dependency
tree. [Eleven portable tests](2026-10-28-rust-lua-io-init-portable.json) execute
without the default C feature. The [library audit](2026-10-28-rust-lua-io-init-stdlib.json)
matches 5,507 argument cases. The
[native audit](2026-10-28-rust-lua-io-init-boundary-current.json) balances 1,229,274
operations, 19,219 states and 52 libraries across 42 successful Rust processes;
25 expected terminating processes are counted separately. Library entrypoint
declaration coverage is 8/8 with `luaL_openlibs` declared; core coverage remains
76/80.

Both native-enabled i686 CLIs pass unchanged `main.lua` and `all.lua` in the
[stock suite](2026-10-28-rust-lua-io-init-i686-suite.json); T/testC remains absent.
[Windows](2026-10-28-rust-lua-io-init-windows-suite.json) retains 17/18 independent
passes in all four modes and the common `files.lua:59` failure.

Full Lua 5.1 parity remains unfinished. The
[strict hook-yield gap](2026-10-28-rust-lua-io-closers-hook-yield.json) remains
recorded separately. General native `FILE *` ownership and C stdio/Rust buffer
interoperation, arbitrary file userdata created by external C code, the four
core lifecycle/allocator APIs, exact heap/allocation and collector accounting,
broader continuations, diagnostics and resource/OOM behavior are outside this
milestone's completed proof.
