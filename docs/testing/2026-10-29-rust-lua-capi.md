# Standalone Rust Lua 5.1 C library

The `deadlib-lua-capi` workspace package produces a shared library containing the
Rust interpreter and its C boundary. A pure C application can call
`luaL_newstate`, initialize libraries, execute Lua, load C modules and call
`lua_close` without a Rust bootstrap or Rust-facing VM. The unchanged official
`lua.c` CLI also links against this library. No C Lua interpreter is linked.

`scripts/build_rust_lua_c_api.py` stages public headers, `lua51.dll` and its MSVC
import library on Windows, or `liblua5.1.so.0` and its development symlink on
Linux. It adds pkg-config metadata and a manifest binding the sources and
payload. The engine's normal/build Rust dependencies remain empty; the C API
package depends only on the engine.

Public entry points use generated naked jumps to the C implementation. Variadic
arguments and return registers pass through unchanged. Error jumps therefore
cross no Rust forwarding frames. The C implementation's definitions have private
names, while its calls and stored function pointers use the public entries. This
preserves pointer identity when a hook inspects standard-library initializers.
Linux binds internal function references locally and exposes the conventional
`liblua5.1.so.0` SONAME. Each owned state retains the image containing its engine.

The public-header C host checks initializer identities, function-pointer round
trips, variadic formatting, a separately built native module, independent states,
parser readers/writers, binary reload, recursion limits, hooks, coroutines,
close-time finalizers, weak roots and panic recovery. It compiles and links only
C client sources against the staged library and headers. Separate default and
returning panic probes retain their expected status-1 termination.

All 97 observations match unmodified release and assertion-enabled Lua 5.1.5
shared libraries on [Windows debug](2026-10-29-rust-lua-capi-debug.json),
[Windows release](2026-10-29-rust-lua-capi-release.json) and
[Linux i686 SSE2 release](2026-10-29-rust-lua-capi-i686.json): 291 three-engine
observations plus six expected Rust panic terminations. Normal C-host runs
balance six states, five groups, one native library and every dispatcher return.

Each profile also compiles the original `lua.c` unchanged against the staged
headers, runs a Lua program through that C CLI, and loads the test C module.
The i686 gate runs unchanged `main.lua` and `all.lua`, with all four official
native libraries, on release PUC, checked PUC and the Rust shared library.
All six stock-suite runs pass; T/testC remain absent.

The machine-code checks inspect all 133 public/protocol forwarders per profile.
Windows export addresses jump to the corresponding private C addresses in the
package's preserved linker map. Linux disassembly targets the matching hidden C
definitions. Private implementation symbols stay out of the dynamic export
table. The [i686 verifier](2026-10-29-rust-lua-capi-i686-verification.json) checks
ELF32 binaries, source/artifact hashes, SSE2 flags, assertions, native modules,
the stock suite, SONAME and absence of an external Lua library dependency.
The evidence verifier also inspects the Windows runtime's import table.

The normal C API integration test builds its own shared library in an isolated
target directory, so it works from a fresh target. It compiles a C executable and
module, compares the full transcript, checks cleanup and runs both panic probes.
[Full validation](2026-10-29-rust-lua-capi-validation.json) passes 832 Windows
tests and 833 tests on each Linux target, strict Clippy, formatting and all-target
checks without default features. The separate
[portable run](2026-10-29-rust-lua-capi-portable.json) passes 35 tests.

The [library audit](2026-10-29-rust-lua-capi-stdlib.json) matches 5,507 cases.
The [native audit](2026-10-29-rust-lua-capi-boundary-current.json) retains 42
normal and 25 expected termination probes with balanced lifetimes.
[Windows](2026-10-29-rust-lua-capi-windows-suite.json) retains 17/18 independent
passes in all four existing runner modes, with the common `files.lua:59` failure.
The [evidence manifest](2026-10-29-rust-lua-capi-evidence.json) binds the final
reports, sources, binaries and logs and embeds the proof tools.

Core declarations remain 77/80. `lua_newstate`, `lua_getallocf` and `lua_setallocf`
are absent; default states use Rust's allocator. Custom allocator ownership,
recoverable OOM, exact heap/GC accounting and general continuation/resource
behavior remain open. Deep C callback probes reserve 32 MiB of host stack,
matching existing Rust runners. Equivalent behavior under PUC's smaller/default
host stack limits is unproven. Full Lua 5.1 parity is not established.
