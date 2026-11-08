# Lua 5.1 native file userdata

C-created `FILE **` userdata now works with the Rust I/O bodies. A zero pointer
is a closed file; a live pointer attaches a stdio resource from the active C API
image. The payload remains authoritative when native code closes, replaces or
transfers the pointer. The adapter retains its C image while the resource lives.
Lua execution and library control flow remain in Rust; the C helper performs
stdio operations without decoding a `FILE` layout in Rust.

Native reads, line input, numeric scanning, writes, seeks, buffering and flushes
use the owning C runtime's buffers and error indicators. C and Lua reads can
alternate without an extra Rust prefetch buffer hiding unread bytes. Line input
retains `fgets`/`strlen` handling of embedded NULs and defers stream errors until
the line operation ends. Numeric input uses that runtime's `fscanf`.

Lua close handlers clear the `FILE **` payload after closing, including failure
results. Borrowed Rust records do not independently close the pointer when
collected. Process exit leaves native stream flushing to C's own stream table;
a cached record may refer to a handle that C has already closed. A reachable
native file also remains in the successful corpus until VM shutdown, exercising
its finalizer and cleanup guards.

The [previous milestone](2026-10-28-rust-lua-native-files-baseline.json),
`870863f11`, returns nil for `io.type` on a C-created closed file userdata.
The current portable Rust regression checks closed-file classification, strings,
method errors and finalization with the default C feature disabled.

The public-header C fixture is linked into each tested API image so its file
factory and stdio operations use one CRT. This matters for Windows builds using
static CRT linkage. Reference executables link the unmodified pinned PUC
5.1.5 sources with that fixture. Checked references force-enable assertions.
Rust's fixture image links the current jump boundary and the same public-header
fixture; its interpreter and function bodies come from the Rust engine.

The [Windows debug](2026-10-28-rust-lua-native-files-focused.json),
[Windows release](2026-10-28-rust-lua-native-files-current.json) and
[Linux i686 SSE2 release](2026-10-28-rust-lua-native-files-i686-current.json)
comparisons each match 47 observations against release and checked references:

- Closed userdata and all file-method closed-handle errors.
- Alternating C/Lua reads, numeric reads, writes, seeks and iterators under
  unbuffered, full and line buffering.
- Pointer replacement, native closure and reopening, ownership transfer,
  native close handlers and explicit finalization.
- Partial numeric tokens, infinities, NaNs, hexadecimal input and embedded NULs.
- Read-only errors, an iterator's retained error state and subsequent recovery.
- Large-file positions and successful closure of pipes with zero or nonzero
  child exit status.

Each profile also runs a separate process-exit case on all three engines. It
closes a file through C, leaves another file buffered, writes an output marker
and exits with status 7. Those nine terminating processes are separate from the
successful cleanup-counted comparisons.

Two development regressions remain explicit. The
[Windows pipe probe](2026-10-28-rust-lua-native-files-edges-before.json) rejected
the child exit status 7 even though PUC treats any completed `pclose` as success.
The [i686 position probe](2026-10-28-rust-lua-native-files-i686-edges-before.json)
converted `ftell`'s `-1` into an unsigned value of about `1.84e19`. The adapter now
checks `pclose != -1` and preserves signed positions. The failed release Rust
probes abort at their assertion under the workspace's panic-abort policy; both
reference builds complete the corpus. Original executables are retained with
their reports. The checked Linux build also requires its feature macros before
the assertion header includes stdio.

[Full validation](2026-10-28-rust-lua-native-files-validation.json) passes 815
tests on Windows x64 and 816 on Linux x64 and i686 SSE2, strict Clippy,
formatting, all-target checks without default features and an empty normal/build
dependency tree. [Twelve portable tests](2026-10-28-rust-lua-native-files-portable.json)
run with the default C feature disabled. The
[library audit](2026-10-28-rust-lua-native-files-stdlib.json) matches 5,507 cases,
and the [native audit](2026-10-28-rust-lua-native-files-boundary-current.json)
retains its dispatch, state and library cleanup checks.

Both native-enabled i686 CLIs pass unchanged `main.lua` and `all.lua` in the
[stock suite](2026-10-28-rust-lua-native-files-i686-suite.json); T/testC remains
absent. [Windows](2026-10-28-rust-lua-native-files-windows-suite.json) retains
17/18 independent passes in all four modes and the common `files.lua:59` failure.
The [i686 verifier](2026-10-28-rust-lua-native-files-i686-verification.json) checks
ELF32 width, artifacts, source hashes, signed positions and assertion recipes.
The [evidence manifest](2026-10-28-rust-lua-native-files-evidence.json) binds
current sources, references, reports, binaries and logs and embeds the private
audit tools.

Full Lua 5.1 parity remains unfinished. Remaining work includes native payloads
for Rust-created files, shared C/Rust standard-stream buffering, the four core
lifecycle/allocator APIs, exact allocation/heap and collector accounting,
strict continuations, diagnostics and resource/OOM behavior. The separate
[hook-yield mismatch](2026-10-28-rust-lua-io-closers-hook-yield.json) remains
recorded. This milestone proves the listed C-created file cases.
