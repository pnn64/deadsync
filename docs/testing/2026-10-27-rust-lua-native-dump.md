# Native Lua 5.1 dump writer callbacks

`NativeStack::dump` and development `lua_dump` serialize the Lua function on top
of the current native stack. Non-Lua values return 1 without invoking a writer.
The first nonzero writer status stops callbacks and returns unchanged, including
negative statuses. The API does not insert a visible Lua/native dump frame or
change the caller's stack. Writers can inspect or mutate caller slots, collect,
invoke Lua, resume child coroutines and dump recursively.

The existing serializer now optionally records the endpoints of Lua 5.1 writer
blocks. Headers, scalar fields, vectors and terminated strings follow the
reference's boundaries. Empty debug vectors still invoke the writer; the C
adapter passes NULL for their empty storage. Ordinary string.dump uses the same
serializer without recording a block plan. Imported native code retains its
original words, constant ordering and debug metadata. Source functions use this
engine's native lowering; their native words are not claimed byte-identical to
the official compiler's output.

Serialization prepares owned bytes and block endpoints before invoking writers.
Each callback borrows its block only until return and uses the original native
window. Callback errors escape unprotected with their arbitrary Lua value and
fault frames. Ignoring a failed child-call or dump Result cannot suppress the
pending error. Instruction limits escape through the embedding's fatal quota
path, drop the serialization buffers and permit the next VM entry. The C-only
writer boundary returns through Rust dispatch before any C error jump; its
borrowed context and DLL owner remain live until return.

The shared Windows/MSVC fixture compares eleven imported functions. Nine have
original, empty or NUL-containing upvalue names with different name counts. Two
more include stripped locations and valid unused constants covering nil,
booleans, binary strings, negative zero and a NaN payload. Both engines load
identical official chunks. Writer block sizes and complete bytes match. Every
one of the constants fixture's 98 writer boundaries is also tested as the first
failure point. Signed status extremes, NULL empty-vector storage, non-Lua values,
a NULL unused writer, nested calls, collection, coroutine resumes, caller-slot
mutation and unprotected errors are checked. Source dumps round-trip through
the engine's loader while a replaced string.dump global remains unused.

Six new Rust groups compare blocks directly against the official C writer,
including valid chunks with empty line vectors; check every signed failure
boundary; exercise nested dumping, collection and caller mutation; retain fault
frames and sticky errors; verify fatal quota recovery; and reject non-Lua
values without a callback. The reference recorder only copies borrowed bytes
and never calls or raises into Lua. Lua 5.1's C API has no strip argument, so the
empty-vector fixture removes only line information before loading it through
both engines.

Fresh verification passes 668 tests on Windows x64 and 669 on each of Linux x64
and Linux i686 SSE2: 77 unit, 4 documentation and 587/588 integration tests.
Rustfmt, strict Clippy and the zero normal/build dependency tree pass. The C
fixture emits 60,907 identical stdout bytes; 114,187 operation/drop guards
balance, and all 77 C states are destroyed, including 76 collected before VM
shutdown. The Rust executable uses panic=abort. Eighteen imported-name chunks
also match in reference, direct Rust, cache-store and fresh-process cache-hit
modes on each platform. Unchanged stock main.lua and all.lua pass in both CLIs
on Linux i686 SSE2; internal T/testC and suite native modules remain absent.

Raw [C-boundary](2026-10-27-rust-lua-native-dump-boundary.json),
[Windows](2026-10-27-rust-lua-native-dump-windows.json),
[Linux](2026-10-27-rust-lua-native-dump-linux.json),
[i686](2026-10-27-rust-lua-native-dump-i686.json) and
[stock-suite](2026-10-27-rust-lua-native-dump-suite.json) reports retain commands
and observations. [Validation metadata](2026-10-27-rust-lua-native-dump-validation.json)
binds the source snapshot, workspace inputs, fixture bytes, ABI headers, logs
and private executables. Every final build target starts absent. A concurrent
Cargo.lock change adds a dependency only to the unrelated FLAC crate. Validation
retains both lock hashes, proves the Lua dependency records unchanged, and
compares identical selected-package dependency/feature trees on all three
platforms. The compiler build identity does not include Cargo.lock.

This adds one C declaration (57 of 80); declaration counts do not prove complete
semantics. The independent C-reader scratch probe still differs (PUC 1,3 versus
Rust 1,1). Allocator/OOM behavior, source export/resource limits, remaining C
APIs, production native loading, full standalone Lua 5.1 parity, later dialects
and integrations remain unfinished. Optimization and benchmarks stay deferred;
the legacy engine remains in production.
