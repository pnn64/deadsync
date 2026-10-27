# Lua 5.1 text file loading validation

`loadfile` compiles a named text file or underlying standard input and returns a
function, or nil plus an error. `dofile` loads and executes the chunk with no
arguments, returning all results and propagating arbitrary runtime errors.
Both use the existing resumable scanner/parser, register compiler, verifier,
thread globals and stream backend. Binary chunks remain unsupported.

File readers use generation-checked arena keys. Active loading jobs trace their
streams across collection and coroutine switches. Named readers are removed and
closed before the loaded function executes, or when parsing fails or an
instruction abort discards the job. They create no Lua-visible file userdata.
`HeapStats.files` includes these temporary readers as well as userdata records.

The standard-input stream record stays rooted independently of its Lua userdata.
Loading shares decoded lookahead with I/O reads, even after the Lua stdin value
becomes unreachable. Redirecting `io.input` does not redirect `loadfile()`.
The file source name is `@filename`; underlying stdin uses `=stdin`. A leading
`#` line is skipped and replaced by a newline so later line numbers are retained.
Windows text translation and Ctrl+Z follow the existing stream implementation.

File acquisition and parsing resume in bounded input chunks, including long
executable headers. The global instruction quota covers these steps and child
execution. Blocking OS reads/open calls and the final compiler pass are not
preempted by that quota. `dofile` executes through the existing flat scheduler,
retaining its original arguments for local inspection and collection. C-call
yield restrictions, error handlers and hook entry changes remain observable.

The parser now starts with a 200-level syntax budget and deducts active native
call depth for reader/file/string loads. Deep file recursion therefore reaches
the reference's parser budget before a plain C stack overflow. Syntax source
descriptions use the lexer's 80-byte buffer; debug/runtime descriptions retain
their separate 60-byte buffer. The compiler's independent 120-level expression
bound, assignment-depth edge cases and full diagnostics still need parity work.

All 185 tests pass: 171 integration groups, thirteen unit groups and one runnable
documentation example. Formatting, strict Clippy and the zero normal/build
dependency gate pass. Fifteen new integration groups cover:

- Named loads, nil/false/multiple results, zero argument execution, varargs,
  function metadata, environments and ignored extra arguments.
- Empty files, initial hash lines, CR/LF boundaries, quoted/long strings,
  embedded NULs, Ctrl+Z and Lua 5.1's rejection of a UTF-8 BOM.
- Missing files, NUL-terminated filenames, numeric filename conversion, bad
  argument types and protected error-handler order.
- Thread-global inheritance independently of native/function environments.
- Original `dofile` argument slots, local mutation and roots during collection.
- Arbitrary runtime error identity and preservation of faulting frames.
- Reloading changed files and retaining closures after their source changes.
- Nested file execution, parser/C-call depth and prohibited direct yields.
- Call/return hooks, including changing the filename in a call hook.
- Input/header boundaries at 510/511/512/513/4,095/4,096/8,191 bytes, a 100 KB
  comment and 500 function literals under collection pressure.
- Thirty main-thread quota aborts and one coroutine abort, followed by handle
  deletion and successful VM recovery.
- One thousand successful reloads, escaped closures and bounded reusable
  storage: three standard file records remain and estimates stay below 2 MB.
- Forty-nine generated filename lengths comparing lexer source descriptions
  through string and reader loads.
- Rust host error messages matching the actual raised file-error string.
- Six stdin scenarios, each run in separate Rust/reference child processes:
  buffered preceding reads, nil filename/extra arguments, default-input
  redirection and stdin-userdata collection, early-error remaining bytes,
  long executable headers and empty input.

The quota/storage and Rust host-message groups are engine-only. Other new groups
compare scalar bits, byte strings, result arity and identity predicates with
vendored official Lua 5.1.5 on Windows x86-64 MSVC, static CRT and C locale. A plain
`lua_pcall` entry avoids a host error-handler slot below the chunk. Named fixtures
are owned files under `target`, and child stdin tests use piped input.

In-process I/O oracle cases are serialized. The Windows CRT creates inheritable
`fopen` handles, and its `_popen` enables handle inheritance. Concurrent oracle
states could leave an otherwise closed fixture open in another case's child,
causing a Windows sharing violation during deletion. Assertions and cleanup stay
intact; serialization covers the oracle execution and state destruction.

Compatibility remains incomplete. Binary chunks, syntax message/token details,
compiler/resource bounds, native/parser scratch slots and precise GC interruption
timing remain open. Windows filename conversion retains the existing UTF-8/ASCII
reference scope. Exact buffering across independent VMs sharing stdin, additional
devices/errors and interruption of blocking I/O need more work. The CLI's complete
Lua option/argument/file behavior, remaining libraries, persistent caches,
incremental collection and later dialects are also unfinished.

```sh
cargo test -p deadlib-lua
cargo fmt -p deadlib-lua -- --check
cargo clippy -p deadlib-lua --all-targets --no-deps -- -D warnings
cargo tree -p deadlib-lua --edges normal,build
python scripts/check_rust_lua_suite.py --output docs/testing/rust-lua-file-load-suite.json
```

The [official report](2026-10-25-rust-lua-file-load-suite.json) retains sixteen
unchanged probes, the pinned archive checksum, `_U=true`, a 100-million-instruction
budget and 60-second per-file deadlines. Rust passes nine: `constructs.lua`,
`locals.lua`, `nextvar.lua`, `vararg.lua`, `pm.lua`, `events.lua`, `gc.lua`,
`closure.lua` and `db.lua`. Fifteen reference probes pass. The retained
`files.lua` reference failure is the Windows rename of the still-open default
input file at line 59. Rust reaches missing `os.tmpname` at line 19.

| Remaining probe | Rust stopping point |
| --- | --- |
| `calls.lua` | `string.dump`, line 243 |
| `literals.lua` | `os.setlocale`, line 161 |
| `strings.lua` | `os.setlocale`, line 150 |
| `sort.lua` | `os.clock`, line 25 |
| `math.lua` | `require "checktable"`, line 162 |
| `errors.lua` | Syntax-message assertion, line 20 |
| `files.lua` | `os.tmpname`, line 19 |

These are independent probes, not complete `all.lua` or C API coverage. Production
uses the preserved legacy engine. The
[paired measurements](../performance/2026-10-25-rust-lua-file-load.md) include
28 runtime workloads and eight frontend cases.

Behavior was checked against the vendored official `lauxlib.c`, `lbaselib.c`,
`llex.c` and `lparser.c`, plus the installed Windows SDK UCRT I/O sources. The
[Lua 5.1 manual](https://www.lua.org/manual/5.1/manual.html#pdf-loadfile) documents
the file-loading API and standard-input fallback.
