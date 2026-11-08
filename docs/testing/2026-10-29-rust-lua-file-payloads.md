# Lua 5.1 payloads for files created by Rust I/O bodies

With `native-c-api` enabled on Windows and Linux, files opened by the Rust
library bodies now contain real `FILE *` payloads. This covers `io.open`,
`io.tmpfile`, named `io.input` and `io.output`, named `io.lines`, and `io.popen`.
Their stream operations use the same C runtime from creation, preserving unread
input and pending output when C first accesses the payload. The existing Rust
stream implementation remains available without the C feature.

The VM selects the configured native C adapter before its first file open.
`NativeLibraryLoader::as_any` has a default implementation returning None;
the C adapter exposes itself and wrappers can forward that identity. A custom
adapter must be selected before file creation or C calls. Once resources use an
image, entering another image is rejected with the existing image-mixing error.
The integration regression verifies that the original file remains usable after
that rejection.

Native factories return a guarded handle. Publishing its pointer in Lua userdata
transfers close responsibility to the Lua close handler; a handle abandoned
before publication closes through its factory's runtime. Published userdata
retains the previous milestone's authoritative-payload behavior when C closes,
replaces or transfers its pointer. The C helper performs stdio operations;
library control flow and Lua execution remain in Rust.

The [preserved previous milestone](2026-10-28-rust-lua-file-payloads-baseline.json),
`45d986858`, exposes a NULL payload for a live `io.tmpfile()` result. The new
constructor corpus uses Rust's ordinary library initialization. It opens,
writes and partially reads its first file before any C API activation, with a
counter assertion that no boundary state has been created. The public-header C
fixture then reads the remaining bytes directly from that pointer.

Each profile matches 26 constructor observations against release and checked
Lua 5.1.5 references. The cases cover:

- Files created before entering the C API.
- Temporary, text, binary, named input and named output files under three
  buffering modes.
- Alternating C/Lua reads, numeric reads and writes using the same pointer.
- Ownership transfer and closure through either side.
- Pending output that remains invisible to another reader until a seek flushes
  it, automatic line-iterator closure and failed opens.
- Pipes whose child exits successfully or with status 7, and a file retained
  until VM shutdown.

The comparisons are retained for
[Windows debug](2026-10-28-rust-lua-file-payloads-focused.json),
[Windows release](2026-10-28-rust-lua-file-payloads-current.json) and
[Linux i686 SSE2 release](2026-10-28-rust-lua-file-payloads-i686-current.json).
The prior 47-observation native-file corpus also matches on
[Windows debug](2026-10-28-rust-lua-file-payloads-native-focused.json),
[Windows release](2026-10-28-rust-lua-file-payloads-native-current.json) and
[i686 release](2026-10-28-rust-lua-file-payloads-native-i686-current.json),
including signed `ftell` failure on a sparse large file. Together these are 219
differential observations. Both corpora also run separate process-exit probes
on all three engines, for 18 terminating processes with matching output and
exit status 7. The C fixture is linked into each image to preserve CRT ownership
on Windows; pinned PUC sources are unchanged, and checked builds force-enable
assertions.

[Full validation](2026-10-28-rust-lua-file-payloads-validation.json) passes 815
tests on Windows x64 and 816 on Linux x64 and i686 SSE2, strict Clippy,
formatting, checks without default features and an empty normal/build dependency
tree. The [first Windows run](2026-10-28-rust-lua-file-payloads-validation-first.json)
retains a test-runner race: a parallel no-default-feature build replaced the CLI
that three native tests launched. The portable build now uses an isolated target
directory. Only the affected Windows test command was rerun; source hashes were
unchanged and the other completed checks were retained. All
[twelve portable regressions](2026-10-28-rust-lua-file-payloads-portable.json)
pass in the isolated directory.

The [library audit](2026-10-28-rust-lua-file-payloads-stdlib.json) matches 5,507
cases. The [native audit](2026-10-28-rust-lua-file-payloads-boundary-current.json)
retains its balanced operation, state and library guards. Both native-enabled
i686 CLIs pass unchanged `main.lua` and `all.lua` in the
[stock suite](2026-10-28-rust-lua-file-payloads-i686-suite.json); T/testC remains
absent. [Windows](2026-10-28-rust-lua-file-payloads-windows-suite.json) retains
17/18 independent passes in all four modes and the common `files.lua:59` failure.

The [i686 verification](2026-10-28-rust-lua-file-payloads-i686-verification.json)
checks ELF32 width, artifact/source hashes, assertion recipes and both corpora.
The [evidence manifest](2026-10-28-rust-lua-file-payloads-evidence.json) binds the
current sources, references, binaries, reports and logs and embeds the audit
tools. JSON files retain their existing 2026-10-28 run identifiers; this checkpoint
was completed on 2026-10-29.

Full Lua 5.1 parity remains unfinished. Standard-stream payloads and shared C/Rust
buffering, the four core lifecycle/allocator APIs, exact allocation/heap and
collector accounting, strict callback continuations, diagnostics and resource/OOM
behavior remain open. The separate
[hook-yield mismatch](2026-10-28-rust-lua-io-closers-hook-yield.json) is retained.
