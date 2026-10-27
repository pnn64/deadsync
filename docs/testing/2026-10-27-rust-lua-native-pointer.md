# Stable native object pointer identities

Vm::object_pointer and the native stack's object_pointer/to_pointer methods now
provide stable opaque identities for live objects. The development C boundary
adds lua_topointer. Tables, Lua/native closures and threads receive lazy,
pointer-aligned tokens whose addresses survive arena growth and VM moves.
Full userdata returns its actual aligned payload address, including zero-length
payloads; light userdata returns its unmanaged native-width address. Other
scalars and strings return no object pointer. Identity queries do not coerce
values, invoke metamethods or change the native stack.

The token table stores generation-checked IDs without collector edges. It does
not retain Lua objects, and dead tokens are removed after arena sweeping.
Live tokens from different object kinds or simultaneous VMs have distinct
addresses. A collected address can be reused; callers must retain/pin the Lua
value if they need its identity to remain live. Heap accounting includes token
storage and the table's estimated bucket storage.

Default table/function/thread/userdata strings now use the same address and
native pointer formatting as the pointer APIs, replacing the previous formatted
arena IDs. MSVC uses its padded uppercase pointer representation; Linux/glibc
uses its native lowercase/NULL representation. Custom __tostring methods remain
in control of their returned text. The tests compare address relationships,
not raw addresses from unrelated processes.

C thread handles share their thread's VM-owned aligned token. The boundary
treats lua_State pointers as opaque keys and never dereferences an assumed VM
state layout. Live C-only error frames form a TLS chain that restores the
previous frame and state across invoke, reader and writer boundaries. Handle
release checks the entire live chain; the VM owns token storage, so C never
frees a Rust allocation. This also allows moving an idle VM between storage
locations without relocating its thread handles. The Rust pointer-to-thread
lookup remains non-owning and is cleared before its DLL owners are released.

The identical Windows/MSVC C fixture checks scalar/NULL rejection, stable
pointers through callbacks and collection, distinct live identities, default
string agreement, light-userdata round trips, actual userdata payload equality,
thread-state equality before/after native entry, __eq and __tostring isolation,
weak lifetimes and a thousand live tables. Existing nested C/reader/writer
error-boundary checks remain enabled. Five Rust groups check simultaneous VM
uniqueness and moves, indexed/captured queries, non-retention of objects and
owned native bindings, exact native formatting against official Lua using the
same pointer value, and userdata resurrection through nested collection. A
unit test repeatedly collects a thousand transient token owners and verifies
that only the live token survives without moving.

Fresh verification passes 680 tests on Windows x64 and 681 on each of Linux x64
and Linux i686 SSE2: 78 unit, 4 documentation and 598/599 integration tests.
Rustfmt, strict Clippy and the zero normal/build dependency tree pass. The C
fixture emits 61,019 identical stdout bytes, with 141,654 balanced operation/drop
guards. All 183 C thread handles are released, including 182 collected before
VM shutdown. The executable uses panic=abort; error jumps remain within C.
Eighteen imported-name chunks match reference, direct Rust, cache-store and
fresh-process cache-hit modes on each platform. Unchanged stock main.lua and
all.lua pass in both CLIs on Linux i686 SSE2; internal T/testC and suite native
modules remain absent.

Raw [C-boundary](2026-10-27-rust-lua-native-pointer-boundary.json),
[Windows](2026-10-27-rust-lua-native-pointer-windows.json),
[Linux](2026-10-27-rust-lua-native-pointer-linux.json),
[i686](2026-10-27-rust-lua-native-pointer-i686.json) and
[stock-suite](2026-10-27-rust-lua-native-pointer-suite.json) reports retain commands
and observations. [Validation metadata](2026-10-27-rust-lua-native-pointer-validation.json)
binds source snapshots, workspace inputs, logs, ABI headers and private
executables. Final build targets start absent and build from source.

The boundary now declares 61 of 80 core APIs; declarations do not establish
complete semantics. Inactive stack mutation, C coroutine execution APIs, state
lifecycle, allocator/OOM behavior and other C APIs remain unfinished. The
independent parser scratch probe still differs (PUC 1,3 versus Rust 1,1).
Complete standalone Lua 5.1 parity, later dialects and integrations remain open.
Optimization and benchmarks stay deferred; the legacy engine stays in production.
