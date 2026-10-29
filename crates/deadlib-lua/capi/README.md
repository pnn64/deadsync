# Standalone Lua 5.1 C library

This package builds `lua51.dll` on Windows or `liblua5.1.so.0` on Linux, with the
Rust interpreter inside the shared library. A C application can call
`luaL_newstate`, use Lua's public C API and call `lua_close` without a Rust
bootstrap or Rust-facing VM. No C Lua interpreter is linked.

From the repository root:

```text
python scripts/build_rust_lua_c_api.py --output target/lua-capi/dist
```

The output contains public headers in `include`, the runtime in `bin` (Windows)
or `lib` (Linux), an MSVC import library or Linux development symlink, and
`lib/pkgconfig/lua5.1.pc`. `manifest.json` binds the sources and payload hashes.
`--debug`, `--target` and `--target-dir` select other build configurations.
The build requires Rust and the target's C compiler; the C application does not
need a Rust toolchain at runtime.

A small C client:

```c
#include <stdio.h>
#include "lua.h"
#include "lauxlib.h"
#include "lualib.h"

int main(void) {
  lua_State *L = luaL_newstate();
  if (!L) return 1;
  luaL_openlibs(L);
  if (luaL_dostring(L, "return 6 * 7")) {
    fprintf(stderr, "%s\n", lua_tostring(L, -1));
    lua_close(L);
    return 1;
  }
  printf("%g\n", lua_tonumber(L, -1));
  lua_close(L);
  return 0;
}
```

On Windows, compile with `LUA_BUILD_AS_DLL`, include the staged headers, link
`lib/lua51.lib`, and keep `bin/lua51.dll` beside the executable or on its DLL
search path. On Linux, use the staged pkg-config file or link `-llua5.1` with
the staged include/library paths and an appropriate runtime search path.

The C entry points jump directly into the C boundary. They add no Rust forwarding
frames, including for variadic APIs, so Lua error jumps stay within C. Internal
calls and stored C function pointers use those same public entries. Separate C
modules load through the same library. Calls on one state group must be
serialized; callbacks and their code must remain live while Lua can call them.

This is partial Lua 5.1 parity: 77 of 80 core functions are declared.
`lua_newstate`, `lua_getallocf` and `lua_setallocf` remain absent. Default states
use Rust's allocator. Fallible userdata payload failures now return `LUA_ERRMEM`
and bypass error handlers, including through parser readers and coroutines.
The fixed memory diagnostic survives collection. Custom allocator ownership and
recoverable failures in other VM allocations remain unfinished. Exact heap/GC accounting and general continuation/resource
parity also remain open. The deep C callback tests reserve 32 MiB of host stack,
as the existing Rust runners do; default host-stack limits are not proven equal
to PUC Lua's resource bounds.

Supported build targets are Windows MSVC and Linux GNU. The current evidence
covers Windows x64 debug/release and Linux i686 SSE2 release, with Cargo tests
also running on Linux x64. See the
[standalone C checks](../../../docs/testing/2026-10-29-rust-lua-capi.md).

See the [allocation-error checks](../../../docs/testing/2026-10-29-rust-lua-memory-errors.md)
for the tested failure paths and remaining allocation limits.
