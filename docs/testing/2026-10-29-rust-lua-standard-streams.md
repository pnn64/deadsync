# Lua 5.1 native standard streams

With the C feature enabled, stdin, stdout and stderr now use their selected C
runtime's `FILE` objects and buffers. The VM adopts them before its first I/O
operation or C activation and fills existing standard userdata payloads. Bare
states can obtain stdin before opening I/O. `print`, stdin file loading,
interactive-console input, prompts and debugger diagnostics use those streams.

The original kernel streams have persistent records. Each Lua `FILE **` wrapper
retains its own pointer state, so clearing or replacing one wrapper does not
alter the pointers in other wrappers or the stream used by `print` and stdin
loading. Reopening I/O creates fresh userdata while its C `FILE` buffer retains
unread input. Standard userdata still uses the normal no-close handler.

`print` follows Lua 5.1's error behavior by ignoring stdio write failures. Its
string-conversion errors remain errors, and `io.write` returns its failure
tuple. Host output capture retains print/I/O ordering and binary bytes. Without
the C feature, the existing Rust standard-stream implementation remains available.

The [preserved previous milestone](2026-10-29-rust-lua-standard-streams-baseline.json),
`5dec4513b`, exposes a NULL stdin payload after Rust library initialization.
The new corpus starts with a partial Lua read before a C callback is activated,
then reads through C stdio and the userdata pointer. A counter assertion in the
Rust embedding verifies that the early read creates no boundary state. The same
corpus also runs after public C library initialization.

Each initialization style matches 13 observations against release and checked
Lua 5.1.5 references. The cases cover:

- Standard pointer identity and pointer-formatted strings.
- Alternating C/Lua stdin reads after read-ahead, plus I/O reopening.
- Loading original stdin while the current stdin userdata is cleared, followed
  by pointer restoration.
- Shared stdout/stderr buffers under unbuffered, full and line buffering.
- `print` using original stdout while `io.output` names another file or the
  stdout userdata is cleared.
- Debugger prompts through stderr and reads through original stdin.
- Ignored `print` failures and reported `io.write` errors on read-only stdout.

The public-header C fixture temporarily redirects descriptors to files, flushes
and restores them, and compares the captured bytes. On Windows it restores the
process standard handles too. Its factory and stdio operations share one image
so static-CRT ownership stays valid. Pinned PUC sources remain unchanged, and
checked references force-enable assertions.

Reports cover Rust initialization on
[Windows debug](2026-10-29-rust-lua-standard-streams-focused.json),
[Windows release](2026-10-29-rust-lua-standard-streams-current.json) and
[i686 release](2026-10-29-rust-lua-standard-streams-i686-current.json), and public
C initialization on
[Windows debug](2026-10-29-rust-lua-standard-streams-focused-c.json),
[Windows release](2026-10-29-rust-lua-standard-streams-current-c.json) and
[i686 release](2026-10-29-rust-lua-standard-streams-i686-current-c.json).
Both initialization styles produce the same transcript within each platform.
The prior 26-observation file-constructor and 47-observation native-file corpora
also match on all three profiles, bringing this checkpoint to 297 differential
observations. Separate exit probes cover buffered C/Lua output, binary stdout
mode and a cleared stdout userdata. All 36 reference/Rust terminating processes
match their output and exit status 7.

[Full validation](2026-10-29-rust-lua-standard-streams-validation.json) passes
815 tests on Windows x64 and 816 on Linux x64 and i686 SSE2, strict Clippy,
formatting, all-target checks without default features and an empty normal/build
dependency tree. The
[twelve portable regressions](2026-10-29-rust-lua-standard-streams-portable.json)
use an isolated no-default-feature target directory. The
[library audit](2026-10-29-rust-lua-standard-streams-stdlib.json) matches 5,507
cases, and the [native audit](2026-10-29-rust-lua-standard-streams-boundary-current.json)
retains balanced dispatch, state and library lifetimes.

Both native-enabled i686 CLIs pass unchanged `main.lua` and `all.lua` in the
[stock suite](2026-10-29-rust-lua-standard-streams-i686-suite.json); T/testC remains
absent. [Windows](2026-10-29-rust-lua-standard-streams-windows-suite.json) retains
17/18 independent passes in all four modes and the common `files.lua:59` failure.
The [i686 verifier](2026-10-29-rust-lua-standard-streams-i686-verification.json)
checks ELF32 width, current artifacts/sources, reference assertions and all four
corpus variants. The
[evidence manifest](2026-10-29-rust-lua-standard-streams-evidence.json) binds the
current sources, references, binaries, reports and logs and embeds the audit tools.

Full Lua 5.1 parity remains unfinished. The four core lifecycle/allocator APIs,
exact allocation/heap and collector accounting, arbitrary strict continuations,
broader diagnostics and resource/OOM behavior remain open. The separate
[hook-yield mismatch](2026-10-28-rust-lua-io-closers-hook-yield.json) is retained;
the recorded stdio cases do not prove arbitrary native-file interleavings.
