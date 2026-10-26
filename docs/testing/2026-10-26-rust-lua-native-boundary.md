# Native C call/error boundary proof

The development boundary runs the same C module source against official Lua
5.1.5 and the Rust engine. Complete program stdout and successful exit status
match. The Rust runner uses the unchanged release `panic=abort` policy. This
establishes a working error-transfer mechanism for future native-module support;
production `package.loadlib` and the full C API remain unfinished.

The [C shim](../../crates/deadlib-lua/tests/native-boundary/boundary.c) owns
`setjmp` and `longjmp`. Each API operation dispatches into Rust and receives a
status normally. If Rust returns a Lua error, its dispatcher and local values
finish cleanup before the shim jumps to the C invocation boundary. No jump
crosses an active Rust frame. Nested Lua/C/Lua errors follow the same sequence
at each boundary, preserving the original Lua error object.

The [Rust runner](../../crates/deadlib-lua/examples/native_boundary.rs) uses the
existing host-call API, roots its temporary stack in host captures, and appends
returned values through the ordinary native-result path. Drop guards observe
all 1,853 Rust dispatches returning, including the error paths. The Rust process
reports `panic_abort=true`; error recovery does not rely on panic unwinding.

The [shared module](../../crates/deadlib-lua/tests/native-boundary/module.c)
uses ordinary Lua 5.1 C function signatures. The
[Lua cases](../../crates/deadlib-lua/tests/native-boundary/cases.lua) check:

- Numeric-string arguments, native double values, and C stack/result counts.
- Object identity, explicit nils, false values and binary strings.
- Five hundred repeated native errors, including functions, threads and userdata.
- Nil errors, xpcall identity, and one hundred nested Lua/C/Lua error round trips.
- Collection while strings and newly returned tables exist only on the C stack.
- Empty and multiple results, yield rejection across C calls, and later recovery.

All 555 existing Rust tests pass. Formatting, strict Clippy over all targets and
the normal/build dependency check pass; the latter still contains only
deadlib-lua. The interpreter/compiler sources, package loader, production
dependencies and release policy are unchanged. The shim is compiled only by the
development probe. No C Lua implementation executes the Rust cases.

The [audit](2026-10-26-rust-lua-native-boundary-audit.json) records complete
process observations, source hashes, build commands, DLLs and executables. The
reference CLI and reference C module both link the same official `lua51.dll`.
A static Lua CLI must not pass its state into a second Lua DLL: internal
singleton identities, including table dummy nodes, belong to one Lua image.
The official archive remains checksum-pinned and its source files are unchanged.
The Rust module links the small test boundary DLL instead.

[Validation metadata](2026-10-26-rust-lua-native-boundary-validation.json)
pins the isolated build, source map, artifact hashes and check logs. Reproduce
on Windows/MSVC x64 with:

```sh
python scripts/check_rust_lua_native_boundary.py --output target/native-boundary.json
```

This is a development proof with five fixed fixture callbacks, 58 rooted stack
slots, invocation-borrowed state pointers and a small API subset. Its number
checks address unchanged original arguments. Host captures are visible as host
upvalues, so they cannot serve as the production C stack representation.
Persistent thread states, full stack/API semantics, C closure/userdata/thread
operations, library lifetime handling and package loader integration remain work.
The previous library audit's fourteen native-loader failures remain open.

The boundary follows the [Lua 5.1 C API contract](https://www.lua.org/manual/5.1/manual.html#3)
and avoids depending on [Rust panic unwinding across FFI](https://doc.rust-lang.org/nomicon/ffi.html#ffi-and-unwinding).
No performance benchmarks ran. Full standalone Lua 5.1, later dialects and
gameplay integration remain unfinished.
