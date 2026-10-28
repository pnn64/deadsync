# Default Windows/Linux C ABI adapter

`Vm::new` and the production `deadlua` CLI now load native Lua modules on Windows
and Linux with the default `native-c-api` feature. `package.loadlib`, C searchers
and C-root fallback use the existing registry-cached library ownership path.
Applications can still install another `NativeLibraryLoader`, disable future
opens or build with `--no-default-features` to omit the bundled C adapter.
This is partial C API support; full standalone Lua 5.1 parity remains open.

The runtime now owns the dispatcher previously exercised by the development
boundary. Its small C-only shim exports the implemented Lua API and performs
error/yield jumps entirely within C, after Rust dispatchers return. It contains
no C Lua interpreter. MSVC or the target C compiler builds the shim without
normal/build Cargo dependencies. The image is embedded into the Rust artifact;
the CLI does not need a separate file beside its executable.

The adapter opens lazily. It materializes the exact embedded bytes in a
per-image user cache, using an exclusive temporary file and atomic rename.
Existing images are checked byte-for-byte and the loaded protocol version is
validated. The artifact identity includes the compiled bytes, accommodating
different MSVC image timestamps. Windows cache paths use UTF-16; Lua-supplied
module paths retain the reference's `LoadLibraryA` behavior and error formatting.
Windows modules can link the generated `OUT_DIR/lua51.lib`. Linux loads
`liblua5.1.so.0` globally so ordinary unresolved Lua imports can bind to it.
Conflicting existing Lua API images are rejected.

Library objects retain their platform references through functions, hooks,
reader/writer callbacks and C-state destruction. Each VM normalizes replacement
providers to its main thread's C image; child-first calls initialize that owner
before creating a child token. Parallel VMs share the immutable image while
retaining separate C state tokens and thread-local active dispatch contexts.

The [first source-built default-adapter comparison](2026-10-28-rust-lua-default-native-boundary-first.json)
passes every strict observation. Its fixture module links the runtime-generated
import library, and the runner uses `Vm::new` without installing an adapter.
All twenty Rust processes exit successfully under `panic=abort`: 721,409
dispatches return, all 5,838 C states are destroyed, and all 30 native loader
references close. The [evidence manifest](2026-10-28-rust-lua-default-native-evidence.json)
records individual counters, source hashes, binary digests and report digests.

Both ordinary and copied-body native-library fixtures match all 1,767 stdout
bytes. Each destroys eight C states and balances six opens/closes. The extra
reference compared with the preceding development adapter is the helper module,
now loaded through `package.loadlib` during runner initialization. The copied
return/library/package body corpus still matches all 678 observations.

Three new integration groups compile an ordinary module against official public
`lua.h` and `lauxlib.h`, without private bridge declarations. They exercise the
actual CLI's `package.loadlib` and C-root `require`, arithmetic and C captures,
errors, direct C function bodies, Lua calls, coroutine yields, first use in a
child, provider replacement, userdata finalization and balanced cleanup. Eight
parallel VMs retain eight distinct C state identities while sharing one image.
These groups pass on Windows x64, Linux x64 and Linux i686 SSE2.

The [platform validation](2026-10-28-rust-lua-default-native-validation.json)
passes 758 tests on Windows and 759 on each Linux target. All three also pass
strict Clippy and the all-targets build with default features disabled. Windows
formatting passes, and the normal/build dependency tree contains only
`deadlib-lua`. The report checks unchanged sources and records exact log hashes.

The complete [5,507-case Windows library argument audit](2026-10-28-rust-lua-default-native-stdlib.json)
now matches, closing the fourteen previously retained native-loading argument
failures. This bounded matrix is not a proof of complete library semantics.

The [i686 SSE2 stock-suite report](2026-10-28-rust-lua-default-native-i686-suite.json)
passes unchanged `main.lua` and `all.lua` in both actual standalone CLIs. Each
isolated suite tree now builds the four unchanged official native libraries,
including the prefixed second-library copy. Neither `all.lua` run takes the
native-library skip. Executables and native chunks have verified ELF32/32-bit
ABIs. The reference CLI now uses `-Wl,-E` to export its C API, matching the
official Lua 5.1.5 Linux Makefile; arithmetic and overflow flags are unchanged.
The internal T/testC module is still absent. Native x64 full-suite limitations
remain recorded separately.

The [Windows independent probes](2026-10-28-rust-lua-default-native-windows-suite.json)
retain 17/18 passes in reference, Rust and both cache modes. All four retain the
`files.lua:59` open-file rename failure, so that strict runner still fails.

The earlier [captured search-name mismatch](2026-10-27-rust-lua-library-loading-first.json)
and its [paired replay audit](2026-10-27-rust-lua-library-loading-capture-audit.json)
remain retained. That input overwrites the only stack root while reference
`findfile` still holds a raw C string pointer, then collects. Its current match
does not establish stable parity for that input.

The C image still declares 75 of 80 core functions. `lua_atpanic`, `lua_close`,
`lua_getallocf`, `lua_newstate` and `lua_setallocf` remain missing. Complete
inactive-state execution, foreign payload/FILE pointer behavior, native resource
and OOM/error windows, parser-internal mutation, broader debug/hook/scratch-stack
behavior and the retained lowered-call-count yield mismatch remain open. Native
module constructors that reenter a saved Lua state are not covered. The explicit
`NativeCAbiLoader::from_library` constructor requires a trusted compatible image
and is unsafe. Selected passing modules and declaration counts do not close the
full Lua 5.1 parity gate. Performance work remains deferred.
