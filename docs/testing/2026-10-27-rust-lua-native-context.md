# Native API calling contexts

External native hook pseudo-indexes now address the interrupted activation.
`LUA_ENVIRONINDEX` reads/replaces that function's environment; C upvalue indexes
read and mutate its actual captures. The Rust hook's own captures and private
binding remain available through `HostCall`, independently of the C API view.

Previously, these operations used the hook registration's closure. A hook could
change its own environment or fail to find captures while leaving its target
unchanged. The retained [before comparison](2026-10-27-rust-lua-native-context-before.json)
reproduces a native target returning 17 instead of 29, Lua/coroutine targets
returning 17 instead of 42, and incorrect allocation and parent-environment
identities.

Calling-environment lookup now skips internal scheduler continuations. Userdata
and native closures created by a hook inherit the interrupted function's
environment. Waiting and hook-suspended threads expose the same visible call
identity through `NativeThreadStack`, including environment replacement on Lua
functions and C capture writes. A fresh thread reports `no calling environment`
when asked to replace a calling environment; its thread globals remain unchanged.

The [source-built C comparison](2026-10-27-rust-lua-native-context-boundary-current.json)
uses the same module against official Lua 5.1.5 and the Rust engine. It checks
call/return hooks on native and Lua targets, distinct registration/function/thread
environments, userdata and closure environments, capture bounds, active writes,
and child-coroutine reads/writes to a waiting hook's real context. Collection
runs in the child and after delivery. All 595 stdout bytes match, all 555 Rust
dispatch/drop guards balance under `panic=abort`, and all six C states are freed
(five before shutdown). The earlier boundary, parser-root, thread-stack,
coroutine, call-level, debug and hook probes also pass.

Five new portable Rust groups cover the same context distinctions, mutable native
captures, suspended Lua environments, allocation inheritance and fresh-thread
failure. The [Windows validation](2026-10-27-rust-lua-native-context-validation-current.json)
records 724 passing tests, strict Clippy, formatting and an empty normal/build
dependency tree. [Linux validation](2026-10-27-rust-lua-native-context-linux-validation.json)
records 725 passing tests and strict Clippy on both x64 and i686 SSE2. Reports
bind the current sources, commands and log digests.

The unchanged [i686 SSE2 stock suite](2026-10-27-rust-lua-native-context-i686-suite.json)
passes `main.lua` and `all.lua` in both actual CLIs, with verified ELF32/chunk
ABIs. Its stock mode still omits T/testC and native suite libraries. The
[Windows independent probes](2026-10-27-rust-lua-native-context-windows-suite.json)
retain 17/18 passes in reference, direct Rust, cold-cache and cache-hit modes;
the common `files.lua:59` open-file rename failure keeps that runner unsuccessful.

This advances existing C API semantics without adding declarations. Six core
declarations, broader C API/stack/resource/OOM behavior, production native loading, the retained
lowered-call-count yield mismatch and full standalone Lua 5.1 parity remain open.
Performance tuning remains deferred.
