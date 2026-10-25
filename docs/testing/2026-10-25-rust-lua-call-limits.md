# Lua 5.1 call limits and protected recovery

Lua recursion overflow is a catchable runtime error. The engine models the pinned
reference's logical call-info capacity: eight initial slots, doubling growth, a
20,000 normal limit and oversized capacity for error handling. Because growth
crosses the limit, the first overflow and later recovered overflows can occur at
different depths. Internal continuation frames consume no Lua call slots.

A `u16` capacity travels with each thread. An inline physical-length check avoids
counting visible frames on ordinary calls; the cold path counts them near the
boundary. Lua tail calls check the temporary callee slot before reusing the frame.
Protected unwinding restores the normal logical limit when enough frames have
been released. GC shrinks underused logical capacity during thread marking and
the gray-again pass; finalizer-preserved threads first reached later are visited
once. Backing vectors keep their independent reusable capacities.

Native-to-Lua callbacks check the C-call depth on their own entry path. The normal
overflow and emergency exhaustion rules are distinct: handlers can continue in
the emergency range, while exhaustion produces the reference's plain
`error in error handling` object and bypasses further handler invocation. Error
location is added for Lua callers and omitted for native callers where required.
Runtime errors in an `xpcall` handler can invoke that handler again; faulting frames
stay available until the eventual protected recovery. Failed handler entry can
itself raise an error that the handler processes using the reserve. The scheduler
handles this without recursive interpreter entry.

Instruction budgets charge handler-entry attempts and remain uncatchable. Host
errors and quota aborts release reserved contexts so later executions recover.
Coroutines preserve independent capacities across resumes and retain failed
frames for inspection. Surviving captured cells close before unwinding frees
their registers. Values remain 16 bytes and frames 64 bytes on this target.

All 259 tests pass: 245 integration groups, thirteen unit groups and one runnable
documentation example. Sixteen new groups execute 47 paired reference programs:

| Corpus | Paired programs |
| --- | ---: |
| First and repeated overflow counts/messages | 1 |
| Nested protected calls in the overflow reserve | 1 |
| Overflow while handling overflow | 1 |
| Deep pre-unwind tracebacks | 1 |
| Explicit GC and logical-capacity shrinking | 6 |
| Coroutine failure, yields and resumed recovery | 2 |
| Surviving captured locals and collection | 1 |
| Fifty thousand ordinary tail calls | 1 |
| Near-capacity Lua/native/protected calls and error precedence | 20 |
| Handler re-entry and failed handler entry | 2 |
| Recursive runtime-error handlers reaching the C emergency limit | 1 |
| Nested `pcall`/`xpcall` C-boundary depth | 2 |
| Metatable, tostring, table and import callbacks in the emergency range | 4 |
| Host overflow followed by matched recovery | 1 |
| Instruction abort in a handler followed by matched recovery | 1 |
| Deep call hooks and fifty thousand hooked tail calls | 2 |

The host-error and quota groups additionally assert engine-only API behavior;
their later recovery counts are compared with a reference state warmed by a
protected overflow. Other observations compare scalar number bits, booleans,
nil, arity and complete error/traceback bytes. Stopped collection makes recursion
counts deterministic; explicit collection cases stop it again after each full
cycle. This does not establish identical automatic collection scheduling.

The oracle is vendored official Lua 5.1.5 on Windows x86-64 MSVC, static CRT and
C locale. Fixed developer inputs run under Lua protection. Formatting, strict
Clippy and the zero normal/build dependency gate pass. Engine code remains safe
Rust. The [paired measurements](../performance/2026-10-25-rust-lua-call-limits.md)
include the new overflow workload and retained-memory estimates.

```sh
cargo test -p deadlib-lua
cargo fmt -p deadlib-lua -- --check
cargo clippy -p deadlib-lua --all-targets --no-deps -- -D warnings
cargo tree -p deadlib-lua --edges normal,build
python scripts/check_rust_lua_suite.py --output docs/testing/rust-lua-call-limits-suite.json
```

The [official report](2026-10-25-rust-lua-call-limits-suite.json) retains sixteen
unchanged probes, the pinned archive checksum, `_U=true`, a 100-million-instruction
budget and 60-second per-file deadlines. Rust still passes ten: `constructs.lua`,
`locals.lua`, `nextvar.lua`, `vararg.lua`, `pm.lua`, `events.lua`, `gc.lua`,
`closure.lua`, `db.lua` and `math.lua`. Fifteen reference probes pass.

`errors.lua` now passes the recursive overflow, traceback/recovery, error-handler
and syntax-depth assertions. It stops at line 238, where the upvalue-limit error
must identify the owning function at line 3. Wider compiler limit diagnostics
remain required work.

| Remaining probe | Rust stopping point |
| --- | --- |
| `calls.lua` | `string.dump`, line 243 |
| `literals.lua` | `os.setlocale`, line 161 |
| `strings.lua` | `os.setlocale`, line 150 |
| `sort.lua` | `os.clock`, line 25 |
| `errors.lua` | Upvalue-limit diagnostic assertion, line 238 |
| `files.lua` | `os.tmpname`, line 19 |

The reference `files.lua` failure remains the Windows rename of a still-open
default input file at line 59. Independent probes are not complete `all.lua` or
C API coverage. Wider native/parser/resource boundaries, debug/scratch behavior,
allocation-failure behavior and incremental-GC parity remain open. Binary chunks,
remaining OS/base functions, native C modules, persistent caches, later dialects
and gameplay integration also remain unfinished. Production uses the preserved
legacy engine.
