# Native thread identities and retained execution statuses

The safe NativeStack API now converts indexed thread values, pushes the current
thread with its main-thread flag, reads NativeThreadStatus, and attaches typed
host resources to any live thread. These operations do not resume or otherwise
activate the target. HostCall's current-thread state interface is preserved.
Collected IDs return None for lookups; initialization rejects stale IDs,
duplicate type slots and invalid Lua roots, dropping rejected resources normally.

The VM retains execution status separately from coroutine.status. Fresh,
running, normal and successfully completed threads report 0. Yielded threads
report 1 until resumption starts, and failed coroutines retain 2. A failed attempt
to resume an already dead thread preserves its prior execution status. Fatal
embedding quota cancellation records runtime-error status for aborted children;
the main thread remains usable. This does not implement allocator/OOM statuses.

The development C boundary adds lua_tothread, lua_pushthread and lua_status.
C states are allocated lazily even for coroutines that have never entered a C
callback. A private lookup maps stable C pointers to typed thread identities;
it contains no collector roots and removes an entry before freeing its C state.
The thread's owned host resource retains the allocating DLL. Existing state
initialization in native callbacks uses this same path, so an early pointer
query and later native execution observe the same state address. VM moves and
growth of the thread arena do not relocate these C allocations.

Readonly lua_status queries on inactive states route through the current C
callback's checked context. Other inactive-state stack operations remain
unsupported. No live mutable context is retained in an idle C state. Lookup
locks are released before entering Lua or destroying a state. The adapter
asserts that all lookup entries disappear before unloading its DLL owners.

The identical Windows/MSVC C fixture checks main/current flags, native stack
effects, pointers before and after first entry, repeated yields and completion,
failed and protected errors, failed resume attempts, captured pseudo-indexes,
non-thread/NULL conversions and the normal parent's status inside a child.
One hundred live never-resumed coroutines have distinct stable pointers.
Independent weak checks verify that a pointer lookup does not retain a thread
while an actual Lua capture does. Every new C state is released once.

Six Rust groups cover statuses, native/pseudo-index conversion, lazy inactive
resource roots and self cycles, stable resource storage through a VM move and
600 new threads, independent typed slots, duplicate/stale-ID rejection with
Drop accounting, and fatal child-quota cancellation with main-thread recovery.
Lua roots in newly attached resources use the existing validation, tracing and
collector barriers. No new unsafe code enters the engine; DLL allocation and
pointer translation remain in the development boundary.

Fresh verification passes 674 tests on Windows x64 and 675 on each of Linux x64
and Linux i686 SSE2: 77 unit, 4 documentation and 593/594 integration tests.
Rustfmt, strict Clippy and the zero normal/build dependency tree pass. The C
fixture emits 60,987 identical stdout bytes, with 115,169 balanced operation/drop
guards. All 182 C states are destroyed, including 181 collected before VM
shutdown. The Rust executable uses panic=abort; C error jumps remain within C.
Eighteen imported-name chunks match reference, direct Rust, cache-store and
fresh-process cache-hit modes on each platform. Unchanged stock main.lua and
all.lua pass in both CLIs on Linux i686 SSE2. Internal T/testC and suite native
modules remain absent.

Raw [C-boundary](2026-10-27-rust-lua-native-thread-boundary.json),
[Windows](2026-10-27-rust-lua-native-thread-windows.json),
[Linux](2026-10-27-rust-lua-native-thread-linux.json),
[i686](2026-10-27-rust-lua-native-thread-i686.json) and
[stock-suite](2026-10-27-rust-lua-native-thread-suite.json) reports retain commands
and observations. [Validation metadata](2026-10-27-rust-lua-native-thread-validation.json)
binds exact source snapshots, workspace inputs, logs, ABI headers and private
executables. Final targets start absent and build from source. A concurrent
Cargo.lock update only adds an edge from the unrelated FLAC crate to Ogg.
Validation retains both lock hashes and proves the Lua dependency records and
selected feature trees identical on all three platforms. Compiler build identity
does not include Cargo.lock.

The header now declares 60 of 80 core APIs; declarations do not establish full
semantics. Inactive stack access, C-level coroutine creation/resume/yield,
standalone state lifecycle, allocator callbacks/OOM status and remaining C APIs
are unfinished. The independent parser scratch probe still differs (PUC 1,3,
Rust 1,1). Production native loading, full standalone Lua 5.1 parity, later
dialects and integrations remain open. Optimization stays deferred and the
legacy engine remains in production.
