# Initial Lua 5.1 I/O and registry validation

The independent engine now provides the Lua 5.1 I/O namespace and file methods:
named/temporary files, input/output defaults, process pipes, reads, writes, seeks,
buffering, close/type queries and line iterators. File userdata uses the existing
object identity, environment, metatable, weak-table and finalizer machinery.
Operating-system handles stay in a flat generation-checked stream arena. Lua values
contain numeric identities. All engine code remains safe Rust with no normal/build
dependencies; the C interpreter is development-only.

Streams buffer file reads/writes and retain decoded lookahead. Line iterators hold
their userdata and two native upvalues, closing files they created when EOF is
reached. Native I/O methods share the appropriate private/default/pipe-close
environments. Lua 5.1 writes return booleans, and read failures preserve result
arity and stop before later formats are validated. Number scanning leaves the
first nonnumeric byte available to the next read. The hexadecimal converter rounds
directly to binary64, including subnormals, and reproduces the pinned UCRT scanner's
prefix-width-dependent carry behavior at the normal/subnormal boundary.

Windows text streams translate CRLF/LF and Ctrl+Z. Buffered position cookies account
for translated newlines and discarded Ctrl+Z tails rather than assuming logical
character counts are physical offsets. Update-mode opening handles trailing Ctrl+Z.
The reference treats line buffering like full buffering for regular Windows files.
`print` and standard-output I/O share Rust's stdout ordering and emit translated
newlines in the Windows CLI. Host output capture retains logical bytes, including
embedded NULs written through I/O.

`debug.getregistry` returns a shared, rooted Lua table. Its `FILE*` entry controls
file type checks and new-file metatables; `_LOADED` records the builtin modules.
The registry also correctly retains the original globals through builtin
environments. The old stale-native fixture was changed to remove a native function's
binding and give it an otherwise unreachable environment; replacing only the
thread globals no longer makes that environment unreachable. The host-userdata
test now measures against the initial standard-userdata count, and the Windows
shutdown-output fixture expects CRT-style CRLF.

Public native function IDs occupy the range below 256. Internal continuation IDs
start at 256, and generation-bearing native closures remain above 2^32. This keeps
I/O functions visible to hooks/debug queries while preserving hidden VM frames.
Values remain 16 bytes and frames 64 bytes on this target. Heap estimates include
stream records and their buffers; `HeapStats.files` counts live records, including
closed file userdata. Closed records are reclaimed with their userdata, potentially
on the cycle after finalization.

All 170 tests pass: 156 integration groups, thirteen unit groups and a runnable
documentation example. Formatting, strict Clippy and the zero normal/build dependency
gate pass. Twenty-two new integration groups cover:

- Userdata identity, file metatables, close environments, defaults and iterator
  upvalue counts.
- Byte writes, binary reads, seeks, multiple results, zero-length reads and EOF.
- Lua 5.1's `fgets`/`strlen` behavior around embedded NUL bytes and buffer boundaries.
- Decimal/hexadecimal number scanning, infinities, NaNs, malformed prefixes and
  unconsumed suffixes. A generated corpus checks 330 streams, comparing finite/
  infinite value bits, signed zero and remaining bytes; NaN payloads are normalized.
- Shared default input/output environments and restoration after redirection.
- Iterator roots, repeated EOF calls, appended data and seek/peek interactions.
- Standard-stream close restrictions and closed-file method failures.
- Buffering visibility through a second handle, partial read validation and file
  position changes.
- Named file modes, append behavior, NUL-terminated path/mode prefixes and automatic
  iterator close.
- Windows text conversion, Ctrl+Z, buffered seek cookies and update-mode truncation.
- Access/open failures and result tuples; unspecified/stale CRT error text after
  invalid access-mode operations is not treated as a stable diagnostic contract.
- Pipe reads/writes, flush/EOF/wait order and successful close across child exit codes.
- Weak values, recursive collection and file finalization exactly once.
- Ten batches of 100 temporary files, followed by collection: three standard file
  records remain, with reserved-capacity estimates below 2 MB.
- Logical stdout capture and a real child CLI using piped stdin/stdout.
- Registry identity, rooted values, builtin module entries and mutable file tags.

The development oracle is vendored official Lua 5.1.5 on Windows x86-64 MSVC,
static CRT and C locale. Tests compare scalar bits/bytes and arity through plain
`lua_pcall`, without a host error handler below the chunk. The large numeric corpus
reserves conversion stack space before creating host values. Resource/heap and CLI
capture checks are engine-only. Owned named fixtures stay under `target`; temporary
handles use exclusive creation and automatic OS cleanup.

Compatibility remains incomplete. Windows paths currently use UTF-8 conversion;
the differential path guarantee covers ASCII. Exact standard-stream buffering,
additional devices and OS errors, native scratch-stack layouts, full diagnostics,
CRT mode extensions and malformed native-userdata internals remain open. Blocking
reads and child waits do not observe the instruction quota. Registry C-private
entries, remaining libraries, binary chunks, incremental collection pauses and
later dialects remain unfinished. The package loader is not implemented by the
presence of `_LOADED`.

```sh
cargo test -p deadlib-lua
cargo fmt -p deadlib-lua -- --check
cargo clippy -p deadlib-lua --all-targets --no-deps -- -D warnings
cargo tree -p deadlib-lua --edges normal,build
python scripts/check_rust_lua_suite.py --output docs/testing/rust-lua-io-suite.json
```

The [official report](2026-10-25-rust-lua-io-suite.json) retains sixteen unchanged
files, archive/source checksums, `_U=true`, a 100-million-instruction budget and
60-second per-file deadlines. Rust passes nine: `constructs.lua`, `locals.lua`,
`nextvar.lua`, `vararg.lua`, `pm.lua`, `events.lua`, `gc.lua`, `closure.lua` and now
`db.lua`. Fifteen reference probes pass. The newly included `files.lua` fails in
the Windows reference at line 59 when renaming the still-open default input file;
this platform failure is retained rather than changing the assertion. Its initial
absolute-path write check is guarded by verifying that the expected missing
parent directory is absent before the probe runs.

| Remaining file | Rust stopping point | Reference |
| --- | --- | --- |
| `calls.lua` | `string.dump`, line 243 | Pass |
| `literals.lua` | `os.setlocale`, line 161 | Pass |
| `strings.lua` | `os.setlocale`, line 150 | Pass |
| `sort.lua` | `os.clock`, line 25 | Pass |
| `math.lua` | `require "checktable"`, line 162 | Pass |
| `errors.lua` | Syntax-message assertion in `checksyntax`, line 20 | Pass |
| `files.lua` | `os.tmpname`, line 19 | Windows rename failure, line 59 |

These are independent probes, not full `all.lua` or C API coverage. Production
continues to use the preserved legacy engine. See the [paired measurements](../performance/2026-10-25-rust-lua-io.md).

Reference behavior was checked against the vendored I/O source and the installed
Microsoft Windows SDK UCRT sources. Relevant public documentation describes
[MSVC file translation](https://learn.microsoft.com/en-us/cpp/c-runtime-library/file-translation-constants?view=msvc-170)
and [Rust Windows open options](https://doc.rust-lang.org/stable/std/os/windows/fs/trait.OpenOptionsExt.html).
